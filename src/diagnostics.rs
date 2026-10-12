use rmcp::transport::{auth::AuthError, streamable_http_client::StreamableHttpError};

pub fn auth(error: &AuthError) -> &'static str {
    match error {
        AuthError::AuthorizationRequired => "authorization_required",
        AuthError::TokenRefreshRejected(_) => "refresh_rejected",
        AuthError::TokenRefreshFailed(_) => "refresh_failed",
        AuthError::CredentialStoreError(_) => "credential_store",
        AuthError::HttpError(_) => "oauth_http",
        AuthError::MetadataError(_) => "oauth_metadata",
        _ => "oauth_other",
    }
}

pub fn transport<E: std::error::Error + Send + Sync + 'static>(
    error: &StreamableHttpError<E>,
) -> &'static str {
    match error {
        StreamableHttpError::ControlRequestTimeout => "control_post_timeout",
        StreamableHttpError::SessionExpired => "session_expired",
        StreamableHttpError::SessionRecoveryTimeout => "session_recovery_timeout",
        StreamableHttpError::TransportChannelClosed => "transport_channel_closed",
        StreamableHttpError::UnexpectedEndOfStream => "stream_ended",
        StreamableHttpError::UnexpectedServerResponse(_) => "unexpected_http_response",
        StreamableHttpError::UnexpectedContentType(_) => "unexpected_content_type",
        StreamableHttpError::Client(_) => "http_client",
        StreamableHttpError::Auth(error) => auth(error),
        StreamableHttpError::AuthRequired(_) => "authorization_required",
        StreamableHttpError::InsufficientScope(_) => "insufficient_scope",
        StreamableHttpError::Sse(_) => "sse",
        StreamableHttpError::Io(_) => "io",
        StreamableHttpError::Deserialize(_) => "invalid_json",
        _ => "transport_other",
    }
}

pub fn network(error: &reqwest::Error) -> &'static str {
    if error.is_timeout() {
        "network_timeout"
    } else if error.is_connect() {
        "proxy_or_connect"
    } else if error.is_body() {
        "response_body"
    } else if error.is_decode() {
        "response_decode"
    } else if error.is_status() {
        "http_status"
    } else {
        "network_other"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn diagnostic_labels_never_include_untrusted_error_payloads() {
        let secret = "fixture-private-token https://private.example/";
        let errors: [StreamableHttpError<reqwest::Error>; 4] = [
            StreamableHttpError::UnexpectedServerResponse(Cow::Owned(secret.into())),
            StreamableHttpError::UnexpectedContentType(Some(secret.into())),
            StreamableHttpError::Auth(AuthError::CredentialStoreError(secret.into())),
            StreamableHttpError::ControlRequestTimeout,
        ];
        assert_eq!(
            errors.each_ref().map(transport),
            [
                "unexpected_http_response",
                "unexpected_content_type",
                "credential_store",
                "control_post_timeout"
            ]
        );
    }
}
