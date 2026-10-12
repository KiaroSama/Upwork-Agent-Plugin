use axum::{Router, http::StatusCode, routing::post};
use rmcp::{model::ClientJsonRpcMessage, transport::streamable_http_client::StreamableHttpClient};
use std::{collections::HashMap, fs, sync::Arc, time::Duration};
use upwork_connector::{
    config::{self, Mode},
    diagnostics,
    http_route::McpRoute,
    logging::Log,
    route::Route,
};

#[tokio::test]
async fn failed_http_post_logs_only_fixed_classes_and_preserves_the_typed_error() {
    fs::create_dir_all(".ci-work").unwrap();
    let dir = tempfile::tempdir_in(".ci-work").unwrap();
    let path = dir.path().join("config/connections.json");
    config::update(&path, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let log = Log::new(dir.path(), "diagnostic-fixture");
    let client = McpRoute::new(Route::new(path).unwrap())
        .unwrap()
        .with_log(log);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let app = Router::new().route(
        "/mcp",
        post(|| async { (StatusCode::BAD_GATEWAY, "fixture-private-response-token") }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let message: ClientJsonRpcMessage =
        serde_json::from_str(r#"{"jsonrpc":"2.0","id":"fixture-ping","result":{}}"#).unwrap();
    let outcome = tokio::time::timeout(
        Duration::from_secs(2),
        client.post_message(
            Arc::from(endpoint),
            message,
            Some(Arc::from("fixture-session")),
            None,
            HashMap::new(),
        ),
    )
    .await;
    server.abort();
    let _ = server.await;
    let error = outcome.unwrap().unwrap_err();
    assert_eq!(diagnostics::transport(&error), "unexpected_http_response");
    let path = fs::read_dir(dir.path().join("logs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("stage=post class=unexpected_http_response"));
    assert!(!text.contains("fixture-private-response-token"));
    assert!(!text.contains("fixture-session"));
    assert!(!text.contains("http://"));
}
