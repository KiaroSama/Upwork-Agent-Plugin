use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Sse, sse::Event},
    routing::post,
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
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::Notify,
};
use upwork_connector::{
    bridge,
    config::{self, Mode},
    http_route::McpRoute,
    route::Route,
};

async fn heartbeat_case(observed_ping: bool) {
    fs::create_dir_all(".ci-work").unwrap();
    let dir = tempfile::tempdir_in(".ci-work").unwrap();
    let path = dir.path().join("config/connections.json");
    config::update(&path, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let route = Route::new(path).unwrap();
    let client = McpRoute::new(route.clone()).unwrap();
    let replies = Arc::new(AtomicUsize::new(0));
    let entered = Arc::new(Notify::new());
    let released = Arc::new(Notify::new());
    let fast_reply = Arc::new(Notify::new());
    let fast_seen = fast_reply.clone();
    let seen = replies.clone();
    let started = entered.clone();
    let dropped = released.clone();
    let app = Router::new().route("/mcp", post(move |Json(v): Json<Value>| {
        let seen = seen.clone();
        let started = started.clone();
        let dropped = dropped.clone();
        let fast_seen = fast_seen.clone();
        async move {
            if v.get("method").is_none() && v.get("id").is_some() {
                seen.fetch_add(1, Ordering::SeqCst);
                if v["id"] == "slow-ping" {
                    started.notify_one();
                    dropped.notified().await;
                } else {
                    fast_seen.notify_one();
                }
                return StatusCode::ACCEPTED.into_response();
            }
            if v["method"] == "ping" {
                let ping_id = if v["id"] == 2 { "slow-ping" } else { "fast-ping" };
                let events = [
                    json!({"jsonrpc":"2.0","id":ping_id,"method":"ping"}),
                    json!({"jsonrpc":"2.0","id":v["id"],"result":{}}),
                ];
                let stream = futures::stream::iter(events.map(|event| {
                    Ok::<_, std::convert::Infallible>(Event::default().data(event.to_string()))
                }));
                return Sse::new(stream).into_response();
            }
            let result = if v["method"] == "initialize" {
                json!({"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"heartbeat-fixture","version":"1"}})
            } else {
                json!({"tools":[]})
            };
            Json(json!({"jsonrpc":"2.0","id":v["id"],"result":result})).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut writer, input) = tokio::io::duplex(4096);
    let (output, reader) = tokio::io::duplex(4096);
    let mut relay =
        tokio::spawn(async move { bridge::relay(client, route, &endpoint, input, output).await });
    let mut reader = BufReader::new(reader);
    let outcome = tokio::time::timeout(Duration::from_secs(10), async {
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await.unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.contains("serverInfo"));
        // Observe the actual server ping before submitting its result.
        if observed_ping {
            writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n").await.unwrap();
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            assert!(line.contains("slow-ping"));
            line.clear();
            reader.read_line(&mut line).await.unwrap();
        }
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"slow-ping\",\"result\":{}}\n").await.unwrap();
        entered.notified().await;
        if !observed_ping {
            let result = (&mut relay).await.unwrap();
            assert!(matches!(result, Err(upwork_connector::error::Error::Transport("control_post_timeout"))));
            return;
        }
        // Wait on the real SDK deadline, not a sleep standing in for completion.
        assert!(tokio::time::timeout(Duration::from_secs(6), &mut relay).await.is_err());
        released.notify_waiters();
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"tools/list\"}\n").await.unwrap();
        line.clear();
        assert!(reader.read_line(&mut line).await.unwrap() > 0);
        assert!(line.contains("tools"));
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":4,\"method\":\"ping\"}\n").await.unwrap();
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.contains("fast-ping"));
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"fast-ping\",\"result\":{}}\n").await.unwrap();
        fast_reply.notified().await;
        // A following request proves the transport is still usable without closing early.
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":5,\"method\":\"tools/list\"}\n").await.unwrap();
        line.clear();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.contains("tools"));
        writer.shutdown().await.unwrap();
        assert!((&mut relay).await.unwrap().is_ok());
    }).await;
    relay.abort();
    let _ = relay.await;
    server.abort();
    let _ = server.await;
    outcome.unwrap();
    assert_eq!(
        replies.load(Ordering::SeqCst),
        if observed_ping { 2 } else { 1 },
        "Timed-out replies must never be replayed"
    );
}

#[tokio::test]
async fn matched_ping_timeout_keeps_same_transport_usable_without_replay() {
    heartbeat_case(true).await;
}

#[tokio::test]
async fn unmatched_control_timeout_remains_fatal() {
    heartbeat_case(false).await;
}
