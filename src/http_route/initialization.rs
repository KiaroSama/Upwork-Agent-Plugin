use http::{HeaderName, HeaderValue};
use rmcp::{
    model::{ClientJsonRpcMessage, ServerJsonRpcMessage},
    transport::streamable_http_client::{StreamableHttpError, StreamableHttpPostResponse},
};
use serde_json::Value;
use std::{borrow::Cow, collections::HashMap, sync::Arc};
type Failure = StreamableHttpError<reqwest::Error>;

fn invalid() -> Failure {
    StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
        "Invalid MCP initialization envelope; private response suppressed.",
    ))
}

pub(super) fn envelope(bytes: &[u8]) -> Result<ServerJsonRpcMessage, Failure> {
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

pub(super) fn scope(header: &str) -> Option<String> {
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
    super::responses::post(client, uri, message, session, auth, options, true).await
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
