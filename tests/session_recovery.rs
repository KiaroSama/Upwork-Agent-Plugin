use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::post};
use futures::FutureExt;
use rmcp::{
    model::ClientJsonRpcMessage,
    transport::{
        StreamableHttpClientTransport, Transport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    },
};
use serde_json::{Value, json};
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use upwork_connector::{
    config::{self, Mode},
    http_route::McpRoute,
    route::Route,
};

#[tokio::test]
async fn explicit_session_rejection_recovers_once() {
    recovery_case("expired").await;
}

#[tokio::test]
async fn repeated_session_rejection_stops_after_one_recovery() {
    recovery_case("always_expired").await;
}

#[tokio::test]
async fn invalid_json_is_not_replayed() {
    recovery_case("invalid_json").await;
}

#[tokio::test]
async fn failed_response_body_is_not_replayed() {
    recovery_case("body_failure").await;
}

#[tokio::test]
async fn rejected_control_is_not_replayed_or_reinitialized() {
    recovery_case("control_expired").await;
}

#[tokio::test]
async fn authentication_rejection_retries_only_after_a_changed_token() {
    authentication_case(true).await;
}

#[tokio::test]
async fn unchanged_refreshed_token_does_not_resend_the_rejected_post() {
    authentication_case(false).await;
}

async fn authentication_case(changed: bool) {
    use axum::{extract::Form, http::HeaderMap};
    use rmcp::transport::{
        AuthClient,
        auth::{CredentialStore, OAuthClientConfig, StoredCredentials},
        streamable_http_client::StreamableHttpClient,
    };
    use std::collections::HashMap;
    use upwork_connector::{auth, credentials::Store};
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |v| {
        v.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let tokens = Arc::new(AtomicUsize::new(0));
    let (seen, refreshes) = (calls.clone(), tokens.clone());
    let app = Router::new()
        .route("/mcp", post(move |headers: HeaderMap, Json(value): Json<Value>| {
            let seen = seen.clone();
            async move {
                let index = seen.fetch_add(1, Ordering::SeqCst);
                assert_eq!(value["id"], 2);
                let expected = if index == 0 { "Bearer synthetic-old" } else { "Bearer synthetic-new" };
                assert_eq!(headers.get("authorization").unwrap(), expected);
                if index == 0 {
                    return (StatusCode::UNAUTHORIZED, [("www-authenticate", "Bearer error=invalid_token")]).into_response();
                }
                Json(json!({"jsonrpc":"2.0","id":2,"result":{}})).into_response()
            }
        }))
        .route("/token", post(move |Form(fields): Form<HashMap<String, String>>| {
            let refreshes = refreshes.clone();
            async move {
                refreshes.fetch_add(1, Ordering::SeqCst);
                assert_eq!(fields.get("grant_type").map(String::as_str), Some("refresh_token"));
                Json(json!({"access_token":if changed {"synthetic-new"} else {"synthetic-old"},"token_type":"Bearer","expires_in":3600,"refresh_token":"synthetic-refresh-new"}))
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let outcome = std::panic::AssertUnwindSafe(async {
        let route = Route::new(path).unwrap();
        let store = Store { root: root.path().into() };
        let token = serde_json::from_value(json!({"access_token":"synthetic-old","token_type":"Bearer","expires_in":3600,"refresh_token":"synthetic-refresh-old"})).unwrap();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
        store.save(StoredCredentials::new("synthetic-client".into(), Some(token), vec![], Some(now)).with_issuer(Some(origin.clone()))).await.unwrap();
        let mut manager = auth::manager(route.clone(), store, &format!("{origin}/mcp")).await.unwrap();
        manager.set_metadata(serde_json::from_value(json!({"issuer":origin,"authorization_endpoint":format!("{origin}/authorize"),
            "token_endpoint":format!("{origin}/token"),"response_types_supported":["code"],"token_endpoint_auth_methods_supported":["none"]})).unwrap());
        manager.configure_client(OAuthClientConfig::new("synthetic-client", "http://localhost:1/callback")).unwrap();
        let client = AuthClient::new(McpRoute::new(route).unwrap(), manager);
        let message = serde_json::from_value(json!({"jsonrpc":"2.0","id":2,"method":"ping"})).unwrap();
        let result = client.post_message(format!("{origin}/mcp").into(), message, None, None, HashMap::new()).await;
        assert_eq!(result.is_ok(), changed);
        assert_eq!(calls.load(Ordering::SeqCst), if changed {2} else {1});
        assert_eq!(tokens.load(Ordering::SeqCst), 1);
    }).catch_unwind();
    let result = tokio::time::timeout(Duration::from_secs(5), outcome).await;
    server.abort();
    let _ = server.await;
    result.unwrap().unwrap();
}

async fn recovery_case(failure: &'static str) {
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |value| {
        value.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let client = McpRoute::new(Route::new(path).unwrap()).unwrap();
    let initializations = Arc::new(AtomicUsize::new(0));
    let ordinary = Arc::new(AtomicUsize::new(0));
    let (init, calls) = (initializations.clone(), ordinary.clone());
    let app = Router::new().route("/mcp", post(move |Json(value): Json<Value>| {
            let (init, calls) = (init.clone(), calls.clone());
            async move {
                match value["method"].as_str() {
                    Some("initialize") => {
                        let generation = init.fetch_add(1, Ordering::SeqCst) + 1;
                        let mut response = Json(json!({"jsonrpc":"2.0","id":value["id"],"result":{
                            "protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}})).into_response();
                        response.headers_mut().insert("mcp-session-id", format!("generation-{generation}").parse().unwrap());
                        response
                    }
                    Some("notifications/initialized") => StatusCode::ACCEPTED.into_response(),
                    Some("ping") | None => {
                        assert_eq!(value["id"], 2);
                        let index = calls.fetch_add(1, Ordering::SeqCst);
                        if failure == "body_failure" {
                            let stream = futures::stream::iter([
                                Ok(axum::body::Bytes::from_static(b"{unfinished")),
                                Err(std::io::Error::other("synthetic response body failure")),
                            ]);
                            return axum::http::Response::builder().status(200)
                                .header("content-type", "application/json")
                                .body(axum::body::Body::from_stream(stream)).unwrap().into_response();
                        }
                        if failure == "invalid_json" {
                            return ([("content-type", "application/json")], "{unfinished").into_response();
                        }
                        if index == 0 || failure == "always_expired" { return StatusCode::NOT_FOUND.into_response(); }
                        Json(json!({"jsonrpc":"2.0","id":2,"result":{}})).into_response()
                    }
                    _ => StatusCode::ACCEPTED.into_response(),
                }
            }
        }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let mut transport = StreamableHttpClientTransport::with_client(
        client,
        StreamableHttpClientTransportConfig::with_uri(uri),
    );
    let outcome = std::panic::AssertUnwindSafe(async {
            let init: ClientJsonRpcMessage = serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}})).unwrap();
            transport.send(init).await.unwrap();
            assert!(transport.receive().await.is_some());
            transport.send(serde_json::from_value(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).unwrap()).await.unwrap();
            let message = if failure == "control_expired" {
                json!({"jsonrpc":"2.0","id":2,"result":{}})
            } else { json!({"jsonrpc":"2.0","id":2,"method":"ping"}) };
            let result = transport.send(serde_json::from_value(message).unwrap()).await;
            assert_eq!(result.is_ok(), failure == "expired");
            let expected = if matches!(failure, "expired" | "always_expired") { 2 } else { 1 };
            assert_eq!(ordinary.load(Ordering::SeqCst), expected);
            assert_eq!(initializations.load(Ordering::SeqCst), expected);
        }).catch_unwind();
    let result = tokio::time::timeout(Duration::from_secs(8), outcome).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), transport.close()).await;
    server.abort();
    let _ = server.await;
    result.unwrap().unwrap();
}
