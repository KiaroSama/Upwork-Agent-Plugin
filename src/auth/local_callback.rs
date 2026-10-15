use crate::error::{Error, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpListener,
};
use url::Url;

const HEADER_LIMIT: usize = 8192;
const IO_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn stored_redirect(client: Option<&Value>) -> Result<Option<&str>> {
    client
        .map(|client| {
            client
                .get("redirect_uris")
                .and_then(Value::as_array)
                .and_then(|uris| uris.first())
                .and_then(Value::as_str)
                .filter(|uri| !uri.is_empty())
                .ok_or(Error::Safe("Saved client callback missing or invalid."))
        })
        .transpose()
}

fn target(raw: &str) -> Result<(Url, u16)> {
    let url = Url::parse(raw).map_err(|_| Error::Safe("Invalid local callback."))?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("localhost" | "127.0.0.1"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || raw.chars().any(char::is_control)
    {
        return Err(Error::Safe(
            "Callback must be a local HTTP address without credentials or fragment.",
        ));
    }
    // Url::port() omits a scheme's default even when explicitly written.
    // A stored redirect must bind its effective port, never an unrelated ephemeral one.
    let port = url
        .port_or_known_default()
        .filter(|port| *port != 0)
        .ok_or(Error::Safe("Saved callback port invalid."))?;
    Ok((url, port))
}

pub(super) async fn bind(saved: Option<&str>) -> Result<(TcpListener, String, Url)> {
    if let Some(raw) = saved {
        let (url, port) = target(raw)?;
        let listener = TcpListener::bind(("127.0.0.1", port)).await.map_err(|_| {
            Error::Safe("Cannot bind saved callback port; check permissions or another sign-in.")
        })?;
        // Preserve the registered URI byte-for-byte for OAuth redirect matching.
        return Ok((listener, raw.to_owned(), url));
    }
    let listener = TcpListener::bind(("127.0.0.1", 0)).await?;
    let raw = format!(
        "http://127.0.0.1:{}/oauth/callback",
        listener.local_addr()?.port()
    );
    let (url, _) = target(&raw)?;
    Ok((listener, raw, url))
}

pub(super) async fn read_headers<R: AsyncRead + Unpin>(socket: &mut R) -> Result<Vec<u8>> {
    // One deadline covers the entire header, including continuously arriving bytes.
    // Reading one byte at a time avoids consuming any body after the delimiter.
    tokio::time::timeout(IO_TIMEOUT, async {
        let mut bytes = Vec::with_capacity(256);
        let mut byte = [0];
        while bytes.len() < HEADER_LIMIT {
            socket
                .read_exact(&mut byte)
                .await
                .map_err(|_| Error::Safe("Incomplete local callback headers."))?;
            bytes.push(byte[0]);
            if bytes.ends_with(b"\r\n\r\n") {
                return Ok(bytes);
            }
        }
        Err(Error::Safe(
            "Local callback headers exceed the allowed size.",
        ))
    })
    .await
    .map_err(|_| Error::Safe("Local callback headers timed out."))?
}

pub(super) async fn write_response<W: AsyncWrite + Unpin>(socket: &mut W, bytes: &[u8]) {
    // Browser acknowledgement is best effort, not proof of token persistence.
    // A stalled or disconnected browser must not occupy the sign-in loop indefinitely.
    let _ = tokio::time::timeout(IO_TIMEOUT, socket.write_all(bytes)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::duplex;

    #[test]
    fn stored_default_http_ports_resolve_to_eighty() {
        for host in ["127.0.0.1", "localhost"] {
            for port in ["", ":80"] {
                let raw = format!("http://{host}{port}/oauth/callback");
                assert_eq!(target(&raw).unwrap().1, 80);
            }
        }
    }

    #[test]
    fn stored_nondefault_ports_and_business_query_are_preserved() {
        for port in [1, 1595, 443, 65535] {
            let raw = format!("http://127.0.0.1:{port}/oauth/callback?tenant=fixture%2Bvalue");
            let (url, actual) = target(&raw).unwrap();
            assert_eq!(actual, port);
            assert_eq!(url.query(), Some("tenant=fixture%2Bvalue"));
        }
    }

    #[test]
    fn unsafe_and_unusable_stored_targets_fail_without_private_details() {
        for raw in [
            "not a URL",
            "https://127.0.0.1:1234/callback",
            "http://example.invalid:1234/callback",
            "http://127.0.0.1:0/callback",
            "http://127.0.0.1:65536/callback",
            "http://private-marker@127.0.0.1/callback",
            "http://127.0.0.1/callback#private-marker",
            "http://127.0.0.1/call\nback",
        ] {
            let error = target(raw).unwrap_err().to_string();
            assert!(!error.contains("private-marker"));
        }
    }

    #[test]
    fn absent_registration_is_distinct_from_malformed_saved_callback() {
        assert_eq!(stored_redirect(None).unwrap(), None);
        for value in [
            Value::Null,
            json!({}),
            json!({"redirect_uris": []}),
            json!({"redirect_uris": [null]}),
            json!({"redirect_uris": [""]}),
            json!({"redirect_uris": "http://127.0.0.1/callback"}),
        ] {
            assert!(stored_redirect(Some(&value)).is_err());
        }
        let client = json!({"redirect_uris": ["http://localhost:80/oauth/callback"]});
        assert_eq!(
            stored_redirect(Some(&client)).unwrap(),
            Some("http://localhost:80/oauth/callback")
        );
    }

    #[tokio::test]
    async fn new_registration_uses_the_actual_ephemeral_loopback_port() {
        let (listener, raw, url) = bind(None).await.unwrap();
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback());
        assert_ne!(addr.port(), 0);
        assert_eq!(url.port_or_known_default(), Some(addr.port()));
        assert_eq!(
            raw,
            format!("http://127.0.0.1:{}/oauth/callback", addr.port())
        );
    }

    #[tokio::test]
    async fn occupied_saved_port_fails_without_ephemeral_fallback() {
        let occupied = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let raw = format!(
            "http://127.0.0.1:{}/oauth/callback",
            occupied.local_addr().unwrap().port()
        );
        assert!(bind(Some(&raw)).await.is_err());
    }

    #[tokio::test]
    async fn stored_uri_is_not_canonicalized_for_oauth_matching() {
        let first = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = first.local_addr().unwrap().port();
        drop(first);
        let raw = format!("http://LOCALHOST:{port}/oauth/callback?tenant=%2bFixture");
        let (listener, preserved, _) = bind(Some(&raw)).await.unwrap();
        assert_eq!(preserved, raw);
        assert_eq!(listener.local_addr().unwrap().port(), port);
    }

    #[tokio::test]
    async fn complete_header_does_not_consume_following_body() {
        let mut input = &b"GET /oauth/callback HTTP/1.1\r\nHost: localhost\r\n\r\nbody"[..];
        let bytes = read_headers(&mut input).await.unwrap();
        assert!(bytes.ends_with(b"\r\n\r\n"));
        assert_eq!(input, b"body");
    }

    #[tokio::test]
    async fn exact_limit_delimiter_is_accepted_but_oversize_is_rejected() {
        let mut exact = vec![b'x'; HEADER_LIMIT - 4];
        exact.extend_from_slice(b"\r\n\r\n");
        assert_eq!(
            read_headers(&mut &exact[..]).await.unwrap().len(),
            HEADER_LIMIT
        );
        let mut overflow = vec![b'x'; HEADER_LIMIT];
        overflow.extend_from_slice(b"\r\n\r\n");
        let mut input = &overflow[..];
        assert!(read_headers(&mut input).await.is_err());
        assert_eq!(input, b"\r\n\r\n");
    }

    #[tokio::test]
    async fn eof_and_incomplete_delimiters_are_rejected() {
        for bytes in [
            &b""[..],
            &b"GET / HTTP/1.1\r\n"[..],
            &b"\r\n\r"[..],
            &b"\n\n"[..],
        ] {
            assert!(read_headers(&mut &bytes[..]).await.is_err());
        }
    }

    #[tokio::test(start_paused = true)]
    async fn silent_connection_has_one_bounded_deadline() {
        let (_writer, mut reader) = duplex(32);
        let start = tokio::time::Instant::now();
        assert!(read_headers(&mut reader).await.is_err());
        assert_eq!(start.elapsed(), IO_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn dripped_bytes_cannot_renew_the_header_deadline() {
        let (mut writer, mut reader) = duplex(32);
        let sender = tokio::spawn(async move {
            loop {
                if writer.write_all(b"x").await.is_err() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        let start = tokio::time::Instant::now();
        let result = read_headers(&mut reader).await;
        sender.abort();
        let _ = sender.await;
        assert!(matches!(
            result,
            Err(Error::Safe("Local callback headers timed out."))
        ));
        assert_eq!(start.elapsed(), IO_TIMEOUT);
    }

    #[tokio::test(start_paused = true)]
    async fn delimiter_split_across_reads_completes_before_the_deadline() {
        let (mut writer, mut reader) = duplex(32);
        let sender = tokio::spawn(async move {
            for chunk in [&b"GET / HTTP/1.1\r\n"[..], &b"\r"[..], &b"\n"[..]] {
                writer.write_all(chunk).await.unwrap();
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        let result = read_headers(&mut reader).await;
        sender.abort();
        let _ = sender.await;
        assert_eq!(result.unwrap(), b"GET / HTTP/1.1\r\n\r\n");
    }

    #[tokio::test(start_paused = true)]
    async fn stalled_browser_acknowledgement_is_bounded() {
        let (mut writer, _reader) = duplex(1);
        let start = tokio::time::Instant::now();
        write_response(&mut writer, b"HTTP/1.1 200 OK\r\n\r\n").await;
        assert_eq!(start.elapsed(), IO_TIMEOUT);
    }

    #[tokio::test]
    async fn disconnected_browser_acknowledgement_does_not_panic() {
        let (mut writer, reader) = duplex(1);
        drop(reader);
        write_response(&mut writer, b"HTTP/1.1 200 OK\r\n\r\n").await;
    }
}
