use super::{
    child::{Result, require},
    fixture::{Request, State, json_reply, response},
};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};

pub fn result(case: &str, id: &Value) -> Value {
    let mut result = json!({"protocolVersion":"2025-11-25","capabilities":{},"serverInfo":{"name":"fixture","version":"1"}});
    match case {
        "null_info" => result["serverInfo"] = Value::Null,
        "wrong_version" => result["protocolVersion"] = json!("2099-01-01"),
        "missing_version" => {
            result.as_object_mut().unwrap().remove("protocolVersion");
        }
        "missing_capabilities" => {
            result.as_object_mut().unwrap().remove("capabilities");
        }
        "bad_info" => result["serverInfo"] = json!({}),
        "missing_info" => {
            result.as_object_mut().unwrap().remove("serverInfo");
        }
        "numeric_info" => result["serverInfo"] = json!(42),
        "bad_name" => result["serverInfo"]["name"] = json!(false),
        "bad_capabilities" => result["capabilities"] = json!({"tools":{"listChanged":"invalid"}}),
        "null_capabilities" => result["capabilities"] = Value::Null,
        "array_capabilities" => result["capabilities"] = json!([]),
        "null_version" => result["protocolVersion"] = Value::Null,
        "empty_version" => result["protocolVersion"] = json!(""),
        "numeric_version" => result["protocolVersion"] = json!(1),
        "stateless_version" => result["protocolVersion"] = json!("2026-07-28"),
        "bad_instructions" => result["instructions"] = json!(123),
        "optional_metadata" => {
            result["instructions"] = json!("fixture");
            result["serverInfo"]["title"] = json!("Fixture");
            result["capabilities"] = json!({"tools":{},"experimental":{"fixture":{}}});
        }
        _ => {
            if let Some(version) = case.strip_prefix("version_") {
                result["protocolVersion"] = json!(version);
            }
        }
    }
    let mut response = json!({"jsonrpc":"2.0","id":id,"result":result});
    match case {
        "wrong_id" => response["id"] = json!(999),
        "string_id" => response["id"] = json!("1"),
        "wrong_string_id" => response["id"] = json!("999"),
        "missing_id" => {
            response.as_object_mut().unwrap().remove("id");
        }
        "null_result" => response["result"] = Value::Null,
        "empty_result" => response["result"] = json!({}),
        "invalid_jsonrpc" => response["jsonrpc"] = json!("1.0"),
        "error" => {
            response = json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"fixture rejection"}})
        }
        _ => {}
    }
    response
}
pub fn event(value: &Value) -> Vec<u8> {
    format!("event: message\ndata: {value}\n\n").into_bytes()
}

pub async fn respond<W: AsyncWrite + Unpin>(
    stream: &mut W,
    request: &Request,
    state: &State,
) -> Result<bool> {
    let message = request.json()?;
    let (family, case) = state.update(|d| (d.family.clone(), d.case.clone()));
    if family == "ordinary" && case != "prepare" {
        return super::ordinary::post(stream, &message, state).await;
    }
    let method = message.get("method").and_then(Value::as_str);
    if method == Some("notifications/initialized") {
        state.update(|d| {
            d.initialized += 1;
            if family == "ping" && case != "prepare" {
                d.ready_version_ok &= request.header("mcp-protocol-version")
                    == Some(if case == "fallback" {
                        "2025-03-26"
                    } else {
                        "2025-11-25"
                    });
            }
        });
        json_reply(
            stream,
            Value::Null,
            if case == "notification_fail" {
                500
            } else {
                202
            },
        )
        .await?;
        state.mark("ready", true);
        return Ok(false);
    }
    if method.is_none() {
        state.update(|d| {
            d.pings += 1;
            if family == "ping" && case != "prepare" {
                d.valid &= message["id"] == d.expected
                    && message["result"] == json!({})
                    && request.header("mcp-session-id") == Some("gated-fixture")
                    && request.header("authorization") == Some("Bearer disposable-fixture-token")
                    && (request.header("mcp-protocol-version").is_none()
                        || (case == "fallback"
                            && request.header("mcp-protocol-version") == Some("2025-03-26")));
            }
        });
        let valid = state.update(|d| d.valid);
        json_reply(
            stream,
            Value::Null,
            if case == "reply_failure" {
                500
            } else if valid {
                202
            } else {
                400
            },
        )
        .await?;
        state.mark("answered", true);
        return Ok(false);
    }
    if family == "route" && method == Some("ping") {
        state.update(|d| d.ordinary += 1);
        json_reply(
            stream,
            json!({"jsonrpc":"2.0","id":message["id"],"result":{}}),
            200,
        )
        .await?;
        return Ok(false);
    }
    require(
        method == Some("initialize"),
        "Unexpected initialization method",
    )?;
    state.update(|d| {
        d.initializations += 1;
        d.responses += 1;
    });
    let value = result(&case, &message["id"]);
    if case == "prepare" {
        json_reply(stream, value, 200).await?;
        return Ok(false);
    }
    match family.as_str() {
        "extended" => extended(stream, value, &case).await,
        "terminal" => terminal(stream, &case, state).await,
        "media" => media(stream, value, &case, state).await,
        "ping" => ping(stream, &message["id"], &case, state).await,
        _ => {
            if matches!(case.as_str(), "log_first" | "ping_first") {
                let first = if case == "log_first" {
                    json!({"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"fixture"}})
                } else {
                    json!({"jsonrpc":"2.0","id":"fixture-ping","method":"ping"})
                };
                let body = [event(&first), event(&value)].concat();
                response(
                    stream,
                    200,
                    &[("Content-Type", "text/event-stream".into())],
                    &body,
                    Some(body.len()),
                )
                .await?;
            } else {
                json_reply(stream, value, 200).await?;
            }
            Ok(false)
        }
    }
}
async fn extended<W: AsyncWrite + Unpin>(
    stream: &mut W,
    mut value: Value,
    case: &str,
) -> Result<bool> {
    let valid = value.clone();
    if case.starts_with("mixed_error_null") {
        value["error"] = Value::Null;
    } else if case.starts_with("mixed_error") {
        value["error"] = json!({"code":-32602,"message":"private-fixture-marker"});
    } else if case.starts_with("mixed_method") {
        value["method"] = json!("ping");
    } else if case.starts_with("invalid_jsonrpc") {
        value["jsonrpc"] = json!("1.0");
    } else if case.starts_with("top_array") {
        value = json!([value]);
    } else if case == "error_then_valid_sse" {
        value = json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"private-fixture-marker"}});
    } else if case == "invalid_version_then_valid_sse" {
        value["result"]["protocolVersion"] = json!("2099-01-01");
    }
    if case == "oversize_json" {
        value["result"]["instructions"] = json!("x".repeat(16 * 1024 * 1024));
    }
    let mut payload = serde_json::to_vec(&value)?;
    if case.starts_with("invalid_utf8") {
        payload.insert(0, 255);
    }
    if case == "malformed_then_valid_sse" {
        payload = b"{\"private-fixture-marker\":".to_vec();
    }
    let sse = case.ends_with("_sse") || case.starts_with("valid_sse");
    if sse {
        let delimiter = if case == "valid_sse_crlf" {
            "\r\n"
        } else {
            "\n"
        };
        payload = [
            format!("event: message{delimiter}data: ").into_bytes(),
            payload,
            delimiter.repeat(2).into_bytes(),
        ]
        .concat();
        if case.ends_with("then_valid_sse") {
            payload.extend_from_slice(
                &[
                    b"data: ".to_vec(),
                    serde_json::to_vec(&valid)?,
                    delimiter.repeat(2).into_bytes(),
                ]
                .concat(),
            );
        }
        if case == "valid_sse_comments" {
            payload = [
                ": heartbeat\n".repeat(1000).into_bytes(),
                b"\n".to_vec(),
                payload,
            ]
            .concat();
        }
    }
    response(
        stream,
        200,
        &[
            (
                "Content-Type",
                if sse {
                    "text/event-stream"
                } else {
                    "application/json"
                }
                .into(),
            ),
            ("Connection", "close".into()),
        ],
        &payload,
        Some(payload.len()),
    )
    .await?;
    Ok(true)
}
pub const TERMINAL_CASES: [&str; 8] = [
    "json_success",
    "json_error",
    "error_then_success_sse",
    "split_error_then_success_sse",
    "string_error_then_success_sse",
    "error_only_closed_sse",
    "error_only_open_sse",
    "success_then_error_sse",
];
async fn terminal<W: AsyncWrite + Unpin>(
    stream: &mut W,
    case: &str,
    state: &State,
) -> Result<bool> {
    let success = result("valid", &json!(1));
    let mut error =
        json!({"jsonrpc":"2.0","id":1,"error":{"code":-32602,"message":"private-fixture-marker"}});
    if case == "string_error_then_success_sse" {
        error["id"] = json!("1");
    }
    if case.starts_with("json_") {
        json_reply(
            stream,
            if case == "json_success" {
                success
            } else {
                error
            },
            200,
        )
        .await?;
        return Ok(false);
    }
    let first = event(if case == "success_then_error_sse" {
        &success
    } else {
        &error
    });
    let second = if case.starts_with("error_only") {
        Vec::new()
    } else {
        event(if case == "success_then_error_sse" {
            &error
        } else {
            &success
        })
    };
    let open = case == "error_only_open_sse";
    response(
        stream,
        200,
        &[
            ("Content-Type", "text/event-stream".into()),
            ("Connection", "close".into()),
        ],
        &first,
        if open {
            None
        } else {
            Some(first.len() + second.len())
        },
    )
    .await?;
    if case == "split_error_then_success_sse" {
        // Deliberate split models a later terminal event, never readiness polling.
        let _ = state.wait("stop", Duration::from_millis(250)).await;
    }
    stream.write_all(&second).await?;
    stream.flush().await?;
    if open {
        let _ = state.wait("stop", Duration::from_secs(8)).await;
    }
    Ok(true)
}
pub const MEDIA_CASES: [&str; 14] = [
    "json",
    "json_large",
    "json_upper",
    "json_parameters",
    "sse",
    "sse_upper",
    "json_prefix",
    "sse_prefix",
    "json_suffix",
    "combined",
    "invalid_parameter",
    "duplicate",
    "missing",
    "oversized",
];
pub fn media_positive(case: &str) -> bool {
    matches!(
        case,
        "json" | "json_large" | "json_upper" | "json_parameters" | "sse" | "sse_upper"
    )
}
async fn media<W: AsyncWrite + Unpin>(
    stream: &mut W,
    value: Value,
    case: &str,
    state: &State,
) -> Result<bool> {
    let media = match case {
        "json_upper" => "Application/JSON",
        "json_parameters" => "APPLICATION/JSON; Charset=\"utf-8\"",
        "sse" => "text/event-stream",
        "sse_upper" => "Text/Event-Stream; charset=utf-8",
        "json_prefix" => "application/json-extra",
        "sse_prefix" => "text/event-stream-extra",
        "json_suffix" => "application/json+fixture",
        "combined" => "application/json, text/plain",
        "invalid_parameter" => "application/json; x=\"unterminated",
        _ => "application/json",
    };
    let mut body = if case.starts_with("sse") {
        [
            b"data: ".to_vec(),
            serde_json::to_vec(&value)?,
            b"\n\n".to_vec(),
        ]
        .concat()
    } else {
        serde_json::to_vec(&value)?
    };
    if case == "json_large" {
        body.resize(2 * 1024 * 1024, b' ');
    }
    if case == "oversized" {
        body.resize(16 * 1024 * 1024 + 1, b' ');
    }
    state.update(|d| d.response_bytes = body.len());
    let mut headers = vec![("Connection", "close".into())];
    if case != "missing" {
        headers.push(("Content-Type", media.into()));
    }
    if case == "duplicate" {
        headers.push(("Content-Type", "text/plain".into()));
    }
    response(stream, 200, &headers, &body, Some(body.len())).await?;
    Ok(true)
}
async fn ping<W: AsyncWrite + Unpin>(
    stream: &mut W,
    id: &Value,
    case: &str,
    state: &State,
) -> Result<bool> {
    response(
        stream,
        200,
        &[
            ("Content-Type", "text/event-stream".into()),
            ("Mcp-Session-Id", "gated-fixture".into()),
            ("Connection", "close".into()),
        ],
        &[],
        None,
    )
    .await?;
    let ids = match case {
        "numeric" => vec![json!(7)],
        "string" => vec![json!("ping-a")],
        "numeric_string" => vec![json!("007")],
        "multiple" => vec![json!(7), json!("7")],
        "overflow" => (0..17).map(|i| json!(i)).collect(),
        "no_ping" => vec![],
        _ => vec![json!("ping-a")],
    };
    if case == "log_first" {
        stream.write_all(&event(&json!({"jsonrpc":"2.0","method":"notifications/message","params":{"level":"info","data":"fixture"}}))).await?;
    }
    for id in ids {
        state.mark("answered", false);
        state.update(|d| d.expected = id.clone());
        stream
            .write_all(&event(&json!({"jsonrpc":"2.0","id":id,"method":"ping"})))
            .await?;
        stream.flush().await?;
        if state
            .wait("answered", Duration::from_millis(1500))
            .await
            .is_err()
        {
            state.mark("gate_expired", true);
            return Ok(true);
        }
        if !state.update(|d| d.valid) || case == "reply_failure" {
            return Ok(true);
        }
    }
    if case == "ping_then_error" {
        stream.write_all(&event(&json!({"jsonrpc":"2.0","id":id,"error":{"code":-32602,"message":"private-fixture-marker"}}))).await?;
    }
    let mut terminal = result("valid", id);
    if case == "fallback" {
        terminal["result"]["protocolVersion"] = json!("2025-03-26");
    }
    stream.write_all(&event(&terminal)).await?;
    stream.flush().await?;
    Ok(true)
}
