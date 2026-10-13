use super::send_owned;
use crate::error::{Error, Result};
use rmcp::{
    RoleClient,
    model::{ClientJsonRpcMessage, InitializeResult, ProtocolVersion},
    transport::Transport,
};
use serde_json::{Value, json};

const REQUEST_ID: i64 = 1;

fn validate(response: &Value) -> Result<()> {
    // Match the SDK's numeric-request/string-response compatibility without
    // accepting an unrelated response or echoing untrusted response contents.
    let id = response.get("id").and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
    });
    if response.get("jsonrpc").and_then(Value::as_str) != Some("2.0")
        || id != Some(REQUEST_ID)
        || response.get("method").is_some()
        || response.get("error").is_some()
    {
        return Err(Error::Safe(
            "MCP initialization response rejected; sign-in not verified.",
        ));
    }
    let result = response.get("result").ok_or(Error::Safe(
        "MCP initialization result missing; sign-in not verified.",
    ))?;
    // Deserialize the actual result schema, not just the presence of serverInfo.
    let initialized: InitializeResult = serde_json::from_value(result.clone())
        .map_err(|_| Error::Safe("MCP initialization result invalid; sign-in not verified."))?;
    if !ProtocolVersion::KNOWN_VERSIONS.contains(&initialized.protocol_version)
        || !initialized.protocol_version.has_initialize()
    {
        return Err(Error::Safe(
            "MCP initialization protocol unsupported; sign-in not verified.",
        ));
    }
    Ok(())
}

pub(super) async fn initialize<T: Transport<RoleClient>>(transport: &mut T) -> Result<()> {
    let message: ClientJsonRpcMessage = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "id": REQUEST_ID,
        "method": "initialize",
        "params": {
            "protocolVersion": ProtocolVersion::LATEST_WITH_INITIALIZE,
            "capabilities": {},
            "clientInfo": {
                "name": "upwork-native-sign-in",
                "version": env!("CARGO_PKG_VERSION")
            }
        }
    }))?;
    let send = send_owned(transport, message);
    let (sent, response) = tokio::join!(send, transport.receive());
    sent.map_err(|_| Error::Safe("Session saved but MCP initialization failed."))?;
    let response = response.ok_or(Error::Safe("MCP initialization did not return."))?;
    validate(&serde_json::to_value(response)?)?;
    // Ready is published only after validating identity, schema and negotiation.
    let initialized = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "method": "notifications/initialized"
    }))?;
    send_owned(transport, initialized)
        .await
        .map_err(|_| Error::Safe("MCP initialization notification failed."))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        Json, Router,
        http::StatusCode,
        response::{IntoResponse, Sse, sse::Event},
        routing::post,
    };
    use rmcp::transport::{
        StreamableHttpClientTransport, streamable_http_client::StreamableHttpClientTransportConfig,
    };
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };

    fn valid() -> Value {
        json!({
            "jsonrpc": "2.0", "id": REQUEST_ID,
            "result": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "serverInfo": {"name": "fixture", "version": "1"}
            }
        })
    }

    #[test]
    fn accepts_known_handshake_versions_and_sdk_compatible_ids() {
        for version in ProtocolVersion::KNOWN_VERSIONS {
            if !version.has_initialize() {
                continue;
            }
            for id in [json!(1), json!("1")] {
                let mut response = valid();
                response["id"] = id;
                response["result"]["protocolVersion"] = json!(version);
                assert!(validate(&response).is_ok());
            }
        }
    }

    #[test]
    fn refuses_unknown_and_handshakeless_versions() {
        for version in ["2099-01-01", "2026-07-28", "", "2025-01-01"] {
            let mut response = valid();
            response["result"]["protocolVersion"] = json!(version);
            assert!(validate(&response).is_err());
        }
    }

    #[test]
    fn refuses_mismatched_missing_and_invalid_ids() {
        for id in [
            json!(999),
            json!("999"),
            json!(null),
            json!(true),
            json!({}),
        ] {
            let mut response = valid();
            response["id"] = id;
            assert!(validate(&response).is_err());
        }
        let mut response = valid();
        response.as_object_mut().unwrap().remove("id");
        assert!(validate(&response).is_err());
    }

    #[test]
    fn refuses_error_and_request_envelopes_even_with_result_present() {
        for key in ["error", "method"] {
            let mut response = valid();
            response[key] = json!("private-fixture-marker");
            let error = validate(&response).unwrap_err().to_string();
            assert!(!error.contains("private-fixture-marker"));
        }
    }

    #[test]
    fn refuses_missing_result_and_required_result_fields() {
        let mut response = valid();
        response.as_object_mut().unwrap().remove("result");
        assert!(validate(&response).is_err());
        for key in ["protocolVersion", "capabilities", "serverInfo"] {
            let mut response = valid();
            response["result"].as_object_mut().unwrap().remove(key);
            assert!(validate(&response).is_err());
        }
    }

    #[test]
    fn refuses_null_or_malformed_result_fields() {
        for key in ["protocolVersion", "capabilities", "serverInfo"] {
            for value in [Value::Null, json!(false), json!(42), json!([])] {
                let mut response = valid();
                response["result"][key] = value;
                assert!(validate(&response).is_err());
            }
        }
        for value in [Value::Null, json!(false), json!(42), json!([])] {
            let mut response = valid();
            response["result"] = value;
            assert!(validate(&response).is_err());
        }
    }

    #[test]
    fn refuses_incomplete_server_identity_and_invalid_capabilities() {
        for key in ["name", "version"] {
            let mut response = valid();
            response["result"]["serverInfo"]
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(validate(&response).is_err());
            response["result"]["serverInfo"][key] = json!(42);
            assert!(validate(&response).is_err());
        }
        let mut response = valid();
        response["result"]["capabilities"] = json!({"tools": {"listChanged": "invalid"}});
        assert!(validate(&response).is_err());
    }

    #[test]
    fn accepts_optional_metadata_without_logging_or_rewriting_it() {
        let mut response = valid();
        response["result"]["instructions"] = json!("fixture instructions");
        response["result"]["serverInfo"]["title"] = json!("Fixture Server");
        response["result"]["capabilities"] = json!({"tools": {}, "experimental": {"fixture": {}}});
        assert!(validate(&response).is_ok());
    }

    async fn wire_case(
        response: Value,
        notification_status: StatusCode,
        sse: bool,
    ) -> (Result<()>, usize) {
        let count = Arc::new(AtomicUsize::new(0));
        let observed = count.clone();
        let app = Router::new().route(
            "/mcp",
            post(move |Json(request): Json<Value>| {
                let response = response.clone();
                let observed = observed.clone();
                async move {
                    if request["method"] == "notifications/initialized" {
                        observed.fetch_add(1, Ordering::SeqCst);
                        return notification_status.into_response();
                    }
                    if sse {
                        let events = [
                            json!({"jsonrpc":"2.0","method":"notifications/message",
                                "params":{"level":"info","data":"fixture"}}),
                            response,
                        ];
                        let stream = futures::stream::iter(events.map(|event| {
                            Ok::<_, std::convert::Infallible>(
                                Event::default().data(event.to_string()),
                            )
                        }));
                        Sse::new(stream).into_response()
                    } else {
                        Json(response).into_response()
                    }
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/mcp", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        std::fs::create_dir_all(".ci-work").unwrap();
        let root = tempfile::tempdir_in(".ci-work").unwrap();
        let path = root.path().join("config/connections.json");
        crate::config::update(&path, |config| {
            config.mode = crate::config::Mode::Direct;
            Ok(())
        })
        .unwrap();
        let client = crate::http_route::McpRoute::new(crate::route::Route::new(path).unwrap())
            .unwrap()
            .with_initialization_verification();
        let mut transport = StreamableHttpClientTransport::with_client(
            client,
            StreamableHttpClientTransportConfig::with_uri(endpoint),
        );
        let outcome =
            tokio::time::timeout(Duration::from_secs(3), initialize(&mut transport)).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), transport.close()).await;
        server.abort();
        let _ = server.await;
        (
            outcome.expect("Bounded initialization fixture"),
            count.load(Ordering::SeqCst),
        )
    }

    #[tokio::test]
    async fn valid_json_response_sends_ready_exactly_once() {
        let (result, count) = wire_case(valid(), StatusCode::ACCEPTED, false).await;
        assert!(result.is_ok());
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn valid_sse_response_after_log_sends_ready_exactly_once() {
        let (result, count) = wire_case(valid(), StatusCode::ACCEPTED, true).await;
        assert!(result.is_ok());
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn mixed_response_envelopes_never_send_ready_on_json_or_sse() {
        for sse in [false, true] {
            for (key, value) in [
                (
                    "error",
                    json!({"code": -32602, "message": "private-fixture-marker"}),
                ),
                ("method", json!("ping")),
            ] {
                let mut response = valid();
                response[key] = value;
                let (result, count) = wire_case(response, StatusCode::ACCEPTED, sse).await;
                assert!(result.is_err());
                assert_eq!(count, 0);
            }
        }
    }

    #[test]
    fn sdk_normalization_cannot_replace_raw_envelope_validation() {
        let mut response = valid();
        response["error"] = json!({"code": -32602, "message": "rejected"});
        let normalized: rmcp::model::ServerJsonRpcMessage =
            serde_json::from_value(response.clone()).unwrap();
        assert!(validate(&response).is_err());
        assert!(validate(&serde_json::to_value(normalized).unwrap()).is_ok());
    }

    #[tokio::test]
    async fn wrong_response_id_never_sends_ready() {
        let mut response = valid();
        response["id"] = json!(999);
        let (result, count) = wire_case(response, StatusCode::ACCEPTED, false).await;
        assert!(result.is_err());
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn unsupported_version_never_sends_ready() {
        let mut response = valid();
        response["result"]["protocolVersion"] = json!("2099-01-01");
        let (result, count) = wire_case(response, StatusCode::ACCEPTED, false).await;
        assert!(result.is_err());
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn invalid_result_never_sends_ready() {
        for field in ["protocolVersion", "capabilities", "serverInfo"] {
            let mut response = valid();
            response["result"][field] = Value::Null;
            let (result, count) = wire_case(response, StatusCode::ACCEPTED, false).await;
            assert!(result.is_err());
            assert_eq!(count, 0);
        }
    }

    #[tokio::test]
    async fn explicit_initialization_error_never_sends_ready() {
        let response = json!({"jsonrpc":"2.0","id":1,
            "error":{"code":-32602,"message":"private-fixture-marker"}});
        let (result, count) = wire_case(response, StatusCode::ACCEPTED, false).await;
        let error = result.unwrap_err().to_string();
        assert!(!error.contains("private-fixture-marker"));
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn failed_ready_publication_is_not_success_and_is_not_replayed() {
        let (result, count) = wire_case(valid(), StatusCode::INTERNAL_SERVER_ERROR, false).await;
        assert!(result.is_err());
        assert_eq!(count, 1);
    }
}
