use axum::{
    Json, Router,
    response::{Sse, sse::Event},
    routing::post,
};
use futures::{FutureExt, StreamExt, stream};
use serde_json::{Value, json};
use std::{convert::Infallible, fs, panic::AssertUnwindSafe, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::mpsc,
};
use upwork_connector::{
    bridge,
    config::{self, Mode},
    http_route::McpRoute,
    route::Route,
};

#[derive(Clone, Copy)]
enum End {
    Error,
    Block,
    Delete,
    Eof,
}

async fn scenario(end: End) {
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let route = Route::new(path.clone()).unwrap();
    let client = McpRoute::new(route.clone()).unwrap();
    let (sent, mut requests) = mpsc::channel(4);
    let (polled, mut body_started) = mpsc::channel(1);
    let error =
        json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"fixture rejection"}});
    let expected = error.clone();
    let app = Router::new().route("/mcp", post(move |Json(request): Json<Value>| {
        let sent = sent.clone(); let error = error.clone(); let polled = polled.clone();
        async move {
            sent.send(request).await.unwrap();
            let first = if matches!(end, End::Error) { error } else {
                json!({"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"fixture"}})
            };
            let events = stream::once(async move {
                polled.send(()).await.unwrap();
                Ok::<_, Infallible>(Event::default().data(first.to_string()))
            });
            // No EOF is available: cancellation or the first terminal error must suffice.
            ([ ("mcp-session-id", "fixture-session") ], Sse::new(events.chain(stream::pending())))
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
    let mut joined = false;
    let outcome = tokio::time::timeout(Duration::from_secs(5), AssertUnwindSafe(async {
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await.unwrap();
        assert_eq!(requests.recv().await.unwrap()["method"], "initialize");
        body_started.recv().await.unwrap();
        match end {
            End::Error => {
                let mut line = String::new();
                reader.read_line(&mut line).await.unwrap();
                assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), expected);
                writer.shutdown().await.unwrap();
            }
            End::Block => { config::update(&path, |c| { c.mode = Mode::Blocked; Ok(()) }).unwrap(); }
            End::Delete => fs::remove_file(&path).unwrap(),
            End::Eof => writer.shutdown().await.unwrap(),
        }
        let result = (&mut relay).await.unwrap(); joined = true;
        if matches!(end, End::Block | End::Delete) { assert!(result.is_err()); }
        else { assert!(result.is_ok()); }
        assert!(requests.try_recv().is_err(), "No initialization replay");
    }).catch_unwind()).await;
    if !joined {
        relay.abort();
        let _ = relay.await;
    }
    server.abort();
    let _ = server.await;
    if let Err(panic) = outcome.expect("Held-open initialization must stop within existing bounds")
    {
        std::panic::resume_unwind(panic);
    }
}

#[tokio::test]
async fn relay_forwards_first_error_without_waiting_for_eof() {
    scenario(End::Error).await;
}
#[tokio::test]
async fn held_open_initialization_stops_when_route_is_blocked() {
    scenario(End::Block).await;
}
#[tokio::test]
async fn held_open_initialization_stops_when_route_file_is_deleted() {
    scenario(End::Delete).await;
}
#[tokio::test]
async fn held_open_initialization_stops_when_downstream_closes() {
    scenario(End::Eof).await;
}
