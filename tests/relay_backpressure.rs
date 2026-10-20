use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Sse, sse::Event},
    routing::post,
};
use futures::{FutureExt, StreamExt};
use serde_json::{Value, json};
use std::{fs, panic::AssertUnwindSafe, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    sync::{Notify, Semaphore},
};
use upwork_connector::{
    bridge,
    config::{self, Mode},
    http_route::McpRoute,
    route::Route,
};

#[tokio::test]
async fn relay_saturation_releases_cleanly_and_route_cancellation_remains_bounded() {
    fs::create_dir_all(".ci-work").unwrap();
    let dir = tempfile::tempdir_in(".ci-work").unwrap();
    let path = dir.path().join("config/connections.json");
    config::update(&path, |v| {
        v.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let route = Route::new(path.clone()).unwrap();
    let client = McpRoute::new(route.clone()).unwrap();
    let held = Arc::new(Semaphore::new(0));
    let entered = Arc::new(Semaphore::new(0));
    let control = Arc::new(Notify::new());
    let (gate, seen, reply) = (held.clone(), entered.clone(), control.clone());
    let app = Router::new().route("/mcp", post(move |Json(v): Json<Value>| {
        let (gate, seen, reply) = (gate.clone(), seen.clone(), reply.clone());
        async move {
            if v.get("method").is_none() { reply.notify_one(); return StatusCode::ACCEPTED.into_response(); }
            if v["method"] == "notifications/initialized" { return StatusCode::ACCEPTED.into_response(); }
            if v["method"] == "initialize" {
                return Json(json!({"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}}})).into_response();
            }
            if v["id"] == 2 {
                let stream = futures::stream::once(async move {
                    Ok::<_, std::convert::Infallible>(Event::default().data(json!({"jsonrpc":"2.0","id":"held-ping","method":"ping"}).to_string()))
                }).chain(futures::stream::pending());
                return Sse::new(stream).into_response();
            }
            seen.add_permits(1);
            gate.acquire().await.unwrap().forget();
            Json(json!({"jsonrpc":"2.0","id":v["id"],"result":{}})).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let uri = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    let (mut writer, input) = tokio::io::duplex(16384);
    let (output, reader) = tokio::io::duplex(16384);
    let mut relay =
        tokio::spawn(async move { bridge::relay(client, route, &uri, input, output).await });
    let mut reader = BufReader::new(reader);
    let mut joined = false;
    let outcome = tokio::time::timeout(Duration::from_secs(8), AssertUnwindSafe(async {
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await.unwrap();
        let mut line = String::new(); reader.read_line(&mut line).await.unwrap();
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\n").await.unwrap();
        line.clear(); reader.read_line(&mut line).await.unwrap();
        assert!(line.contains("held-ping"));
        for id in 3..19 {
            writer.write_all(format!("{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"ping\"}}\n").as_bytes()).await.unwrap();
        }
        entered.acquire_many(2).await.unwrap().forget();
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":\"held-ping\",\"result\":{}}\n").await.unwrap();
        // Characterize the current bounded failure window, then prove fixture health by release.
        // This timeout is the observation itself, not a readiness delay or an asserted repair.
        let control_before_release = tokio::time::timeout(Duration::from_millis(200), control.notified()).await.is_ok();
        held.add_permits(16);
        if !control_before_release {
            tokio::time::timeout(Duration::from_secs(2), control.notified()).await
                .expect("Control must progress after the ordinary gate is released");
        }
        fs::write(".ci-work/relay-characterization.json",
            serde_json::to_vec(&json!({"control_before_release":control_before_release,
                "observation_ms":200,"ordinary_frames":16,"repair_proved":false})).unwrap()).unwrap();
        config::update(&path, |v| { v.mode = Mode::Blocked; Ok(()) }).unwrap();
        let stopped = tokio::time::timeout(Duration::from_secs(3), &mut relay).await.unwrap();
        joined = true;
        assert!(stopped.unwrap().is_err());
    }).catch_unwind()).await;
    held.add_permits(16);
    if !joined {
        relay.abort();
        let _ = relay.await;
    }
    server.abort();
    let _ = server.await;
    outcome.unwrap().unwrap();
}
