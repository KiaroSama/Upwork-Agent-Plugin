use super::McpRoute;
use crate::{
    config::{self, Mode},
    logging::Log,
    route::Route,
};
use futures::{FutureExt, StreamExt};
use rmcp::transport::streamable_http_client::StreamableHttpClient;
use std::{collections::HashMap, fs, sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::{TlsAcceptor, rustls};

fn client_tls() -> rustls::ClientConfig {
    use rustls::pki_types::{CertificateDer, pem::PemObject};
    let mut roots = rustls::RootCertStore::empty();
    for cert in
        CertificateDer::pem_slice_iter(include_bytes!("../../tests/fixtures/localhost-test.crt"))
    {
        roots.add(cert.unwrap()).unwrap();
    }
    // Pin only this fixture CA. Native macOS trust rejects the old synthetic
    // certificate; production platform verification is deliberately unchanged.
    rustls::ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

fn tls() -> TlsAcceptor {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let certs =
        CertificateDer::pem_slice_iter(include_bytes!("../../tests/fixtures/localhost-test.crt"))
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
    let key =
        PrivateKeyDer::from_pem_slice(include_bytes!("../../tests/fixtures/localhost-test.key"))
            .unwrap();
    TlsAcceptor::from(Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap(),
    ))
}

#[tokio::test]
async fn sdk_resumes_abrupt_tls_get_with_cursor_without_replaying_posts() {
    use rmcp::transport::{
        StreamableHttpClientTransport, Transport,
        streamable_http_client::StreamableHttpClientTransportConfig,
    };
    use serde_json::{Value, json};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |v| {
        v.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let route = Route::new(path).unwrap();
    let client = McpRoute {
        client: route
            .builder(false, None)
            .unwrap()
            .tls_backend_preconfigured(client_tls())
            .build()
            .unwrap(),
        route: route.clone(),
        log: Some(Log::new(root.path(), "sdk-idle-tls")),
        verify_initialization: false,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!(
        "https://localhost:{}/mcp",
        listener.local_addr().unwrap().port()
    );
    let gets = Arc::new(AtomicUsize::new(0));
    let posts = Arc::new(AtomicUsize::new(0));
    let (seen_gets, seen_posts) = (gets.clone(), posts.clone());
    let server = tokio::spawn(async move {
        let acceptor = tls();
        let mut children = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (socket, _) = accepted.unwrap();
                    let (acceptor, gets, posts) = (acceptor.clone(), seen_gets.clone(), seen_posts.clone());
                    children.spawn(async move {
                        tokio::time::timeout(Duration::from_secs(5), async move {
                            let mut socket = acceptor.accept(socket).await.unwrap();
                            let mut head = Vec::new();
                            while !head.ends_with(b"\r\n\r\n") {
                                head.push(socket.read_u8().await.unwrap());
                                assert!(head.len() < 8192);
                            }
                            let head = String::from_utf8(head).unwrap().to_ascii_lowercase();
                            if head.starts_with("get ") {
                                let index = gets.fetch_add(1, Ordering::SeqCst);
                                assert!(index < 2, "No uncontrolled reconnect loop");
                                assert!(head.contains("mcp-session-id: fixture-session"));
                                if index == 1 { assert!(head.contains("last-event-id: fixture-cursor")); }
                                let marker = if index == 0 { "first" } else { "resumed" };
                                let body = format!("id: fixture-cursor\ndata: {{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\",\"params\":{{\"level\":\"info\",\"data\":\"{marker}\"}}}}\n\n");
                                socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n").await.unwrap();
                                socket.write_all(body.as_bytes()).await.unwrap();
                                socket.flush().await.unwrap();
                                if index == 1 { futures::future::pending::<()>().await; }
                                return; // First stream has no TLS close_notify.
                            }
                            if head.starts_with("delete ") {
                                socket.write_all(b"HTTP/1.1 204 No Content\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                                return;
                            }
                            assert!(head.starts_with("post "));
                            posts.fetch_add(1, Ordering::SeqCst);
                            let length: usize = head.lines().find_map(|l| l.strip_prefix("content-length: ")).unwrap().parse().unwrap();
                            assert!(length < 8192);
                            let mut body = vec![0; length];
                            socket.read_exact(&mut body).await.unwrap();
                            let request: Value = serde_json::from_slice(&body).unwrap();
                            let response = match request["method"].as_str().unwrap() {
                                "initialize" => json!({"jsonrpc":"2.0","id":request["id"],"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}}),
                                "ping" => json!({"jsonrpc":"2.0","id":request["id"],"result":{}}),
                                "notifications/initialized" => {
                                    socket.write_all(b"HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                                    return;
                                }
                                _ => panic!("Unexpected POST"),
                            }.to_string();
                            let headers = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nMcp-Session-Id: fixture-session\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len());
                            socket.write_all(headers.as_bytes()).await.unwrap();
                            socket.write_all(response.as_bytes()).await.unwrap();
                            socket.flush().await.unwrap();
                        }).await.unwrap();
                    });
                }
                result = children.join_next(), if !children.is_empty() => { result.unwrap().unwrap(); }
            }
        }
    });
    let mut transport = StreamableHttpClientTransport::with_client(
        client,
        StreamableHttpClientTransportConfig::with_uri(endpoint),
    );
    let outcome = tokio::time::timeout(Duration::from_secs(4), std::panic::AssertUnwindSafe(async {
        transport.send(serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}})).unwrap()).await.unwrap();
        let init = transport.receive().await.unwrap();
        assert_eq!(serde_json::to_value(init).unwrap()["id"], 1);
        transport.send(serde_json::from_value(json!({"jsonrpc":"2.0","method":"notifications/initialized"})).unwrap()).await.unwrap();
        for expected in ["first", "resumed"] {
            let notification = serde_json::to_value(transport.receive().await.unwrap()).unwrap();
            assert_eq!(notification["params"]["data"], expected);
        }
        transport.send(serde_json::from_value(json!({"jsonrpc":"2.0","id":2,"method":"ping"})).unwrap()).await.unwrap();
        assert_eq!(serde_json::to_value(transport.receive().await.unwrap()).unwrap()["id"], 2);
        assert_eq!(gets.load(Ordering::SeqCst), 2);
        assert_eq!(posts.load(Ordering::SeqCst), 3, "Exactly init, ready and one ping; no POST replay");
        let guarded = route.guard(futures::future::pending::<()>());
        let blocked = async {
            config::update(&root.path().join("config/connections.json"), |v| { v.mode = Mode::Blocked; Ok(()) }).unwrap();
        };
        let (cancelled, ()) = tokio::join!(guarded, blocked);
        assert!(cancelled.is_err(), "Route revocation still cancels resumed work");
    }).catch_unwind()).await;
    let _ = tokio::time::timeout(Duration::from_secs(1), transport.close()).await;
    server.abort();
    let _ = server.await;
    outcome.unwrap().unwrap();
    let file = fs::read_dir(root.path().join("logs"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let log = fs::read_to_string(file).unwrap();
    assert!(log.contains("recovery=sdk_get"));
    assert_eq!(log.matches("MCP GET stream opened").count(), 2);
    assert!(!log.contains("[ERROR]"));
    assert!(!log.contains("fixture-cursor"));
}

#[tokio::test]
async fn abrupt_tls_idle_get_remains_recoverable_but_partial_event_is_an_error() {
    for (partial, post) in [(false, false), (true, false), (false, true)] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/connections.json");
        config::update(&path, |value| {
            value.mode = Mode::Direct;
            Ok(())
        })
        .unwrap();
        let route = Route::new(path).unwrap();
        let http = route
            .builder(false, None)
            .unwrap()
            .tls_backend_preconfigured(client_tls())
            .build()
            .unwrap();
        let log = Log::new(root.path(), "idle-tls");
        let client = McpRoute {
            client: http,
            route,
            log: Some(log),
            verify_initialization: false,
        };
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!(
            "https://localhost:{}/mcp",
            listener.local_addr().unwrap().port()
        );
        let acceptor = tls();
        let peer = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut stream = acceptor.accept(socket).await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
                assert!(request.len() < 8192);
            }
            if post {
                let head = String::from_utf8(request).unwrap().to_ascii_lowercase();
                let length: usize = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .unwrap()
                    .parse()
                    .unwrap();
                assert!(length < 8192);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).await.unwrap();
            }
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\nid: fixture-cursor\ndata: {\"jsonrpc\":\"2.0\",\"method\":\"notifications/cancelled\",\"params\":{\"requestId\":7}}\n\n").await.unwrap();
            if partial {
                stream.write_all(b"data: {\"private-fixture").await.unwrap();
            }
            stream.flush().await.unwrap();
            // Drop TCP without sending TLS close_notify, reproducing rustls EOF.
        });
        let outcome = tokio::time::timeout(
            Duration::from_secs(3),
            std::panic::AssertUnwindSafe(async {
                let mut stream = if post {
                    let message = serde_json::from_value(serde_json::json!({"jsonrpc":"2.0","id":7,"method":"ping"})).unwrap();
                    match client.post_message(endpoint.into(), message, None, None, HashMap::new()).await.unwrap() {
                        rmcp::transport::streamable_http_client::StreamableHttpPostResponse::Sse(stream, _) => stream,
                        _ => panic!("Expected fixture SSE response"),
                    }
                } else {
                    client.get_stream(endpoint.into(), None, None, None, HashMap::new()).await.unwrap()
                };
                assert_eq!(
                    stream.next().await.unwrap().unwrap().id.as_deref(),
                    Some("fixture-cursor")
                );
                assert!(
                    stream.next().await.unwrap().is_err(),
                    "EOF must reach SDK recovery, not become success"
                );
            })
            .catch_unwind(),
        )
        .await;
        peer.abort();
        let _ = peer.await;
        outcome.unwrap().unwrap();
        let log = fs::read_dir(root.path().join("logs"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let text = fs::read_to_string(log).unwrap();
        if partial || post {
            assert!(text.contains("[ERROR]"));
            assert!(!text.contains("recovery=sdk_get"));
        } else {
            assert!(text.contains("[WARNING]"));
            assert!(text.contains("recovery=sdk_get"));
            assert!(text.contains("status=200"));
            assert!(!text.contains("[ERROR]"));
        }
        assert!(!text.contains("private-fixture"));
        assert!(!text.contains("fixture-cursor"));
        assert!(!text.contains("localhost"));
    }
}
