use super::event_limit::EventLimit;
use futures::StreamExt;
use http::{HeaderName, HeaderValue};
use rmcp::{
    model::{ClientJsonRpcMessage, ServerJsonRpcMessage},
    transport::streamable_http_client::{
        AuthRequiredError, InsufficientScopeError, StreamableHttpError, StreamableHttpPostResponse,
    },
};
use serde_json::Value;
use std::{borrow::Cow, collections::HashMap, sync::Arc};

type Failure = StreamableHttpError<reqwest::Error>;

fn invalid() -> Failure {
    StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
        "Invalid MCP initialization envelope; private response suppressed.",
    ))
}

fn envelope(bytes: &[u8]) -> Result<ServerJsonRpcMessage, Failure> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    let object = value.as_object().ok_or_else(invalid)?;
    let forms = ["method", "result", "error"]
        .iter()
        .filter(|key| object.contains_key(**key))
        .count();
    if forms != 1 {
        return Err(invalid());
    }
    serde_json::from_value(value).map_err(|_| invalid())
}

fn scope(header: &str) -> Option<String> {
    let start = header.to_ascii_lowercase().find("scope=")? + 6;
    let value = &header[start..];
    if let Some(quoted) = value.strip_prefix('"') {
        return Some(quoted.split_once('"')?.0.to_owned());
    }
    let value = value.split([',', ';', ' ', '\t']).next()?;
    (!value.is_empty()).then(|| value.to_owned())
}

pub(super) async fn post(
    client: &reqwest::Client,
    uri: Arc<str>,
    message: ClientJsonRpcMessage,
    session: Option<Arc<str>>,
    auth: Option<String>,
    options: (HashMap<HeaderName, HeaderValue>, usize),
) -> Result<StreamableHttpPostResponse, Failure> {
    let (headers, max) = options;
    let mut request = client.post(uri.as_ref()).header(
        reqwest::header::ACCEPT,
        "text/event-stream, application/json",
    );
    for (name, value) in headers {
        if matches!(name.as_str(), "accept" | "mcp-session-id" | "last-event-id") {
            return Err(StreamableHttpError::ReservedHeaderConflict(
                name.to_string(),
            ));
        }
        request = request.header(name, value);
    }
    if let Some(auth) = auth {
        request = request.bearer_auth(auth);
    }
    let attached = session.is_some();
    if let Some(session) = session {
        request = request.header("mcp-session-id", session.as_ref());
    }
    let mut response = request
        .json(&message)
        .send()
        .await
        .map_err(Failure::Client)?;
    let status = response.status();
    if matches!(status.as_u16(), 401 | 403)
        && let Some(challenge) = response.headers().get(reqwest::header::WWW_AUTHENTICATE)
    {
        let challenge = challenge.to_str().map_err(|_| invalid())?.to_owned();
        return Err(if status.as_u16() == 401 {
            StreamableHttpError::AuthRequired(AuthRequiredError::new(challenge))
        } else {
            let required_scope = scope(&challenge);
            StreamableHttpError::InsufficientScope(InsufficientScopeError::new(
                challenge,
                required_scope,
            ))
        });
    }
    if status.as_u16() == 404 && attached {
        return Err(StreamableHttpError::SessionExpired);
    }
    let session = response
        .headers()
        .get("mcp-session-id")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let content = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if status.is_success() && content.starts_with("text/event-stream") {
        let chunks = response
            .bytes_stream()
            .scan(EventLimit::default(), move |limit, chunk| {
                let chunk =
                    chunk.map_err(|_| std::io::Error::other("Initialization stream failed."));
                let bounded = chunk.and_then(|bytes| {
                    if limit.observe(&bytes, max) {
                        Ok(bytes)
                    } else {
                        Err(std::io::Error::other("Initialization event too large."))
                    }
                });
                futures::future::ready(Some(bounded))
            });
        let stream = sse_stream::SseStream::from_bytes_stream(chunks).map(|event| {
            let event = event?;
            if let Some(data) = &event.data
                && !data.trim().is_empty()
            {
                envelope(data.as_bytes()).map_err(|_| {
                    sse_stream::Error::Body(Box::new(std::io::Error::other(
                        "Invalid initialization envelope; private response suppressed.",
                    )))
                })?;
            }
            Ok(event)
        });
        return Ok(StreamableHttpPostResponse::Sse(stream.boxed(), session));
    }
    if !content.starts_with("application/json") {
        return Err(invalid());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(Failure::Client)? {
        if chunk.len() > max.saturating_sub(body.len()) {
            return Err(invalid());
        }
        body.extend_from_slice(&chunk);
    }
    let message = envelope(&body)?;
    if !status.is_success() && !matches!(message, ServerJsonRpcMessage::Error(_)) {
        return Err(invalid());
    }
    Ok(StreamableHttpPostResponse::Json(message, session))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_mixed_envelopes_are_rejected_before_sdk_normalization() {
        for mixed in [
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{},"error":{"code":-1,"message":"private"}}),
            serde_json::json!({"jsonrpc":"2.0","id":1,"result":{},"method":"ping"}),
        ] {
            assert!(envelope(&serde_json::to_vec(&mixed).unwrap()).is_err());
        }
    }
}
