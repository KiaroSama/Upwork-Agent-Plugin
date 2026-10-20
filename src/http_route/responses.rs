use super::{
    event_limit::EventLimit,
    initialization,
    media_type::{self, Kind},
};
use futures::StreamExt;
use http::{HeaderName, HeaderValue, StatusCode};
use rmcp::{
    model::{ClientJsonRpcMessage, ClientRequest, ErrorData, ServerJsonRpcMessage},
    transport::{
        common::client_side_sse::BoxedSseResponse,
        streamable_http_client::{
            AuthRequiredError, InsufficientScopeError, StreamableHttpError,
            StreamableHttpPostResponse,
        },
    },
};
use std::{borrow::Cow, collections::HashMap, sync::Arc};

type Failure = StreamableHttpError<reqwest::Error>;

// JSON and SSE are different framing policies. The SDK's SSE event option
// must not silently become a smaller limit for otherwise valid JSON replies.
const MAX_JSON_RESPONSE_BYTES: usize = 16 * 1024 * 1024;

fn rejected(message: &'static str) -> Failure {
    StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(message))
}

fn request(
    mut builder: reqwest::RequestBuilder,
    headers: HashMap<HeaderName, HeaderValue>,
    session: Option<&str>,
    auth: Option<&str>,
    last: Option<&str>,
) -> Result<reqwest::RequestBuilder, Failure> {
    for (name, value) in headers {
        if matches!(name.as_str(), "accept" | "mcp-session-id" | "last-event-id") {
            return Err(StreamableHttpError::ReservedHeaderConflict(
                name.to_string(),
            ));
        }
        builder = builder.header(name, value);
    }
    builder = builder.header("accept", "text/event-stream, application/json");
    if let Some(session) = session {
        builder = builder.header("mcp-session-id", session);
    }
    if let Some(auth) = auth {
        builder = builder.bearer_auth(auth);
    }
    if let Some(last) = last {
        builder = builder.header("last-event-id", last);
    }
    Ok(builder)
}

fn challenge(response: &reqwest::Response) -> Result<(), Failure> {
    let status = response.status();
    if matches!(status.as_u16(), 401 | 403)
        && let Some(header) = response.headers().get("www-authenticate")
    {
        let header = header
            .to_str()
            .map_err(|_| rejected("Invalid authentication challenge."))?
            .to_owned();
        return Err(if status == StatusCode::UNAUTHORIZED {
            StreamableHttpError::AuthRequired(AuthRequiredError::new(header))
        } else {
            let scope = initialization::scope(&header);
            StreamableHttpError::InsufficientScope(InsufficientScopeError::new(header, scope))
        });
    }
    Ok(())
}

async fn body(mut response: reqwest::Response, max: usize) -> Result<Vec<u8>, Failure> {
    if response
        .content_length()
        .is_some_and(|length| length > max as u64)
    {
        return Err(rejected(
            "MCP JSON response exceeds the configured message limit.",
        ));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(Failure::Client)? {
        if chunk.len() > max.saturating_sub(bytes.len()) {
            return Err(rejected(
                "MCP JSON response exceeds the configured message limit.",
            ));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

fn stream(
    response: reqwest::Response,
    max: usize,
    strict: bool,
    log: Option<crate::logging::Log>,
    stage: &'static str,
) -> BoxedSseResponse {
    let status = response.status().as_u16();
    let chunks = response.bytes_stream().scan(
        (EventLimit::default(), false),
        move |(limit, reported), chunk| {
            let chunk = chunk.map_err(|error| {
                let idle_eof = stage == "get"
                    && limit.at_boundary()
                    && crate::diagnostics::network_io_kind(&error)
                        == Some(std::io::ErrorKind::UnexpectedEof);
                if !*reported && let Some(log) = &log {
                    let recovery = if idle_eof { " recovery=sdk_get; no POST replay." } else { "" };
                    log.event(
                        if idle_eof { "WARNING" } else { "ERROR" },
                        &format!(
                            "MCP stream failure stage={stage} phase=body class={} http_status={status} {}{recovery}",
                            crate::diagnostics::network(&error),
                            crate::diagnostics::network_context(&error)
                        ),
                    );
                }
                *reported = true;
                // Preserve failure for SDK cursor-aware GET resumption. Never
                // turn a truncated TLS/HTTP body into successful completion.
                std::io::Error::other("MCP stream failed.")
            });
            let bounded = chunk.and_then(|bytes| {
                if limit.observe(&bytes, max) {
                    Ok(bytes)
                } else {
                    Err(std::io::Error::other(
                        "MCP event exceeds the configured message limit.",
                    ))
                }
            });
            futures::future::ready(Some(bounded))
        },
    );
    sse_stream::SseStream::from_bytes_stream(chunks)
        .map(move |event| {
            let event = event?;
            if strict
                && let Some(data) = &event.data
                && !data.trim().is_empty()
            {
                initialization::envelope(data.as_bytes()).map_err(|_| {
                    sse_stream::Error::Body(Box::new(std::io::Error::other(
                        "Invalid initialization envelope; private response suppressed.",
                    )))
                })?;
            }
            Ok(event)
        })
        .boxed()
}

pub(super) async fn post(
    client: &reqwest::Client,
    uri: Arc<str>,
    message: ClientJsonRpcMessage,
    session: Option<Arc<str>>,
    auth: Option<String>,
    options: (
        HashMap<HeaderName, HeaderValue>,
        usize,
        Option<crate::logging::Log>,
    ),
    strict: bool,
) -> Result<StreamableHttpPostResponse, Failure> {
    let (headers, max, log) = options;
    let initialize = matches!(&message, ClientJsonRpcMessage::Request(request)
        if matches!(request.request, ClientRequest::InitializeRequest(_)));
    let validate_envelope = strict || initialize;
    let json_max = if strict { max } else { MAX_JSON_RESPONSE_BYTES };
    let response = request(
        client.post(uri.as_ref()),
        headers,
        session.as_deref(),
        auth.as_deref(),
        None,
    )?
    .json(&message)
    .send()
    .await
    .map_err(Failure::Client)?;
    if !response.status().is_success()
        && let Some(log) = &log
    {
        log.event(
            "WARNING",
            &format!(
                "MCP HTTP response stage=post phase=headers status={}",
                response.status().as_u16()
            ),
        );
    }
    challenge(&response)?;
    let status = response.status();
    let attached = session.is_some();
    if status == StatusCode::NOT_FOUND && attached {
        return Err(StreamableHttpError::SessionExpired);
    }
    let expects_reply = matches!(&message, ClientJsonRpcMessage::Request(_));
    if !strict
        && (matches!(status.as_u16(), 202 | 204)
            || (status.is_success() && response.content_length() == Some(0) && !expects_reply))
    {
        return Ok(StreamableHttpPostResponse::Accepted);
    }
    let session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let kind = media_type::classify(response.headers());
    // Preserve the pinned SDK's correlated legacy discovery rejection without
    // copying a private response body into diagnostics or replaying the request.
    if !strict
        && !attached
        && status.is_client_error()
        && !matches!(status.as_u16(), 401 | 403)
        && let ClientJsonRpcMessage::Request(request) = &message
        && matches!(request.request, ClientRequest::DiscoverRequest(_))
    {
        let error = if kind.as_ref().is_ok_and(|kind| *kind == Kind::Json) {
            let bytes = body(response, json_max).await?;
            match serde_json::from_slice::<ServerJsonRpcMessage>(&bytes) {
                Ok(ServerJsonRpcMessage::Error(error)) => error.error,
                _ => {
                    ErrorData::invalid_request("Server discovery rejected by HTTP endpoint.", None)
                }
            }
        } else {
            ErrorData::invalid_request("Server discovery rejected by HTTP endpoint.", None)
        };
        return Ok(StreamableHttpPostResponse::Json(
            ServerJsonRpcMessage::error(error, Some(request.id.clone())),
            None,
        ));
    }
    // Non-MCP HTTP errors retain the established diagnostic class, without
    // reading an unbounded error body or including private response contents.
    let kind = kind.map_err(|error| {
        if status.is_success() {
            error
        } else {
            rejected("HTTP request failed without an acceptable MCP response.")
        }
    })?;
    match kind {
        Kind::EventStream if status.is_success() => Ok(StreamableHttpPostResponse::Sse(
            stream(response, max, validate_envelope, log, "post"),
            session,
        )),
        Kind::Json => {
            let bytes = body(response, json_max).await?;
            let parsed = if validate_envelope {
                initialization::envelope(&bytes)
            } else {
                serde_json::from_slice(&bytes)
                    .map_err(|_| rejected("Invalid MCP JSON response; private body suppressed."))
            };
            match parsed {
                Ok(message)
                    if status.is_success() || matches!(message, ServerJsonRpcMessage::Error(_)) =>
                {
                    Ok(StreamableHttpPostResponse::Json(message, session))
                }
                Err(_) if !validate_envelope && status.is_success() && !expects_reply => {
                    Ok(StreamableHttpPostResponse::Accepted)
                }
                _ => Err(rejected(
                    "HTTP response did not contain an acceptable MCP result.",
                )),
            }
        }
        _ => Err(rejected(
            "HTTP response did not contain an acceptable MCP result.",
        )),
    }
}

pub(super) async fn get(
    client: &reqwest::Client,
    uri: Arc<str>,
    session: Option<Arc<str>>,
    last: Option<String>,
    auth: Option<String>,
    options: (
        HashMap<HeaderName, HeaderValue>,
        usize,
        Option<crate::logging::Log>,
    ),
) -> Result<BoxedSseResponse, Failure> {
    let (headers, max, log) = options;
    let response = request(
        client.get(uri.as_ref()),
        headers,
        session.as_deref(),
        auth.as_deref(),
        last.as_deref(),
    )?
    .send()
    .await
    .map_err(Failure::Client)?;
    if response.status() == StatusCode::METHOD_NOT_ALLOWED {
        return Err(StreamableHttpError::ServerDoesNotSupportSse);
    }
    if !response.status().is_success()
        && let Some(log) = &log
    {
        log.event(
            "WARNING",
            &format!(
                "MCP HTTP response stage=get phase=headers status={}",
                response.status().as_u16()
            ),
        );
    }
    challenge(&response)?;
    response.error_for_status_ref().map_err(Failure::Client)?;
    // Retain the SDK's two accepted exact types on GET, including its legacy
    // application/json-labelled event-stream compatibility. Never accept prefixes.
    media_type::classify(response.headers())?;
    if let Some(log) = &log {
        log.event(
            "INFO",
            &format!(
                "MCP GET stream opened phase=headers status={}; SDK owns cursor-aware resumption, no POST replay.",
                response.status().as_u16()
            ),
        );
    }
    Ok(stream(response, max, false, log, "get"))
}
