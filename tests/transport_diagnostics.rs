use axum::{Router, http::StatusCode, routing::post};
use rmcp::{model::ClientJsonRpcMessage, transport::streamable_http_client::StreamableHttpClient};
use std::{collections::HashMap, fs, sync::Arc, time::Duration};
use tokio::io::AsyncReadExt;
use upwork_connector::{
    config::{self, Mode},
    diagnostics,
    http_route::McpRoute,
    logging::Log,
    route::Route,
};

#[tokio::test]
async fn failed_http_post_logs_only_fixed_classes_and_preserves_the_typed_error() {
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::BAD_GATEWAY,
    ] {
        failed_status_is_safely_logged(status).await;
    }
}

async fn failed_status_is_safely_logged(status: StatusCode) {
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
        post(move || async move { (status, "fixture-private-response-token") }),
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
    assert!(text.contains(&format!(
        "stage=post phase=headers status={}",
        status.as_u16()
    )));
    assert!(!text.contains("fixture-private-response-token"));
    assert!(!text.contains("fixture-session"));
    assert!(!text.contains("http://"));
}

#[tokio::test]
async fn late_get_body_failure_retains_safe_phase_and_does_not_replay() {
    use futures::StreamExt;
    use tokio::io::AsyncWriteExt;
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |v| {
        v.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let client = McpRoute::new(Route::new(path).unwrap())
        .unwrap()
        .with_log(Log::new(root.path(), "late-stream"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}/private-fixture", listener.local_addr().unwrap());
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(socket.read_u8().await.unwrap());
            assert!(request.len() < 8192);
        }
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 1000\r\nConnection: close\r\n\r\ndata: {").await.unwrap();
    });
    let outcome = tokio::time::timeout(Duration::from_secs(2), async {
        let mut stream = client
            .get_stream(uri.into(), None, None, None, HashMap::new())
            .await
            .unwrap();
        assert!(stream.next().await.unwrap().is_err());
    })
    .await;
    peer.abort();
    let _ = peer.await;
    outcome.unwrap();
    let log = fs::read_dir(root.path().join("logs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = fs::read_to_string(log).unwrap();
    assert_eq!(
        text.matches("MCP stream failure stage=get phase=body class=")
            .count(),
        1
    );
    assert!(!text.contains("private-fixture"));
    assert!(!text.contains("http://"));
}

#[tokio::test]
async fn graceful_sse_completion_reuses_the_connection_without_failure_diagnostics() {
    use axum::{
        extract::ConnectInfo,
        response::{Sse, sse::Event},
        routing::get,
    };
    use futures::{FutureExt, StreamExt};
    use std::{net::SocketAddr, panic::AssertUnwindSafe, sync::Mutex};
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |value| {
        value.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let log = Log::new(root.path(), "graceful-stream");
    log.event("INFO", "Graceful stream fixture started.");
    let client = McpRoute::new(Route::new(path).unwrap())
        .unwrap()
        .with_log(log);
    let peers = Arc::new(Mutex::new(Vec::new()));
    let observed = peers.clone();
    let app = Router::new().route("/mcp", get(move |ConnectInfo(peer): ConnectInfo<SocketAddr>| {
        observed.lock().unwrap().push(peer);
        async {
            Sse::new(futures::stream::once(async {
                Ok::<_, std::convert::Infallible>(Event::default().data(
                    r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":7,"reason":"fixture"}}"#
                ))
            }))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint: Arc<str> = format!("http://{}/mcp", listener.local_addr().unwrap()).into();
    let server = tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .unwrap();
    });
    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        AssertUnwindSafe(async {
            for _ in 0..2 {
                let mut stream = client
                    .get_stream(endpoint.clone(), None, None, None, HashMap::new())
                    .await
                    .unwrap();
                let event = stream.next().await.unwrap().unwrap();
                let value: serde_json::Value =
                    serde_json::from_str(event.data.as_deref().unwrap()).unwrap();
                assert_eq!(value["method"], "notifications/cancelled");
                assert!(
                    stream.next().await.is_none(),
                    "A complete SSE body must end cleanly"
                );
            }
            let peers = peers.lock().unwrap();
            assert_eq!(peers.len(), 2, "Exactly two explicit GETs, no retry");
            assert_eq!(
                peers[0], peers[1],
                "The healthy pooled connection was reused"
            );
            let log = fs::read_dir(root.path().join("logs"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path();
            let text = fs::read_to_string(log).unwrap();
            assert!(text.contains("Graceful stream fixture started."));
            assert!(!text.contains("MCP stream failure"));
            assert!(!text.contains("MCP HTTP failure"));
        })
        .catch_unwind(),
    )
    .await;
    server.abort();
    let _ = server.await;
    outcome.unwrap().unwrap();
}

#[test]
fn status_context_is_numeric_and_builder_details_are_not_formatted() {
    let response = reqwest::Response::from(
        http::Response::builder()
            .status(503)
            .body("private-response")
            .unwrap(),
    );
    let error = response.error_for_status().unwrap_err();
    assert_eq!(diagnostics::network(&error), "http_status");
    assert!(diagnostics::network_context(&error).contains("status=503"));
    let error = reqwest::Client::new()
        .get("https://[private-invalid")
        .build()
        .unwrap_err();
    assert_eq!(diagnostics::network(&error), "request_builder");
    let context = diagnostics::network_context(&error);
    assert!(context.contains("builder=true"));
    assert!(!context.contains("private"));
    assert!(!context.contains("http://"));
}

#[tokio::test]
async fn io_context_preserves_the_typed_kind_without_source_text() {
    let body = reqwest::Body::wrap_stream(futures::stream::once(async {
        Err::<Vec<u8>, _>(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "private-token http://private.example/",
        ))
    }));
    let response = reqwest::Response::from(http::Response::new(body));
    let error = response.bytes().await.unwrap_err();
    let context = diagnostics::network_context(&error);
    assert!(context.contains("io_kind=Some(PermissionDenied)"));
    assert!(!context.contains("private"));
    assert!(!context.contains("http://"));
}

#[tokio::test]
async fn closed_get_response_keeps_request_class_without_logging_private_context() {
    fs::create_dir_all(".ci-work").unwrap();
    let dir = tempfile::tempdir_in(".ci-work").unwrap();
    let path = dir.path().join("config/connections.json");
    config::update(&path, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let client = McpRoute::new(Route::new(path).unwrap())
        .unwrap()
        .with_log(Log::new(dir.path(), "get-diagnostic"));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "http://{}/private-path?token=private-query",
        listener.local_addr().unwrap()
    );
    let peer = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        loop {
            let byte = socket.read_u8().await.unwrap();
            request.push(byte);
            assert!(request.len() < 8192);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
    });
    let outcome = tokio::time::timeout(
        Duration::from_secs(2),
        client.get_stream(
            Arc::from(endpoint),
            Some(Arc::from("private-session")),
            Some("private-event".into()),
            Some("private-token".into()),
            HashMap::new(),
        ),
    )
    .await;
    peer.abort();
    let _ = peer.await;
    let error = match outcome.unwrap() {
        Err(error) => error,
        Ok(_) => panic!("Expected an HTTP request failure"),
    };
    let rmcp::transport::streamable_http_client::StreamableHttpError::Client(error) = error else {
        panic!("Expected the original typed HTTP client failure");
    };
    assert!(error.is_request());
    assert_eq!(diagnostics::network(&error), "request_failed");
    let path = fs::read_dir(dir.path().join("logs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = fs::read_to_string(path).unwrap();
    assert!(text.contains("stage=get class=request_failed"));
    assert!(text.contains("request=true"));
    assert!(text.contains("status=0"));
    assert!(text.contains("io_kind="));
    for private in [
        "private-path",
        "private-query",
        "private-session",
        "private-event",
        "private-token",
        "http://",
    ] {
        assert!(!text.contains(private));
    }
}
