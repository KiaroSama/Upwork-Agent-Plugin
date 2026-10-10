mod support;
use rmcp::transport::{
    StreamableHttpClientTransport, Transport,
    auth::{AuthorizationRequest, AuthorizationSession, CredentialStore, OAuthClientConfig},
    streamable_http_client::{StreamableHttpClient, StreamableHttpClientTransportConfig},
};
use std::{collections::HashMap, sync::atomic::Ordering, time::Duration};
use support::*;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use upwork_connector::{
    auth, bridge,
    config::{self, Mode},
    credentials::Store,
};

#[tokio::test]
async fn proxy_protocols_credentials_remote_dns_and_refusal() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let f = fixture().await;
        let cert =
            reqwest::Certificate::from_pem(include_bytes!("fixtures/localhost-test.crt")).unwrap();
        for (scheme, addr) in [
            ("http", f.http),
            ("https", f.https),
            ("socks5", f.socks),
            ("socks5h", f.socks),
        ] {
            let dir = temp();
            let r = route(
                dir.path(),
                format!(
                    "{scheme}://fixture-user:fixture-pass@localhost:{}",
                    addr.port()
                ),
            );
            let client = r
                .builder(true, Some(Duration::from_secs(2)))
                .unwrap()
                .tls_certs_merge([cert.clone()])
                .build()
                .unwrap();
            let response = client
                .get(format!(
                    "http://unresolvable.invalid:{}/redirect",
                    f.target.port()
                ))
                .send()
                .await
                .unwrap();
            assert_eq!(response.text().await.unwrap(), "routed");
            let secure = client
                .get(format!("https://localhost:{}/", f.secure_target.port()))
                .send()
                .await
                .unwrap();
            assert_eq!(secure.text().await.unwrap(), "secure");
            f.observed.refusal.store(true, Ordering::SeqCst);
            let count = f.observed.targets.load(Ordering::SeqCst);
            let refused = r
                .builder(false, Some(Duration::from_secs(2)))
                .unwrap()
                .tls_certs_merge([cert.clone()])
                .build()
                .unwrap()
                .get(format!("http://127.0.0.1:{}/", f.target.port()))
                .send()
                .await;
            // HTTP forward proxies return 407 as a response; SOCKS/CONNECT fail the handshake.
            assert!(
                refused
                    .map(|response| response.status()
                        == reqwest::StatusCode::PROXY_AUTHENTICATION_REQUIRED)
                    .unwrap_or(true)
            );
            assert_eq!(f.observed.targets.load(Ordering::SeqCst), count);
            f.observed.refusal.store(false, Ordering::SeqCst);
        }
        assert!(
            f.observed
                .domains
                .lock()
                .unwrap()
                .iter()
                .any(|s| s.contains("unresolvable.invalid"))
        );
        assert!(f.observed.auth.lock().unwrap().len() >= 4);
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn direct_is_explicit_and_route_changes_invalidate_clients() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let f = fixture().await;
        let dir = temp();
        let r = direct(dir.path());
        assert_eq!(
            r.client(false, Some(Duration::from_secs(2)))
                .unwrap()
                .get(format!("http://{}/", f.target))
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "routed"
        );
        config::update(&dir.path().join("config/connections.json"), |c| {
            c.mode = Mode::Blocked;
            Ok(())
        })
        .unwrap();
        assert!(r.check().is_err());
        r.changed().await;
        assert!(r.client(false, None).is_err());
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn oauth_exchange_refresh_and_rejected_callback_use_selected_proxy() {
    tokio::time::timeout(Duration::from_secs(10), async {
        let f = fixture().await;
        let dir = temp();
        let route = route(dir.path(), format!("http://{}", f.http));
        let store = Store {
            root: dir.path().into(),
        };
        let mut manager = auth::manager(
            route.clone(),
            store.clone(),
            &format!("http://{}/mcp", f.target),
        )
        .await
        .unwrap();
        let metadata = manager.resolve_metadata().await.unwrap();
        manager.set_metadata(metadata.metadata);
        let session = AuthorizationSession::new(
            manager,
            AuthorizationRequest::new("http://localhost:1/callback")
                .with_preregistered_client("fixture-client"),
        )
        .await
        .map_err(|(_, e)| e)
        .unwrap();
        let url = url::Url::parse(session.get_authorization_url()).unwrap();
        let params = url.query_pairs().into_owned().collect::<HashMap<_, _>>();
        assert!(params.contains_key("code_challenge"));
        assert_eq!(
            params.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(
            session
                .handle_callback("fixture-code", "wrong-state")
                .await
                .is_err()
        );
        assert_eq!(f.observed.tokens.load(Ordering::SeqCst), 0);
        assert!(
            session
                .handle_callback_with_issuer(
                    "fixture-code",
                    &params["state"],
                    Some("https://forged.example")
                )
                .await
                .is_err()
        );
        assert_eq!(f.observed.tokens.load(Ordering::SeqCst), 0);
        session
            .handle_callback_with_issuer(
                "fixture-code",
                &params["state"],
                Some(&format!("http://{}", f.target)),
            )
            .await
            .unwrap();
        assert_eq!(f.observed.tokens.load(Ordering::SeqCst), 1);
        assert!(store.path().exists());
        session.auth_manager.refresh_token().await.unwrap();
        assert_eq!(f.observed.tokens.load(Ordering::SeqCst), 2);
        f.observed.refusal.store(true, Ordering::SeqCst);
        // Use a new proxy connection: refusal of CONNECT cannot revoke an already-open tunnel.
        let mut refused = auth::manager(
            route.clone(),
            store.clone(),
            &format!("http://{}/mcp", f.target),
        )
        .await
        .unwrap();
        refused.set_metadata(
            session
                .auth_manager
                .resolve_metadata()
                .await
                .unwrap()
                .metadata,
        );
        refused
            .configure_client(OAuthClientConfig::new(
                "fixture-client",
                "http://localhost:1/callback",
            ))
            .unwrap();
        assert!(refused.refresh_token().await.is_err());
        assert_eq!(f.observed.tokens.load(Ordering::SeqCst), 2);
        assert!(!f.observed.domains.lock().unwrap().is_empty());
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn stdio_relay_preserves_initialize_and_tool_messages_then_stops_on_route_change() {
    tokio::time::timeout(Duration::from_secs(8),async {
        let f=fixture().await;let dir=temp();let route=route(dir.path(),format!("socks5://{}",f.socks));let client=route.client(false,None).unwrap();
        let (mut writer,input)=tokio::io::duplex(8192);let (output,reader)=tokio::io::duplex(8192);
        let endpoint=format!("http://unresolvable.invalid:{}/mcp",f.target.port());
        let task=tokio::spawn(async move{bridge::relay(client,route,&endpoint,input,output).await});
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"test\",\"version\":\"1\"}}}\n").await.unwrap();
        let mut reader=BufReader::new(reader);let mut line=String::new();reader.read_line(&mut line).await.unwrap();let v:serde_json::Value=serde_json::from_str(&line).unwrap();assert_eq!(v["id"],1);assert_eq!(v["result"]["serverInfo"]["name"],"local-fixture");
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"tools/list\"}\n").await.unwrap();line.clear();reader.read_line(&mut line).await.unwrap();let v:serde_json::Value=serde_json::from_str(&line).unwrap();assert_eq!(v["id"],2);assert_eq!(v["result"]["tools"],serde_json::json!([]));
        config::update(&dir.path().join("config/connections.json"),|c|{c.mode=Mode::Blocked;Ok(())}).unwrap();assert!(task.await.unwrap().is_err());
    }).await.unwrap();
}
#[tokio::test]
async fn routed_mcp_and_refresh_work_with_saved_native_credentials() {
    tokio::time::timeout(Duration::from_secs(8),async {
        let f=fixture().await;let dir=temp();let r=route(dir.path(),format!("http://{}",f.http));let store=Store{root:dir.path().into()};
        let tokens=serde_json::from_value(serde_json::json!({"access_token":"fixture-token","token_type":"Bearer","refresh_token":"fixture-refresh","expires_in":1})).unwrap();
        store.save(rmcp::transport::auth::StoredCredentials::new("fixture-client".into(),Some(tokens),vec![],Some(1))).await.unwrap();
        let mut m=auth::manager(r.clone(),store.clone(),&format!("http://{}/mcp",f.target)).await.unwrap();let metadata=m.resolve_metadata().await.unwrap();m.set_metadata(metadata.metadata);m.configure_client(OAuthClientConfig::new("fixture-client","http://localhost:1/callback")).unwrap();
        let token=m.get_access_token().await.unwrap();assert_eq!(token,"fixture-access");assert_eq!(f.observed.tokens.load(Ordering::SeqCst),1);
        let mut t=StreamableHttpClientTransport::with_client(rmcp::transport::AuthClient::new(r.client(false,None).unwrap(),m),StreamableHttpClientTransportConfig::with_uri(format!("http://{}/mcp",f.target)));
        let msg=serde_json::from_value(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}})).unwrap();let send=t.send(msg);let (sent,received)=tokio::join!(send,t.receive());sent.unwrap();assert!(received.is_some());t.close().await.unwrap();
    }).await.unwrap();
}

#[tokio::test]
async fn changed_route_blocks_mcp_post_get_and_cleanup_delete() {
    tokio::time::timeout(Duration::from_secs(5), async {
        let f = fixture().await;
        let dir = temp();
        let r = direct(dir.path());
        let client = upwork_connector::http_route::McpRoute::new(r).unwrap();
        config::update(&dir.path().join("config/connections.json"), |c| {
            c.mode = Mode::Blocked;
            Ok(())
        })
        .unwrap();
        let uri: std::sync::Arc<str> = format!("http://{}/mcp", f.target).into();
        let message: rmcp::model::ClientJsonRpcMessage =
            serde_json::from_value(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"ping"}))
                .unwrap();
        assert!(
            client
                .post_message(uri.clone(), message, None, None, HashMap::new())
                .await
                .is_err()
        );
        assert!(
            client
                .get_stream(uri.clone(), None, None, None, HashMap::new())
                .await
                .is_err()
        );
        assert!(
            client
                .delete_session(uri, "fixture-session".into(), None, HashMap::new())
                .await
                .is_err()
        );
        assert_eq!(f.observed.targets.load(Ordering::SeqCst), 0);
    })
    .await
    .unwrap();
}
#[tokio::test]
async fn native_registration_restores_matching_secret_not_unrelated_legacy() {
    tokio::time::timeout(Duration::from_secs(8),async{
        let f=fixture().await;let dir=temp();let store=Store{root:dir.path().into()};let r=route(dir.path(),format!("http://{}",f.http));
        let registration=OAuthClientConfig::new("native-client","http://localhost:1/callback").with_client_secret("fixture-secret");store.save_registration(&registration).unwrap();
        std::fs::create_dir_all(store.legacy_path("client_info").parent().unwrap()).unwrap();std::fs::write(store.legacy_path("client_info"),r#"{"client_id":"legacy-other","client_secret":"legacy-secret"}"#).unwrap();
        let tokens=serde_json::from_value(serde_json::json!({"access_token":"fixture-token","token_type":"Bearer","refresh_token":"fixture-refresh","expires_in":3600})).unwrap();
        store.save(rmcp::transport::auth::StoredCredentials::new("native-client".into(),Some(tokens),vec![],Some(1))).await.unwrap();
        let mut manager=auth::manager(r,store.clone(),&format!("http://{}/mcp",f.target)).await.unwrap();auth::restore(&mut manager,&store).await.unwrap();assert_eq!(manager.get_access_token().await.unwrap(),"fixture-access");assert_eq!(store.client_registration().unwrap().unwrap()["client_id"],"native-client");
    }).await.unwrap();
}
#[test]
fn legacy_windows_style_registration_cutover_preserves_other_servers() {
    let dir = temp();
    std::fs::create_dir(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/bridge.ts"),
        "test-only legacy presence",
    )
    .unwrap();
    let legacy = std::fs::canonicalize(dir.path().join("src/bridge.ts")).unwrap();
    std::fs::write(dir.path().join(".mcp.json"),serde_json::to_vec(&serde_json::json!({"mcpServers":{"upwork-local":{"command":"node","args":[legacy]},"other":{"command":"other"}}})).unwrap()).unwrap();
    upwork_connector::register::register(&std::fs::canonicalize(dir.path()).unwrap()).unwrap();
    let c: serde_json::Value =
        serde_json::from_slice(&std::fs::read(dir.path().join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(c["mcpServers"]["upwork-local"]["args"][2], "bridge");
    assert_eq!(c["mcpServers"]["other"]["command"], "other");
}
