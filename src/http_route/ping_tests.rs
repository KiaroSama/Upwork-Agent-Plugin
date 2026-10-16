use super::*;
use crate::{config, route::Route};
use axum::{
    Json, Router,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Sse, sse::Event},
    routing::post,
};
use futures::{StreamExt, stream};
use serde_json::{Value, json};
use std::{
    convert::Infallible,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, Semaphore};

async fn exchange(
    strict: bool,
    ids: Vec<Value>,
    reject: bool,
    block: bool,
    explicit_protocol: bool,
    mixed: bool,
) -> (bool, Vec<Value>) {
    std::fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |c| {
        c.mode = config::Mode::Direct;
        Ok(())
    })
    .unwrap();
    let gate = Arc::new(Semaphore::new(0));
    let stop = Arc::new(Notify::new());
    let replies = Arc::new(Mutex::new(Vec::<Value>::new()));
    let initial = Arc::new(AtomicUsize::new(0));
    let record = replies.clone();
    let initial_count = initial.clone();
    let stop_signal = stop.clone();
    let change = path.clone();
    let app = Router::new().route("/mcp", post(move |headers: HeaderMap, Json(message): Json<Value>| {
        let gate = gate.clone();
        let ids = ids.clone();
        let record = record.clone();
        let initial_count = initial_count.clone();
        let change = change.clone();
        let stop_signal = stop_signal.clone();
        async move {
            if message.get("method").is_none() {
                assert_eq!(headers.get("mcp-session-id").unwrap(), "ping-session");
                assert_eq!(headers.get("authorization").unwrap(), "Bearer synthetic-token");
                assert_eq!(headers.get("x-fixture").unwrap(), "preserved");
                assert_eq!(headers.get_all("mcp-protocol-version").iter().count(), usize::from(explicit_protocol));
                if explicit_protocol {
                    assert_eq!(headers.get("mcp-protocol-version").unwrap(), "2025-11-25");
                }
                assert_eq!(message["result"], json!({}));
                record.lock().unwrap().push(message["id"].clone());
                if block {
                    config::update(&change, |c| { c.mode = config::Mode::Blocked; Ok(()) }).unwrap();
                    stop_signal.notified().await;
                    return StatusCode::REQUEST_TIMEOUT.into_response();
                }
                if reject { return StatusCode::INTERNAL_SERVER_ERROR.into_response(); }
                gate.add_permits(1);
                return StatusCode::ACCEPTED.into_response();
            }
            assert_eq!(message["method"], "initialize");
            initial_count.fetch_add(1, Ordering::SeqCst);
            let count = ids.len();
            let events = ids.into_iter().map(move |id| Ok::<_, Infallible>(Event::default().data(
                {
                let mut ping = json!({"jsonrpc":"2.0","id":id,"method":"ping"});
                if mixed { ping["error"] = json!({"code":-32602,"message":"rejected"}); }
                ping.to_string()
            })));
            let final_reply = stream::once(async move {
                gate.acquire_many(count as u32).await.unwrap().forget();
                Ok::<_, Infallible>(Event::default().data(json!({"jsonrpc":"2.0","id":1,"result":{
                    "protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"gated","version":"1"}
                }}).to_string()))
            });
            let mut response = Sse::new(stream::iter(events).chain(final_reply)).into_response();
            response.headers_mut().insert("mcp-session-id", HeaderValue::from_static("ping-session"));
            response
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let client = McpRoute::new(Route::new(path).unwrap()).unwrap();
    let client = if strict {
        client.with_initialization_verification()
    } else {
        client
    };
    let request = serde_json::from_value(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fixture","version":"1"}
    }})).unwrap();
    let mut headers = HashMap::from([(
        HeaderName::from_static("x-fixture"),
        HeaderValue::from_static("preserved"),
    )]);
    if explicit_protocol {
        headers.insert(
            HeaderName::from_static("mcp-protocol-version"),
            HeaderValue::from_static("2025-11-25"),
        );
    }
    let outcome = tokio::time::timeout(
        Duration::from_secs(3),
        client.post_message_with_max_sse_event_size(
            uri.into(),
            request,
            None,
            Some("synthetic-token".into()),
            headers,
            1024 * 1024,
        ),
    )
    .await;
    stop.notify_waiters();
    server.abort();
    let _ = server.await;
    let result = outcome.expect("Ping-gated initialization must be bounded and cancellable");
    if let Ok(StreamableHttpPostResponse::Json(_, session)) = &result {
        assert_eq!(session.as_deref(), Some("ping-session"));
    }
    assert_eq!(
        initial.load(Ordering::SeqCst),
        1,
        "Initialization is not replayed"
    );
    let recorded = replies.lock().unwrap().clone();
    (result.is_ok(), recorded)
}

#[tokio::test]
async fn gated_ping_ids_and_session_auth_headers_are_preserved_in_both_paths() {
    for strict in [false, true] {
        for ids in [
            vec![json!(7)],
            vec![json!("ping")],
            vec![json!("007")],
            vec![json!(1), json!("1")],
        ] {
            let (ok, actual) = exchange(strict, ids.clone(), false, false, false, false).await;
            assert!(ok);
            assert_eq!(actual, ids);
        }
    }
}

#[tokio::test]
async fn rejected_ping_reply_fails_without_initialization_or_control_replay() {
    for strict in [false, true] {
        let (ok, actual) =
            exchange(strict, vec![json!("rejected")], true, false, false, false).await;
        assert!(!ok);
        assert_eq!(actual, vec![json!("rejected")]);
    }
}

#[tokio::test]
async fn control_ping_budget_is_finite_in_both_paths() {
    for strict in [false, true] {
        let (ok, actual) = exchange(
            strict,
            (0..17).map(|id| json!(id)).collect(),
            false,
            false,
            false,
            false,
        )
        .await;
        assert!(!ok);
        assert_eq!(actual.len(), 16);
    }
}

#[tokio::test]
async fn block_cancels_a_stalled_control_response_in_both_paths() {
    for strict in [false, true] {
        let (ok, actual) =
            exchange(strict, vec![json!("blocked")], false, true, false, false).await;
        assert!(!ok);
        assert_eq!(actual, vec![json!("blocked")]);
    }
}

#[tokio::test]
async fn explicitly_supplied_protocol_header_is_preserved_once() {
    for strict in [false, true] {
        let (ok, actual) =
            exchange(strict, vec![json!("explicit")], false, false, true, false).await;
        assert!(ok);
        assert_eq!(actual, vec![json!("explicit")]);
    }
}

#[tokio::test]
async fn mixed_ping_error_is_refused_without_reply_in_both_paths() {
    for strict in [false, true] {
        let (ok, replies) = exchange(strict, vec![json!(7)], false, false, false, true).await;
        assert!(!ok);
        assert!(replies.is_empty());
    }
}
