use rmcp::transport::streamable_http_client::{StreamableHttpError, StreamableHttpPostResponse};
type Failure = StreamableHttpError<reqwest::Error>;
pub(super) async fn first_terminal(
    response: StreamableHttpPostResponse,
    context: Option<super::initialization_control::Context>,
) -> Result<StreamableHttpPostResponse, Failure> {
    super::initialization_control::first_terminal(response, context).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, http_route::McpRoute, route::Route};
    use axum::{
        Json, Router,
        http::HeaderValue,
        response::{IntoResponse, Sse, sse::Event},
        routing::post,
    };
    use futures::StreamExt;
    use futures::{FutureExt, stream};
    use rmcp::model::ServerJsonRpcMessage;
    use rmcp::{
        model::ClientJsonRpcMessage, transport::streamable_http_client::StreamableHttpClient,
    };
    use serde_json::{Value, json};
    use std::panic::AssertUnwindSafe;
    use std::{
        collections::HashMap,
        convert::Infallible,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    fn success() -> Value {
        json!({"jsonrpc":"2.0","id":1,"result":{
            "protocolVersion":"2025-11-25","capabilities":{},
            "serverInfo":{"name":"fixture","version":"1"}
        }})
    }

    fn failure(id: Value) -> Value {
        json!({"jsonrpc":"2.0","id":id,
            "error":{"code":-32602,"message":"private-fixture-marker"}})
    }

    async fn exchange(
        strict: bool,
        events: Vec<Value>,
        json_response: bool,
        keep_open: bool,
    ) -> Result<(ServerJsonRpcMessage, Option<String>), Failure> {
        let requests = Arc::new(AtomicUsize::new(0));
        let seen = requests.clone();
        let app = Router::new().route(
            "/mcp",
            post(move |Json(_): Json<Value>| {
                let events = events.clone();
                let seen = seen.clone();
                async move {
                    seen.fetch_add(1, Ordering::SeqCst);
                    let mut response = if json_response {
                        Json(events[0].clone()).into_response()
                    } else {
                        let records = events.into_iter().map(|value| {
                            Ok::<_, Infallible>(Event::default().data(value.to_string()))
                        });
                        // Never release EOF in the open-stream case. Receiving
                        // the first error must suffice; no sleep signals success.
                        let tail = stream::pending::<Result<Event, Infallible>>()
                            .take(if keep_open { 1 } else { 0 });
                        Sse::new(stream::iter(records).chain(tail)).into_response()
                    };
                    response.headers_mut().insert(
                        "mcp-session-id",
                        HeaderValue::from_static("fixture-session"),
                    );
                    response
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint: Arc<str> = format!("http://{}/mcp", listener.local_addr().unwrap()).into();
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        std::fs::create_dir_all(".ci-work").unwrap();
        let root = tempfile::tempdir_in(".ci-work").unwrap();
        let path = root.path().join("config/connections.json");
        config::update(&path, |value| {
            value.mode = config::Mode::Direct;
            Ok(())
        })
        .unwrap();
        let client = McpRoute::new(Route::new(path).unwrap()).unwrap();
        let client = if strict {
            client.with_initialization_verification()
        } else {
            client
        };
        let request: ClientJsonRpcMessage = serde_json::from_value(json!({
            "jsonrpc":"2.0","id":1,"method":"initialize","params":{
                "protocolVersion":"2025-11-25","capabilities":{},
                "clientInfo":{"name":"fixture","version":"1"}
            }
        }))
        .unwrap();
        let outcome = tokio::time::timeout(
            Duration::from_secs(3),
            AssertUnwindSafe(async {
                client
                    .post_message_with_max_sse_event_size(
                        endpoint,
                        request,
                        None,
                        None,
                        HashMap::new(),
                        1024 * 1024,
                    )
                    .await?
                    .expect_initialized::<reqwest::Error>()
                    .await
            })
            .catch_unwind(),
        )
        .await;
        server.abort();
        let _ = server.await;
        assert_eq!(requests.load(Ordering::SeqCst), 1, "No request replay");
        match outcome.expect("Initialization must complete without waiting for stream EOF") {
            Ok(result) => result,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    async fn assert_first(events: Vec<Value>, expected: Value, json: bool, open: bool) {
        // Exercise both the normal relay client and strict manual sign-in client.
        for strict in [false, true] {
            let (message, session) = exchange(strict, events.clone(), json, open).await.unwrap();
            assert_eq!(serde_json::to_value(message).unwrap(), expected);
            assert_eq!(session.as_deref(), Some("fixture-session"));
        }
    }

    #[tokio::test]
    async fn error_then_success_cannot_replace_the_first_terminal_reply() {
        let error = failure(json!(1));
        assert_first(vec![error.clone(), success()], error, false, false).await;
    }

    #[tokio::test]
    async fn numeric_string_error_id_is_preserved_before_a_later_success() {
        let error = failure(json!("1"));
        assert_first(vec![error.clone(), success()], error, false, false).await;
    }

    #[tokio::test]
    async fn error_only_stream_returns_the_error_before_eof() {
        let error = failure(json!(1));
        assert_first(vec![error.clone()], error, false, true).await;
    }

    #[tokio::test]
    async fn error_only_closed_stream_preserves_the_error_instead_of_empty_stream() {
        let error = failure(json!(1));
        assert_first(vec![error.clone()], error, false, false).await;
    }

    #[tokio::test]
    async fn success_then_error_keeps_the_first_valid_response() {
        assert_first(vec![success(), failure(json!(1))], success(), false, false).await;
    }

    #[tokio::test]
    async fn successful_response_does_not_require_eof() {
        assert_first(vec![success()], success(), false, true).await;
    }

    #[tokio::test]
    async fn json_error_behavior_and_session_are_unchanged() {
        let error = failure(json!(1));
        assert_first(vec![error.clone()], error, true, false).await;
    }

    #[tokio::test]
    async fn json_success_behavior_and_session_are_unchanged() {
        assert_first(vec![success()], success(), true, false).await;
    }

    #[tokio::test]
    async fn logging_notification_before_error_does_not_hide_the_error() {
        let log = json!({"jsonrpc":"2.0","method":"notifications/message",
            "params":{"level":"info","data":"fixture"}});
        let error = failure(json!(1));
        assert_first(vec![log, error.clone(), success()], error, false, false).await;
    }

    #[tokio::test]
    async fn logging_notification_before_success_remains_supported() {
        let log = json!({"jsonrpc":"2.0","method":"notifications/message",
            "params":{"level":"info","data":"fixture"}});
        assert_first(vec![log, success()], success(), false, false).await;
    }

    #[tokio::test]
    async fn strict_conflicting_envelope_is_not_rescued_by_a_later_success() {
        let mut mixed = success();
        mixed["error"] = failure(json!(1))["error"].clone();
        assert!(
            exchange(true, vec![mixed, success()], false, false)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn accepted_notification_carrier_passes_through_without_network() {
        let response = first_terminal(StreamableHttpPostResponse::Accepted, None)
            .await
            .unwrap();
        assert!(matches!(response, StreamableHttpPostResponse::Accepted));
    }
}
