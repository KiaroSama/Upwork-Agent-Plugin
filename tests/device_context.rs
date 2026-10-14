use axum::{Json, Router, http::StatusCode, response::IntoResponse, routing::post};
use futures::FutureExt;
use serde_json::{Value, json};
use std::{fs, panic::AssertUnwindSafe, time::Duration};
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

#[tokio::test]
async fn outbound_device_hints_are_utc_without_changing_business_inputs() {
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
    let (captured, mut messages) = mpsc::channel(16);
    let app = Router::new().route(
        "/mcp",
        post(move |Json(v): Json<Value>| {
            let captured = captured.clone();
            async move {
                captured.send(v.clone()).await.unwrap();
                if v.get("id").is_none() {
                    return StatusCode::ACCEPTED.into_response();
                }
                let result = if v["method"] == "initialize" {
                    json!({"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"device-context-fixture","version":"1"}})
                } else {
                    json!({"content":[{"type":"text","text":"Meeting timezone: Asia/Tehran"}],"_meta":{"timezone":"Asia/Tehran"}})
                };
                Json(json!({"jsonrpc":"2.0","id":v["id"],"result":result}))
                    .into_response()
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut writer, input) = tokio::io::duplex(8192);
    let (output, reader) = tokio::io::duplex(8192);
    let mut relay =
        tokio::spawn(async move { bridge::relay(client, route, &endpoint, input, output).await });
    let mut reader = BufReader::new(reader);
    let mut joined = false;
    let outcome = tokio::time::timeout(Duration::from_secs(8), AssertUnwindSafe(async {
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await.unwrap();
        let init = messages.recv().await.unwrap();
        assert!(init["params"].get("_meta").is_none());
        let mut line = String::new();
        reader.read_line(&mut line).await.unwrap();
        assert!(line.contains("serverInfo"));
        writer.write_all(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\",\"params\":{\"_meta\":{\"timezone\":\"Asia/Tehran\",\"utcOffset\":210}}}\n").await.unwrap();
        let notification = messages.recv().await.unwrap();
        assert_eq!(notification["params"]["_meta"], json!({"timezone":"UTC","utcOffset":0}));
        let business = json!({"timezone":"Asia/Tehran","start":"2026-10-14T09:00:00+03:30","text":"Meet at 9 Tehran time","nested":{"timezone":"Europe/Paris"}});
        let cases = [
            (Some(json!({"timezone":"Asia/Tehran","timeZone":"Europe/Paris","timezone_offset_minutes":210,"utcOffset":-60,"utc_offset":3600,"offsetMinutes":"+03:30","progressToken":"keep","openai/userLocation":{"timezone":"Asia/Tehran","city":"unchanged"},"custom":{"timezone":"business"}})), Some(json!({"timezone":"UTC","timeZone":"UTC","timezone_offset_minutes":0,"utcOffset":0,"utc_offset":0,"offsetMinutes":0,"progressToken":"keep","openai/userLocation":{"timezone":"UTC","city":"unchanged"},"custom":{"timezone":"business"}}))),
            (Some(json!({"timezone":{"nested":"Asia/Tehran"},"utcOffset":null,"openai/userLocation":{"timezone":false,"offsetMinutes":123}})), Some(json!({"timezone":"UTC","utcOffset":0,"openai/userLocation":{"timezone":"UTC","offsetMinutes":0}}))),
            (Some(json!({"progressToken":9,"openai/userLocation":{"city":"unchanged"}})), Some(json!({"progressToken":9,"openai/userLocation":{"city":"unchanged"}}))),
            (None, None),
        ];
        for (index, (meta, expected)) in cases.into_iter().enumerate() {
            let id = 2 + index;
            let mut params = json!({"name":"fixture","arguments":business});
            if let Some(meta) = meta {
                params["_meta"] = meta;
            }
            let request = json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":params});
            let mut bytes = serde_json::to_vec(&request).unwrap();
            bytes.push(b'\n');
            writer.write_all(&bytes).await.unwrap();
            let sent = messages.recv().await.unwrap();
            assert_eq!(sent["id"], id);
            assert_eq!(sent["params"]["arguments"], business);
            assert_eq!(sent["params"].get("_meta"), expected.as_ref());
            line.clear();
            reader.read_line(&mut line).await.unwrap();
            let response: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["id"], id);
            assert_eq!(response["result"]["_meta"]["timezone"], "Asia/Tehran");
            assert_eq!(response["result"]["content"][0]["text"], "Meeting timezone: Asia/Tehran");
        }
        writer.shutdown().await.unwrap();
        let result = (&mut relay).await.unwrap();
        joined = true;
        assert!(result.is_ok());
        assert!(messages.try_recv().is_err(), "No request may be replayed");
    }).catch_unwind())
    .await;
    if !joined {
        relay.abort();
        let _ = relay.await;
    }
    server.abort();
    let _ = server.await;
    if let Err(panic) = outcome.unwrap() {
        std::panic::resume_unwind(panic);
    }
}
