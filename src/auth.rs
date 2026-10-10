use crate::{
    credentials::Store,
    error::{Error, Result},
    logging::Log,
    route::Route,
};
use rmcp::transport::auth::{
    AuthorizationManager, AuthorizationRequest, AuthorizationSession, OAuthHttpClient,
    OAuthHttpClientError, OAuthHttpClientFuture, OAuthHttpRedirectPolicy, OAuthHttpRequest,
};
use std::{sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use url::Url;

#[derive(Clone)]
pub struct OAuthRoute {
    route: Route,
    follow: reqwest::Client,
    stop: reqwest::Client,
}
impl OAuthRoute {
    pub fn new(route: Route) -> Result<Self> {
        Ok(Self {
            follow: route.client(true, Some(Duration::from_secs(30)))?,
            stop: route.client(false, Some(Duration::from_secs(30)))?,
            route,
        })
    }
}
impl OAuthHttpClient for OAuthRoute {
    fn execute(&self, operation: OAuthHttpRequest) -> OAuthHttpClientFuture<'_> {
        Box::pin(async move {
            self.route
                .check()
                .map_err(|e| Box::new(e) as OAuthHttpClientError)?;
            let client = if operation.redirect_policy == OAuthHttpRedirectPolicy::Stop {
                &self.stop
            } else {
                &self.follow
            };
            let request = reqwest::Request::try_from(operation.request)
                .map_err(|e| Box::new(e) as OAuthHttpClientError)?;
            let result=tokio::select! {
                _=self.route.changed()=>return Err(Box::new(Error::Safe("Route changed; OAuth stopped.")) as OAuthHttpClientError),
                result=client.execute(request)=>result,
            }.map_err(|e|Box::new(e) as OAuthHttpClientError)?;
            let mut response = http::Response::builder()
                .status(result.status())
                .version(result.version());
            for (name, value) in result.headers() {
                response = response.header(name, value);
            }
            let mut result = result;
            let mut body = Vec::new();
            while let Some(chunk) = result
                .chunk()
                .await
                .map_err(|e| Box::new(e) as OAuthHttpClientError)?
            {
                if body.len() + chunk.len() > 1024 * 1024 {
                    return Err(Box::new(Error::Safe("OAuth response too large.")));
                }
                body.extend_from_slice(&chunk);
            }
            self.route
                .check()
                .map_err(|e| Box::new(e) as OAuthHttpClientError)?;
            response
                .body(body)
                .map_err(|e| Box::new(e) as OAuthHttpClientError)
        })
    }
}
pub async fn manager(route: Route, store: Store, endpoint: &str) -> Result<AuthorizationManager> {
    let mut manager = AuthorizationManager::new_with_oauth_http_client(
        endpoint,
        Arc::new(OAuthRoute::new(route)?),
    )
    .await?;
    manager.set_credential_store(store);
    Ok(manager)
}
pub async fn restore(manager: &mut AuthorizationManager, store: &Store) -> Result<()> {
    if !manager.initialize_from_store().await? {
        return Err(Error::Safe(
            "Upwork sign-in required. Open the menu and choose Sign in.",
        ));
    }
    // Restore only registration belonging to the stored token, never another legacy client.
    if let Some(client) = store.client_registration()?
        && client.get("client_id").and_then(serde_json::Value::as_str)
            == store.load_local()?.as_ref().map(|c| c.client_id.as_str())
        && let Some(secret) = client
            .get("client_secret")
            .and_then(serde_json::Value::as_str)
    {
        let id = client
            .get("client_id")
            .and_then(serde_json::Value::as_str)
            .ok_or(Error::Safe("Invalid saved client."))?;
        manager.configure_client(
            rmcp::transport::auth::OAuthClientConfig::new(id, crate::ENDPOINT)
                .with_client_secret(secret),
        )?;
    }
    Ok(())
}
pub async fn sign_in(
    route: Route,
    store: Store,
    endpoint: &str,
    log: &Log,
) -> Result<AuthorizationManager> {
    let mut manager = manager(route.clone(), store.clone(), endpoint).await?;
    let resolution = manager.resolve_metadata().await?;
    manager.set_metadata(resolution.metadata);
    let mut client = store.client_registration()?;
    let redirect = client
        .as_ref()
        .and_then(|c| c.get("redirect_uris"))
        .and_then(|v| v.as_array())
        .and_then(|v| v.first())
        .and_then(|v| v.as_str())
        .map(str::to_owned);
    let port = redirect
        .as_ref()
        .and_then(|s| Url::parse(s).ok())
        .and_then(|u| u.port())
        .unwrap_or(0);
    let listener = TcpListener::bind(("127.0.0.1", port)).await?;
    let redirect = redirect.unwrap_or(format!(
        "http://127.0.0.1:{}/oauth/callback",
        listener.local_addr()?.port()
    ));
    let target = Url::parse(&redirect).map_err(|_| Error::Safe("Invalid local callback."))?;
    if target.scheme() != "http" || !matches!(target.host_str(), Some("localhost" | "127.0.0.1")) {
        return Err(Error::Safe("Callback must be local to this computer."));
    }
    if client.is_none() {
        let config = manager
            .register_client("Upwork Native Connector", &redirect, &[])
            .await?;
        store.save_registration(&config)?;
        client = store.client_registration()?;
    }
    let mut request =
        AuthorizationRequest::new(&redirect).with_client_name("Upwork Native Connector");
    if let Some(id) = client
        .as_ref()
        .and_then(|v| v.get("client_id"))
        .and_then(|v| v.as_str())
    {
        request = request.with_preregistered_client(id);
    }
    if let Some(secret) = client
        .as_ref()
        .and_then(|v| v.get("client_secret"))
        .and_then(|v| v.as_str())
    {
        request = request.with_client_secret(secret);
    }
    let session = AuthorizationSession::new(manager, request)
        .await
        .map_err(|(_, e)| Error::Auth(e))?;
    let url = session.get_authorization_url();
    crate::callback::authorization_url(url)?;
    println!(
        "Copy this complete link into your browser on this computer:\n{url}\n\nKeep this terminal open until the session is saved."
    );
    log.event(
        "INFO",
        "Authorization link shown to human; waiting for callback.",
    );
    tokio::time::timeout(Duration::from_secs(300),async {
        loop {
            let (mut socket,_)=listener.accept().await?;
            let mut bytes=vec![]; let mut byte=[0];
            while !bytes.ends_with(b"\r\n\r\n") && bytes.len()<8192 {
                if !matches!(tokio::time::timeout(Duration::from_secs(5),socket.read_exact(&mut byte)).await,Ok(Ok(_))){break;}
                bytes.push(byte[0]);
            }
            let params=crate::callback::parameters(&bytes,&target);
            let result=if let Ok(params)=params {
                if let (Some(code),Some(state))=(params.get("code"),params.get("state")) {
                    session.handle_callback_with_issuer(code,state,params.get("iss").map(String::as_str)).await.map(|_|())
                } else {Err(rmcp::transport::auth::AuthError::InternalError("Invalid callback.".into()))}
            } else {Err(rmcp::transport::auth::AuthError::InternalError("Invalid callback.".into()))};
            let success=result.is_ok();
            let body=if success {"Callback received. Wait for the terminal to verify the session."}else{"Authorization callback rejected. Return to the sign-in terminal."};
            let response=format!("HTTP/1.1 {}\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",if success{"200 OK"}else{"400 Bad Request"},body.len());
            let _=socket.write_all(response.as_bytes()).await;
            if success {return Ok::<(),Error>(());}
            log.event("WARNING","Authorization callback rejected; private parameters suppressed.");
        }
    }).await.map_err(|_|Error::Safe("Sign-in timed out. Run Sign in for a new link."))??;
    Ok(session.auth_manager)
}
