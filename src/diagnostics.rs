use rmcp::transport::{auth::AuthError, streamable_http_client::StreamableHttpError};

pub fn auth(error: &AuthError) -> &'static str {
    match error {
        AuthError::AuthorizationRequired => "authorization_required",
        AuthError::TokenRefreshRejected(_) => "refresh_rejected",
        AuthError::TokenRefreshFailed(_) => "refresh_failed",
        AuthError::CredentialStoreError(_) => "credential_store",
        AuthError::HttpError(_) => "oauth_http",
        AuthError::MetadataError(_) => "oauth_metadata",
        AuthError::AuthorizationFailed(_) => "authorization_failed",
        AuthError::TokenExchangeFailed(_) => "token_exchange_failed",
        AuthError::OAuthError(_) => "oauth_protocol",
        AuthError::PkceUnsupported => "pkce_unsupported",
        AuthError::UrlError(_) => "oauth_url",
        AuthError::NoAuthorizationSupport => "authorization_unsupported",
        AuthError::InternalError(_) => "oauth_internal",
        AuthError::InvalidTokenType(_) => "invalid_token_type",
        AuthError::TokenExpired => "token_expired",
        AuthError::InvalidScope(_) => "invalid_scope",
        AuthError::RegistrationFailed(_) => "registration_failed",
        AuthError::InsufficientScope { .. } => "insufficient_scope",
        AuthError::AuthorizationServerMismatch { .. } => "issuer_mismatch",
        AuthError::AuthorizationServerMissingIssuer { .. } => "issuer_missing",
        AuthError::ClientCredentialsError(_) => "client_credentials",
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
    } else if error.is_dns() {
        "dns"
    } else if error.is_connect() {
        "proxy_or_connect"
    } else if error.is_body() {
        "response_body"
    } else if error.is_decode() {
        "response_decode"
    } else if error.is_status() {
        "http_status"
    } else if error.is_builder() {
        "request_builder"
    } else if error.is_redirect() {
        "redirect"
    } else if error.is_request() {
        "request_failed"
    } else {
        "network_other"
    }
}

pub(crate) fn network_io_kind(error: &reqwest::Error) -> Option<std::io::ErrorKind> {
    use std::error::Error;
    let mut source: Option<&(dyn Error + 'static)> = Some(error);
    // Inspect types only; source messages may contain proxy credentials or URLs.
    for _ in 0..16 {
        let Some(current) = source else { break };
        if let Some(io) = current.downcast_ref::<std::io::Error>() {
            return Some(io.kind());
        }
        source = current.source();
    }
    None
}

pub fn network_context(error: &reqwest::Error) -> String {
    let io_kind = network_io_kind(error);
    let status = error.status().map_or(0, |value| value.as_u16());
    format!(
        "timeout={} connect={} dns={} request={} body={} decode={} builder={} redirect={} status={} io_kind={:?}",
        error.is_timeout(),
        error.is_connect(),
        error.is_dns(),
        error.is_request(),
        error.is_body(),
        error.is_decode(),
        error.is_builder(),
        error.is_redirect(),
        status,
        io_kind,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    #[test]
    fn oauth_labels_distinguish_safe_operation_failures() {
        let private = "sentinel-token https://private.invalid/?code=sentinel";
        let errors = [
            AuthError::TokenExchangeFailed(private.into()),
            AuthError::RegistrationFailed(private.into()),
            AuthError::AuthorizationFailed(private.into()),
            AuthError::InvalidScope(private.into()),
        ];
        assert_eq!(
            errors.each_ref().map(auth),
            [
                "token_exchange_failed",
                "registration_failed",
                "authorization_failed",
                "invalid_scope"
            ]
        );
    }

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
