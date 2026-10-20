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
    // Split only outside quoted strings; other challenges cannot supply Bearer scope.
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    let mut parts = Vec::new();
    for (index, byte) in header.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if quoted && byte == b'\\' {
            escaped = true;
        } else if byte == b'"' {
            quoted = !quoted;
        } else if !quoted && byte == b',' {
            parts.push(header[start..index].trim());
            start = index + 1;
        }
        if byte.is_ascii_control() && byte != b'\t' {
            return None;
        }
    }
    if quoted || escaped {
        return None;
    }
    parts.push(header[start..].trim());
    let (mut bearer, mut found) = (false, None);
    let mut bearer_count = 0;
    let mut names = std::collections::HashSet::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        let mut parameter = part;
        if let Some(space) = part.find(|c: char| c.is_ascii_whitespace())
            && part.find('=').is_none_or(|equal| space < equal)
            && !part[space..].trim_start().starts_with('=')
        {
            bearer = part[..space].eq_ignore_ascii_case("Bearer");
            names.clear();
            if bearer {
                bearer_count += 1;
            }
            parameter = part[space..].trim();
            if parameter.is_empty() {
                continue;
            }
        }
        let Some((name, raw)) = parameter.split_once('=') else {
            if bearer && parameter != part {
                // A token68 challenge cannot acquire comma-separated auth parameters later.
                return None;
            }
            bearer = part.eq_ignore_ascii_case("Bearer");
            names.clear();
            if bearer {
                bearer_count += 1;
            }
            continue;
        };
        if !bearer {
            continue;
        }
        let name = name.trim();
        if !token(name) || !names.insert(name.to_ascii_lowercase()) {
            return None;
        }
        let raw = raw.trim();
        let value = if let Some(inner) = raw.strip_prefix('"') {
            let inner = inner.strip_suffix('"')?;
            let mut value = String::new();
            let mut chars = inner.chars();
            while let Some(c) = chars.next() {
                if c == '\\' {
                    value.push(chars.next()?);
                } else if c == '"' {
                    return None;
                } else {
                    value.push(c);
                }
            }
            value
        } else {
            if !token(raw) {
                return None;
            }
            raw.to_owned()
        };
        if name.eq_ignore_ascii_case("scope") {
            if value.is_empty() {
                return None;
            }
            found = Some(value);
        }
    }
    if bearer_count == 1 { found } else { None }
}

fn token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
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
) -> Result<StreamableHttpPostResponse, Failure> {
    super::responses::post(client, uri, message, session, auth, options, true).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_respects_scheme_parameters_quotes_and_ambiguity() {
        for (header, expected) in [
            ("Bearer scope=jobs", Some("jobs")),
            (
                "bEaReR error=insufficient_scope, SCOPE = \"jobs:read profile\"",
                Some("jobs:read profile"),
            ),
            ("Basic realm=other, Bearer scope=jobs", Some("jobs")),
            ("Bearer scope=jobs, Basic scope=other", Some("jobs")),
            ("Bearer xscope=jobs", None),
            ("Bearer error_description=\"scope=private\"", None),
            ("Basic scope=jobs", None),
            ("Bearer scope=\"\"", None),
            ("Bearer scope=jobs, scope=other", None),
            ("Bearer scope=jobs, Bearer realm=other", None),
            ("Bearer opaque, scope=jobs", None),
            ("Bearer scope=jobs, opaque", Some("jobs")),
            ("Bearer scope=jobs, Basic", Some("jobs")),
            ("Bearer scope=jobs, Bearer", None),
            ("Bearer opaque==, scope=jobs", None),
            ("Bearer realm=one, realm=two, scope=jobs", None),
            ("Bearer description=bad value, scope=jobs", None),
            ("Bearer, scope=jobs", Some("jobs")),
            ("Bearer   , scope=jobs", Some("jobs")),
            (
                "Bearer scope=\"comma,colon:read\"",
                Some("comma,colon:read"),
            ),
            ("Bearer scope=jobs, , Basic realm=other", Some("jobs")),
            ("Bearer scope=\"unfinished", None),
            ("Bearer scope=\"jobs\\\"read\"", Some("jobs\"read")),
        ] {
            assert_eq!(scope(header).as_deref(), expected);
        }
    }

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
