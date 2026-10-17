use super::*;
use axum::{
    Router,
    body::{Body, Bytes},
    extract::Request,
    response::Response,
    routing::any,
};
use futures::StreamExt;
use http::{HeaderMap, Method, StatusCode};
use rmcp::model::ServerJsonRpcMessage;
use serde_json::{Value, json};
use std::{
    fs,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::Notify;

#[derive(Clone)]
struct Reply {
    status: StatusCode,
    headers: Vec<(&'static str, String)>,
    bytes: Vec<u8>,
    chunked: bool,
    stall: bool,
}
impl Reply {
    fn json(kind: &str) -> Self {
        Self {
            status: StatusCode::OK,
            headers: vec![("content-type", kind.into())],
            bytes: br#"{"jsonrpc":"2.0","id":31,"result":{}}"#.to_vec(),
            chunked: false,
            stall: false,
        }
    }
    fn sse(kind: &str) -> Self {
        let mut reply = Self::json(kind);
        reply.bytes = b"data: {\"jsonrpc\":\"2.0\",\"id\":31,\"result\":{}}\n\n".to_vec();
        reply
    }
}
struct Fixture {
    _root: tempfile::TempDir,
    path: std::path::PathBuf,
    client: McpRoute,
    uri: Arc<str>,
    count: Arc<AtomicUsize>,
    entered: Arc<Notify>,
    captured: Arc<Mutex<Vec<(Method, HeaderMap, Value)>>>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new(reply: Reply) -> Self {
        fs::create_dir_all(".ci-work").unwrap();
        let root = tempfile::tempdir_in(".ci-work").unwrap();
        let path = root.path().join("config/connections.json");
        crate::config::update(&path, |c| {
            c.mode = crate::config::Mode::Direct;
            Ok(())
        })
        .unwrap();
        let client = McpRoute::new(Route::new(path.clone()).unwrap()).unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(Notify::new());
        let captured = Arc::new(Mutex::new(Vec::new()));
        let (c, n, values) = (count.clone(), entered.clone(), captured.clone());
        let app = Router::new().route(
            "/mcp",
            any(move |request: Request| {
                let reply = reply.clone();
                let c = c.clone();
                let n = n.clone();
                let values = values.clone();
                async move {
                    let (parts, input) = request.into_parts();
                    let data = axum::body::to_bytes(input, 8192).await.unwrap();
                    let value = serde_json::from_slice(&data).unwrap_or(Value::Null);
                    values
                        .lock()
                        .unwrap()
                        .push((parts.method, parts.headers, value));
                    c.fetch_add(1, Ordering::SeqCst);
                    n.notify_one();
                    let body = if reply.chunked || reply.stall {
                        let chunks: Vec<_> = reply
                            .bytes
                            .chunks(if reply.bytes.len() > 1024 * 1024 {
                                65536
                            } else {
                                37
                            })
                            .map(|part| Ok::<_, std::io::Error>(Bytes::copy_from_slice(part)))
                            .collect();
                        if reply.stall {
                            Body::from_stream(
                                futures::stream::iter(chunks).chain(futures::stream::pending()),
                            )
                        } else {
                            Body::from_stream(futures::stream::iter(chunks))
                        }
                    } else {
                        Body::from(reply.bytes)
                    };
                    let mut builder = Response::builder().status(reply.status);
                    for (name, value) in reply.headers {
                        builder = builder.header(name, value);
                    }
                    builder.body(body).unwrap()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let uri = format!("http://{}/mcp", listener.local_addr().unwrap()).into();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Self {
            _root: root,
            path,
            client,
            uri,
            count,
            entered,
            captured,
            server,
        }
    }
    async fn post(
        &self,
        value: Value,
        max: usize,
        strict: bool,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<reqwest::Error>> {
        let client = if strict {
            self.client.clone().with_initialization_verification()
        } else {
            self.client.clone()
        };
        tokio::time::timeout(
            Duration::from_secs(3),
            client.post_message_with_max_sse_event_size(
                self.uri.clone(),
                serde_json::from_value(value).unwrap(),
                None,
                None,
                HashMap::new(),
                max,
            ),
        )
        .await
        .expect("Bounded response fixture")
    }
}
fn ping() -> Value {
    json!({"jsonrpc":"2.0","id":31,"method":"ping"})
}
fn init() -> Value {
    json!({"jsonrpc":"2.0","id":31,"method":"initialize","params":{
    "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}}})
}

#[tokio::test]
async fn baseline_mixed_case_media_types_work_on_every_post_path() {
    for (request, strict) in [(init(), true), (init(), false), (ping(), false)] {
        let f = Fixture::new(Reply::json("Application/JSON; charset=utf-8")).await;
        assert!(
            f.post(request, 4096, strict).await.is_ok(),
            "Valid mixed-case media type must not reject the response"
        );
        assert_eq!(f.count.load(Ordering::SeqCst), 1);
    }
}

#[tokio::test]
async fn baseline_prefix_lookalikes_are_not_mcp_media_types() {
    for (request, strict) in [(init(), true), (init(), false), (ping(), false)] {
        let f = Fixture::new(Reply::json("application/json-private-fixture-marker")).await;
        let result = f.post(request, 4096, strict).await;
        assert!(
            result.is_err(),
            "A matching prefix is not the required media type"
        );
        assert!(
            !result
                .unwrap_err()
                .to_string()
                .contains("private-fixture-marker")
        );
    }
}

#[tokio::test]
async fn baseline_noninitialization_json_respects_the_response_limit() {
    let mut reply = Reply::json("application/json");
    reply.bytes.resize(16 * 1024 * 1024 + 1, b' ');
    let f = Fixture::new(reply).await;
    assert!(
        f.post(ping(), 512, false).await.is_err(),
        "JSON must have a finite application response bound"
    );
}

#[tokio::test]
async fn media_matrix_covers_json_sse_parameters_and_header_ambiguity() {
    for (kind, sse, valid) in [
        ("application/json", false, true),
        ("APPLICATION/JSON", false, true),
        ("application/json; fixture=\"a,b;c\"", false, true),
        ("text/event-stream", true, true),
        ("Text/Event-Stream; CHARSET=\"utf-8\"", true, true),
        ("application/jsonp", false, false),
        ("application/json+fixture", false, false),
        ("text/event-streaming", true, false),
        ("application/json; x=\"unterminated", false, false),
        ("application/json, text/plain", false, false),
        ("text/plain", false, false),
    ] {
        for (request, strict) in [(init(), true), (init(), false), (ping(), false)] {
            let reply = if sse {
                Reply::sse(kind)
            } else {
                Reply::json(kind)
            };
            let f = Fixture::new(reply).await;
            assert_eq!(f.post(request, 4096, strict).await.is_ok(), valid);
        }
    }
    for headers in [
        vec![],
        vec![
            ("content-type", "application/json".into()),
            ("content-type", "text/plain".into()),
        ],
    ] {
        let mut reply = Reply::json("application/json");
        reply.headers = headers;
        let f = Fixture::new(reply).await;
        assert!(f.post(ping(), 4096, false).await.is_err());
    }
}

#[tokio::test]
async fn exact_json_limits_cover_known_chunked_and_error_responses() {
    for chunked in [false, true] {
        for status in [
            StatusCode::OK,
            StatusCode::BAD_REQUEST,
            StatusCode::INTERNAL_SERVER_ERROR,
        ] {
            for overflow in [false, true] {
                let mut reply = Reply::json("application/json");
                reply.chunked = chunked;
                reply.status = status;
                if !status.is_success() {
                    reply.bytes =
                        br#"{"jsonrpc":"2.0","id":31,"error":{"code":-1,"message":"fixture"}}"#
                            .to_vec();
                }
                reply.bytes.resize(512 + usize::from(overflow), b' ');
                let f = Fixture::new(reply).await;
                assert_eq!(f.post(init(), 512, true).await.is_ok(), !overflow);
                assert_eq!(f.count.load(Ordering::SeqCst), 1);
            }
        }
    }
}

#[tokio::test]
async fn notifications_retain_empty_and_unusable_json_compatibility_but_not_unbounded_reads() {
    let notification = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
    for status in [StatusCode::ACCEPTED, StatusCode::NO_CONTENT, StatusCode::OK] {
        let mut reply = Reply::json("application/json");
        reply.status = status;
        reply.bytes.clear();
        let f = Fixture::new(reply).await;
        assert!(matches!(
            f.post(notification.clone(), 512, false).await,
            Ok(StreamableHttpPostResponse::Accepted)
        ));
    }
    let mut reply = Reply::json("application/json");
    reply.bytes = b"not-json".to_vec();
    let f = Fixture::new(reply.clone()).await;
    assert!(matches!(
        f.post(notification.clone(), 512, false).await,
        Ok(StreamableHttpPostResponse::Accepted)
    ));
    assert!(f.post(ping(), 512, false).await.is_err());
    reply.bytes.resize(16 * 1024 * 1024 + 1, b' ');
    let f = Fixture::new(reply).await;
    assert!(f.post(notification, 512, false).await.is_err());
}

#[tokio::test]
async fn challenges_session_expiry_and_unsupported_get_remain_typed() {
    for (status, scope) in [(401, false), (403, true)] {
        let mut reply = Reply::json("text/plain");
        reply.status = StatusCode::from_u16(status).unwrap();
        reply.headers.push((
            "www-authenticate",
            "Bearer error=\"insufficient_scope\", scope=\"jobs:read\"".into(),
        ));
        let f = Fixture::new(reply).await;
        let error = f.post(ping(), 512, false).await.unwrap_err();
        if scope {
            assert!(
                matches!(error,StreamableHttpError::InsufficientScope(ref e) if e.get_required_scope()==Some("jobs:read"))
            );
        } else {
            assert!(matches!(error, StreamableHttpError::AuthRequired(_)));
        }
        let error = f
            .client
            .get_stream(f.uri.clone(), None, None, None, HashMap::new())
            .await;
        assert!(matches!(
            error,
            Err(StreamableHttpError::AuthRequired(_))
                | Err(StreamableHttpError::InsufficientScope(_))
        ));
    }
    let mut reply = Reply::json("text/plain");
    reply.status = StatusCode::NOT_FOUND;
    let f = Fixture::new(reply).await;
    let result = f
        .client
        .post_message(
            f.uri.clone(),
            serde_json::from_value(ping()).unwrap(),
            Some("old-session".into()),
            None,
            HashMap::new(),
        )
        .await;
    assert!(matches!(result, Err(StreamableHttpError::SessionExpired)));
    let mut reply = Reply::json("text/plain");
    reply.status = StatusCode::METHOD_NOT_ALLOWED;
    let f = Fixture::new(reply).await;
    assert!(matches!(
        f.client
            .get_stream(f.uri.clone(), None, None, None, HashMap::new())
            .await,
        Err(StreamableHttpError::ServerDoesNotSupportSse)
    ));
}

#[tokio::test]
async fn get_stream_media_validation_preserves_request_headers_and_event_bounds() {
    for (kind, valid) in [
        ("Text/Event-Stream; charset=utf-8", true),
        ("text/event-stream-private", false),
        ("application/json", true),
    ] {
        let f = Fixture::new(Reply::sse(kind)).await;
        let custom = HashMap::from([
            (
                HeaderName::from_static("x-fixture"),
                HeaderValue::from_static("preserved"),
            ),
            (
                HeaderName::from_static("mcp-protocol-version"),
                HeaderValue::from_static("2025-11-25"),
            ),
        ]);
        let result = f
            .client
            .get_stream_with_max_sse_event_size(
                f.uri.clone(),
                Some("fixture-session".into()),
                Some("last-fixture-event".into()),
                Some("synthetic-token".into()),
                custom,
                512,
            )
            .await;
        assert_eq!(result.is_ok(), valid);
        if let Ok(mut events) = result {
            assert!(events.next().await.unwrap().is_ok());
        }
        let values = f.captured.lock().unwrap();
        let h = &values[0].1;
        assert_eq!(h["mcp-session-id"], "fixture-session");
        assert_eq!(h["last-event-id"], "last-fixture-event");
        assert!(h["authorization"] == "Bearer synthetic-token");
        assert_eq!(h["x-fixture"], "preserved");
    }
    let f = Fixture::new(Reply::sse("text/event-stream")).await;
    let mut events = f
        .client
        .get_stream_with_max_sse_event_size(f.uri.clone(), None, None, None, HashMap::new(), 16)
        .await
        .unwrap();
    assert!(events.next().await.unwrap().is_err());
}

#[tokio::test]
async fn reserved_headers_fail_before_sending_and_control_requests_do_not_replay() {
    for name in ["accept", "mcp-session-id", "last-event-id"] {
        let f = Fixture::new(Reply::json("application/json")).await;
        let headers = HashMap::from([(
            HeaderName::from_static(name),
            HeaderValue::from_static("fixture"),
        )]);
        let response = f
            .client
            .post_message(
                f.uri.clone(),
                serde_json::from_value(ping()).unwrap(),
                None,
                None,
                headers.clone(),
            )
            .await;
        assert!(matches!(
            response,
            Err(StreamableHttpError::ReservedHeaderConflict(_))
        ));
        assert!(matches!(
            f.client
                .get_stream(f.uri.clone(), None, None, None, headers)
                .await,
            Err(StreamableHttpError::ReservedHeaderConflict(_))
        ));
        assert_eq!(f.count.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn legacy_discovery_http_rejection_keeps_the_request_id() {
    let mut reply = Reply::json("text/plain");
    reply.status = StatusCode::NOT_FOUND;
    reply.bytes = b"private-fixture-marker".to_vec();
    let f = Fixture::new(reply).await;
    let request = json!({"jsonrpc":"2.0","id":31,"method":"server/discover","params":{}});
    let result = f.post(request, 512, false).await.unwrap();
    match result {
        StreamableHttpPostResponse::Json(ServerJsonRpcMessage::Error(error), session) => {
            assert!(error.id.is_some());
            assert!(session.is_none());
            assert!(!error.error.message.contains("private-fixture-marker"));
        }
        _ => panic!("Discovery rejection must remain a correlated error"),
    }
}

#[tokio::test]
async fn blocked_route_cancels_a_stalled_noninitialization_body() {
    let mut reply = Reply::json("application/json");
    reply.stall = true;
    reply.bytes = b"{".to_vec();
    let f = Fixture::new(reply).await;
    let client = f.client.clone();
    let uri = f.uri.clone();
    let mut operation = tokio::spawn(async move {
        client
            .post_message(
                uri,
                serde_json::from_value(ping()).unwrap(),
                None,
                None,
                HashMap::new(),
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), f.entered.notified())
        .await
        .unwrap();
    crate::config::update(&f.path, |c| {
        c.mode = crate::config::Mode::Blocked;
        Ok(())
    })
    .unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(2), &mut operation).await;
    operation.abort();
    assert!(outcome.unwrap().unwrap().is_err());
}

#[tokio::test]
async fn malformed_and_truncated_json_requests_fail_without_private_body_or_replay() {
    for chunked in [false, true] {
        for bytes in [
            &b"private-fixture-marker"[..],
            &b"{\"jsonrpc\":\"2.0\",\"id\":31,\"result\":{\"private-fixture-marker\":"[..],
        ] {
            for status in [StatusCode::OK, StatusCode::INTERNAL_SERVER_ERROR] {
                let mut reply = Reply::json("application/json");
                reply.chunked = chunked;
                reply.status = status;
                reply.bytes = bytes.to_vec();
                let f = Fixture::new(reply).await;
                let error = f.post(ping(), 512, false).await.unwrap_err();
                assert!(!error.to_string().contains("private-fixture-marker"));
                assert_eq!(f.count.load(Ordering::SeqCst), 1);
            }
        }
    }
}

#[tokio::test]
async fn ordinary_post_preserves_session_authorization_protocol_and_custom_headers() {
    let f = Fixture::new(Reply::json("Application/JSON")).await;
    let headers = HashMap::from([
        (
            HeaderName::from_static("x-fixture"),
            HeaderValue::from_static("preserved"),
        ),
        (
            HeaderName::from_static("mcp-protocol-version"),
            HeaderValue::from_static("2025-11-25"),
        ),
    ]);
    let result = tokio::time::timeout(
        Duration::from_secs(3),
        f.client.post_message(
            f.uri.clone(),
            serde_json::from_value(ping()).unwrap(),
            Some("fixture-session".into()),
            Some("synthetic-token".into()),
            headers,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(matches!(result, StreamableHttpPostResponse::Json(_, _)));
    let values = f.captured.lock().unwrap();
    assert_eq!(values.len(), 1);
    let (method, headers, value) = &values[0];
    assert_eq!(*method, Method::POST);
    assert_eq!(headers["authorization"], "Bearer synthetic-token");
    assert_eq!(headers["mcp-session-id"], "fixture-session");
    assert_eq!(headers["mcp-protocol-version"], "2025-11-25");
    assert_eq!(headers["x-fixture"], "preserved");
    assert_eq!(*value, ping());
}

#[tokio::test]
async fn normal_json_budget_is_separate_from_sse_and_bounds_chunked_errors() {
    for chunked in [false, true] {
        for error in [false, true] {
            for length in [2 * 1024 * 1024, 16 * 1024 * 1024, 16 * 1024 * 1024 + 1] {
                let mut reply = Reply::json("application/json");
                reply.chunked = chunked;
                if error {
                    reply.status = StatusCode::INTERNAL_SERVER_ERROR;
                    reply.bytes =
                        br#"{"jsonrpc":"2.0","id":31,"error":{"code":-1,"message":"fixture"}}"#
                            .to_vec();
                }
                reply.bytes.resize(length, b' ');
                let f = Fixture::new(reply).await;
                // A smaller SSE event setting is not a JSON response setting.
                assert_eq!(
                    f.post(ping(), 512, false).await.is_ok(),
                    length <= 16 * 1024 * 1024
                );
                assert_eq!(f.count.load(Ordering::SeqCst), 1);
            }
        }
    }
}
