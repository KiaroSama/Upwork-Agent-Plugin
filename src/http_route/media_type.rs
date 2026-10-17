use http::{HeaderMap, header::CONTENT_TYPE};
use rmcp::transport::streamable_http_client::StreamableHttpError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kind {
    Json,
    EventStream,
}

fn ows(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|b| !matches!(b, b' ' | b'\t'))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|b| !matches!(b, b' ' | b'\t'))
        .map_or(start, |i| i + 1);
    &bytes[start..end]
}

fn token(rest: &mut &[u8]) -> bool {
    let length = rest
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(b))
        .count();
    *rest = &rest[length..];
    length != 0
}

fn quoted(rest: &mut &[u8]) -> bool {
    // The opening quote has already been consumed. Validate quoted-pair and
    // qdtext without decoding, logging or changing the parameter's bytes.
    while let Some((&byte, tail)) = rest.split_first() {
        *rest = tail;
        match byte {
            b'"' => return true,
            b'\\' => {
                let Some((&escaped, tail)) = rest.split_first() else {
                    return false;
                };
                if !(escaped == b'\t' || (escaped >= b' ' && escaped != 0x7f)) {
                    return false;
                }
                *rest = tail;
            }
            b'\t' | b' ' | b'!' | b'#'..=b'[' | b']'..=b'~' | 0x80..=0xff => {}
            _ => return false,
        }
    }
    false
}

fn parameters(mut rest: &[u8]) -> bool {
    // RFC 9110 sections 5.6.6 and 8.3.1: OWS around semicolons, token names,
    // token or quoted values, and empty parameter slots. Commas are not a list
    // separator for Content-Type and are allowed only inside quoted values.
    while !rest.is_empty() {
        if rest[0] != b';' {
            return false;
        }
        rest = ows(&rest[1..]);
        if rest.is_empty() || rest[0] == b';' {
            continue;
        }
        if !token(&mut rest) || rest.first() != Some(&b'=') {
            return false;
        }
        rest = &rest[1..];
        if rest.first() == Some(&b'"') {
            rest = &rest[1..];
            if !quoted(&mut rest) {
                return false;
            }
        } else if !token(&mut rest) {
            return false;
        }
        rest = ows(rest);
        if !rest.is_empty() && rest[0] != b';' {
            return false;
        }
    }
    true
}

pub(super) fn classify(headers: &HeaderMap) -> Result<Kind, StreamableHttpError<reqwest::Error>> {
    let mut values = headers.get_all(CONTENT_TYPE).iter();
    let value = values.next().ok_or_else(invalid)?;
    if values.next().is_some() {
        return Err(invalid());
    }
    let bytes = ows(value.as_bytes());
    let separator = bytes.iter().position(|b| *b == b';').unwrap_or(bytes.len());
    let essence = ows(&bytes[..separator]);
    let kind = if essence.eq_ignore_ascii_case(b"application/json") {
        Kind::Json
    } else if essence.eq_ignore_ascii_case(b"text/event-stream") {
        Kind::EventStream
    } else {
        return Err(invalid());
    };
    if !parameters(&bytes[separator..]) {
        return Err(invalid());
    }
    Ok(kind)
}

fn invalid() -> StreamableHttpError<reqwest::Error> {
    StreamableHttpError::UnexpectedContentType(Some("unsupported or malformed media type".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;

    #[test]
    fn media_types_are_exact_case_insensitive_and_parameter_aware() {
        for (value, expected) in [
            ("application/json", Kind::Json),
            ("Application/JSON", Kind::Json),
            ("APPLICATION/JSON; Charset=\"utf-8\"", Kind::Json),
            (
                " application/json \t; charset=utf-8 \t; fixture=ok ",
                Kind::Json,
            ),
            ("application/json; fixture=\"\"; ;", Kind::Json),
            ("application/json; fixture=\"a\\\"b\\\\c\"", Kind::Json),
            ("text/event-stream", Kind::EventStream),
            ("Text/Event-Stream; charset=utf-8", Kind::EventStream),
            (
                "text/event-stream; fixture=\"comma,semicolon;value\"",
                Kind::EventStream,
            ),
        ] {
            let headers =
                HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_str(value).unwrap())]);
            assert_eq!(classify(&headers).unwrap(), expected);
        }
        let headers = HeaderMap::from_iter([(
            CONTENT_TYPE,
            HeaderValue::from_bytes(b"application/json; x=\"\xff\"").unwrap(),
        )]);
        assert_eq!(classify(&headers).unwrap(), Kind::Json);
    }

    #[test]
    fn lookalikes_lists_invalid_parameters_and_missing_headers_fail_safely() {
        for value in [
            "application/jsonp",
            "application/json-extra",
            "application/json+fixture",
            "text/event-stream-extra",
            "application/problem+json",
            "text/plain",
            "*/*",
            "application/json, text/event-stream",
            "application/json; =bad",
            "application/json; x=\"unterminated",
            "application/json private-fixture-marker",
            "application/json; x=",
            "application/json; x =ok",
            "application/json; x= ok",
            "application/json; x=\"ok\"trailing",
            "application/json; flag",
            "application/json; x=\"trailing\\",
            "application/json; x=\"ok\", text/plain",
        ] {
            let headers =
                HeaderMap::from_iter([(CONTENT_TYPE, HeaderValue::from_str(value).unwrap())]);
            let error = classify(&headers).unwrap_err().to_string();
            assert!(!error.contains("private-fixture-marker"));
        }
        assert!(classify(&HeaderMap::new()).is_err());
        let mut headers = HeaderMap::new();
        headers.append(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.append(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        assert!(classify(&headers).is_err());
        let headers = HeaderMap::from_iter([(
            CONTENT_TYPE,
            HeaderValue::from_bytes(b"application/json; x=\xff").unwrap(),
        )]);
        assert!(classify(&headers).is_err());
    }
}
