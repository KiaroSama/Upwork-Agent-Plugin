use axum::{Json, Router, routing::post};
use serde_json::{Value, json};
use std::{
    fs,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
    sync::{Notify, oneshot},
};
use upwork_connector::{
    auth, bridge,
    config::{self, Mode, Profile},
    credentials::Store,
    http_route::McpRoute,
    route::Route,
};

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("runtime boundaries ")
        .tempdir_in(".ci-work")
        .unwrap()
}

fn direct(root: &std::path::Path) -> Route {
    let path = root.join("config/connections.json");
    config::update(&path, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    Route::new(path).unwrap()
}

#[test]
fn inactive_profiles_do_not_disconnect_an_explicit_direct_route() {
    let dir = temp();
    let route = direct(dir.path());
    config::update(&dir.path().join("config/connections.json"), |c| {
        c.profiles.push(Profile {
            name: "unused".into(),
            url: "http://localhost:1595".into(),
            label: Some("Unused profile".into()),
        });
        Ok(())
    })
    .unwrap();
    assert!(route.check().is_ok());
}

#[test]
fn labels_and_unselected_profiles_do_not_disconnect_the_selected_proxy() {
    let dir = temp();
    let path = dir.path().join("config/connections.json");
    config::update(&path, |c| {
        c.mode = Mode::Proxy;
        c.selected = Some("active".into());
        c.profiles = vec![Profile {
            name: "active".into(),
            url: "socks5://localhost:1595".into(),
            label: Some("Original label".into()),
        }];
        Ok(())
    })
    .unwrap();
    let route = Route::new(path.clone()).unwrap();
    config::update(&path, |c| {
        c.profiles[0].label = Some("New label".into());
        c.profiles.push(Profile {
            name: "unused".into(),
            url: "http://localhost:1596".into(),
            label: None,
        });
        Ok(())
    })
    .unwrap();
    assert!(route.check().is_ok());
    config::update(&path, |c| {
        c.profiles[0].url = "socks5://localhost:1597".into();
        Ok(())
    })
    .unwrap();
    assert!(route.check().is_err());
}

#[test]
fn observed_route_revocation_cannot_be_undone_on_the_old_client() {
    let dir = temp();
    let route = direct(dir.path());
    let clone = route.clone();
    let path = dir.path().join("config/connections.json");
    config::update(&path, |c| {
        c.mode = Mode::Blocked;
        Ok(())
    })
    .unwrap();
    assert!(route.check().is_err());
    config::update(&path, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    assert!(clone.check().is_err());
    assert!(Route::new(path).unwrap().check().is_ok());
}

struct StalledOutput {
    entered: Arc<Notify>,
    flush_only: bool,
}
impl AsyncWrite for StalledOutput {
    fn poll_write(
        self: Pin<&mut Self>,
        _: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        if self.flush_only {
            Poll::Ready(Ok(data.len()))
        } else {
            self.entered.notify_one();
            Poll::Pending
        }
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        if self.flush_only {
            self.entered.notify_one();
            Poll::Pending
        } else {
            Poll::Ready(Ok(()))
        }
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

async fn stalled_output_is_cancelled(flush_only: bool) {
    let dir = temp();
    let route = direct(dir.path());
    let client = McpRoute::new(route.clone()).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let app = Router::new().route(
        "/mcp",
        post(|Json(request): Json<Value>| async move {
            Json(json!({"jsonrpc":"2.0","id":request["id"],"result":{
                "protocolVersion":"2025-11-25","capabilities":{},
                "serverInfo":{"name":"boundary-fixture","version":"1"}
            }}))
        }),
    );
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let (mut writer, input) = tokio::io::duplex(4096);
    let entered = Arc::new(Notify::new());
    let output = StalledOutput {
        entered: entered.clone(),
        flush_only,
    };
    let mut relay =
        tokio::spawn(async move { bridge::relay(client, route, &endpoint, input, output).await });
    writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{\"protocolVersion\":\"2025-11-25\",\"capabilities\":{},\"clientInfo\":{\"name\":\"fixture\",\"version\":\"1\"}}}\n").await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .unwrap();
    let path = dir.path().join("config/connections.json");
    if flush_only {
        fs::remove_file(path).unwrap();
    } else {
        config::update(&path, |c| {
            c.mode = Mode::Blocked;
            Ok(())
        })
        .unwrap();
    }
    // The relay cancels I/O within a watcher tick, then allows two seconds for SDK cleanup.
    let started = std::time::Instant::now();
    let outcome = tokio::time::timeout(Duration::from_secs(3), &mut relay).await;
    eprintln!(
        "Cancellation including bounded SDK cleanup: {:?}",
        started.elapsed()
    );
    relay.abort();
    server.abort();
    assert!(
        outcome.is_ok(),
        "Output backpressure must not disable route cancellation"
    );
    assert!(outcome.unwrap().unwrap().is_err());
}

#[tokio::test]
async fn blocked_route_cancels_stalled_stdout_write() {
    stalled_output_is_cancelled(false).await;
}

#[tokio::test]
async fn deleted_config_cancels_stalled_stdout_flush() {
    stalled_output_is_cancelled(true).await;
}

#[tokio::test]
async fn oauth_route_change_cancels_a_stalled_response_body() {
    let dir = temp();
    let route = direct(dir.path());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
    let (headers_sent, received) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut headers = Vec::new();
        let mut byte = [0];
        while !headers.ends_with(b"\r\n\r\n") && headers.len() < 16384 {
            socket.read_exact(&mut byte).await.unwrap();
            headers.push(byte[0]);
        }
        socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1000\r\n\r\n{").await.unwrap();
        headers_sent.send(()).unwrap();
        std::future::pending::<()>().await;
    });
    let store = Store {
        root: dir.path().into(),
    };
    let mut operation = tokio::spawn(async move {
        let manager = auth::manager(route, store, &endpoint).await.unwrap();
        manager.resolve_metadata().await
    });
    tokio::time::timeout(Duration::from_secs(3), received)
        .await
        .unwrap()
        .unwrap();
    // Observe the incomplete response staying pending before revocation, rather than sleeping.
    assert!(
        tokio::time::timeout(Duration::from_millis(150), &mut operation)
            .await
            .is_err()
    );
    config::update(&dir.path().join("config/connections.json"), |c| {
        c.mode = Mode::Blocked;
        Ok(())
    })
    .unwrap();
    let outcome = tokio::time::timeout(Duration::from_secs(1), &mut operation).await;
    operation.abort();
    server.abort();
    assert!(
        outcome.is_ok(),
        "OAuth body collection must observe route revocation"
    );
    assert!(outcome.unwrap().unwrap().is_err());
}
