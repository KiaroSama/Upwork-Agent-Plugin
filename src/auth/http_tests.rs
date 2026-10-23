use super::*;
use crate::{config, logging::Log, storage::atomic_json};
use std::{fs, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn exchange(response: &[u8], content_type: &'static str) -> (String, String, u16) {
    let response = response.to_vec();
    tokio::time::timeout(Duration::from_secs(4), async {
        fs::create_dir_all(".ci-work").unwrap();
        let dir = tempfile::tempdir_in(".ci-work").unwrap();
        let path = dir.path().join("config/connections.json");
        atomic_json(
            &path,
            &config::Connections {
                mode: config::Mode::Direct,
                ..Default::default()
            },
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let mut peers = tokio::task::JoinSet::new();
        peers.spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            let mut byte = [0];
            while !request.ends_with(b"\r\n\r\n") {
                socket.read_exact(&mut byte).await.unwrap();
                request.push(byte[0]);
                assert!(request.len() <= 8192);
            }
            let mut body = [0; 21];
            socket.read_exact(&mut body).await.unwrap();
            let _ = socket.write_all(&response).await;
            let _ = socket.shutdown().await;
        });
        let client = OAuthRoute::new(Route::new(path).unwrap())
            .unwrap()
            .with_log(Log::new(dir.path(), "oauth-fixture"));
        let request = http::Request::builder()
            .method("POST")
            .uri(format!("{origin}/private?sentinel-query"))
            .header("content-type", content_type)
            .header("authorization", "Bearer sentinel-header")
            .body(b"sentinel-request-body".to_vec())
            .unwrap();
        let result = client
            .execute_http(request, OAuthHttpRedirectPolicy::Stop)
            .await;
        let (error, status) = match result {
            Ok(response) => (String::new(), response.status().as_u16()),
            Err(e) => (format!("{e:?} {e}"), 0),
        };
        peers.join_next().await.unwrap().unwrap();
        let log = fs::read_to_string(
            fs::read_dir(dir.path().join("logs"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .unwrap();
        for secret in [
            "sentinel-query",
            "sentinel-header",
            "sentinel-body",
            "127.0.0.1",
            "sentinel-request-body",
        ] {
            assert!(!log.contains(secret), "Private data escaped to log");
            assert!(
                !error.contains(secret),
                "Private data escaped to adapter error"
            );
        }
        (log, error, status)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn oauth_status_is_logged_without_changing_sdk_response() {
    let (log, error, status) = exchange(b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 13\r\nConnection: close\r\n\r\nsentinel-body", "application/x-www-form-urlencoded").await;
    assert!(log.contains("stage=token phase=headers class=http_status http_status=401"));
    assert_eq!(status, 401);
    assert!(error.is_empty());
}

#[tokio::test]
async fn oauth_request_failure_is_safe_and_has_no_invented_status() {
    fs::create_dir_all(".ci-work").unwrap();
    let dir = tempfile::tempdir_in(".ci-work").unwrap();
    let path = dir.path().join("config/connections.json");
    atomic_json(
        &path,
        &config::Connections {
            mode: config::Mode::Direct,
            ..Default::default()
        },
    )
    .unwrap();
    let client = OAuthRoute::new(Route::new(path).unwrap())
        .unwrap()
        .with_log(Log::new(dir.path(), "oauth-fixture"));
    let request = http::Request::builder()
        .uri("file://fixture.invalid/sentinel-query")
        .body(Vec::new())
        .unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        client.execute_http(request, OAuthHttpRedirectPolicy::Stop),
    )
    .await
    .unwrap()
    .unwrap_err();
    let text = format!("{error} {error:?}");
    assert!(text.contains("http_status=0"));
    assert!(!text.contains("sentinel-query"));
    assert!(error.source().is_none());
}

#[tokio::test]
async fn oauth_metadata_response_has_safe_stage_and_status() {
    let request = http::Request::builder()
        .uri("https://private.invalid/?sentinel-query")
        .body(Vec::new())
        .unwrap();
    assert_eq!(http_diagnostics::stage(&request), "metadata");
    let failure = http_diagnostics::failure(
        None,
        "metadata",
        "body",
        200,
        Box::new(Error::Safe("OAuth response too large.")),
    );
    let message = format!("{failure:?} {failure}");
    assert!(message.contains("class=response_too_large"));
    assert!(message.contains("http_status=200"));
}

#[tokio::test]
async fn oauth_oversized_body_remains_bounded_and_classified() {
    let mut response =
        b"HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n".to_vec();
    response.resize(response.len() + 1048577, b'x');
    let (log, error, status) = exchange(&response, "application/json").await;
    assert_eq!(status, 0);
    assert!(log.contains("class=response_too_large http_status=200"));
    assert!(error.contains("class=response_too_large"));
}

#[tokio::test]
async fn oauth_truncated_body_retains_observed_status_and_phase() {
    let (log, error, status) = exchange(
        b"HTTP/1.1 200 OK\r\nContent-Length: 999\r\nConnection: close\r\n\r\nsentinel-body",
        "application/json",
    )
    .await;
    assert!(log.contains("stage=registration phase=body"));
    assert!(log.contains("http_status=200"));
    assert_eq!(status, 0);
    assert!(error.contains("phase=body"));
}
