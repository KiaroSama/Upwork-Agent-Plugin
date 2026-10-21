use crate::logging::Log;
use rmcp::transport::auth::OAuthHttpClientError;
use std::{error::Error, fmt};

pub(super) fn stage(request: &http::Request<Vec<u8>>) -> &'static str {
    if request.method() == http::Method::GET {
        return "metadata";
    }
    if request.method() == http::Method::POST {
        let kind = request
            .headers()
            .get(http::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .split(';')
            .next()
            .unwrap_or("")
            .trim();
        if kind.eq_ignore_ascii_case("application/x-www-form-urlencoded") {
            return "token";
        }
        if kind.eq_ignore_ascii_case("application/json") {
            return "registration";
        }
    }
    "http"
}

pub(super) fn event(
    log: Option<&Log>,
    stage: &'static str,
    phase: &'static str,
    class: &'static str,
    status: u16,
    context: &str,
) {
    if let Some(log) = log {
        log.event("WARNING", &format!("OAuth HTTP failure stage={stage} phase={phase} class={class} http_status={status} {context}"));
    }
}

// SDK error paths stringify adapter failures; never expose a reqwest URL or payload through Debug.
struct Failure {
    stage: &'static str,
    phase: &'static str,
    class: &'static str,
    status: u16,
}
impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "OAuth HTTP failure stage={} phase={} class={} http_status={}",
            self.stage, self.phase, self.class, self.status
        )
    }
}
impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
// The SDK formats full source chains; typed details were recorded above this boundary.
impl Error for Failure {}

pub(super) fn failure(
    log: Option<&Log>,
    stage: &'static str,
    phase: &'static str,
    status: u16,
    source: OAuthHttpClientError,
) -> OAuthHttpClientError {
    let network = source.downcast_ref::<reqwest::Error>();
    let class = network
        .map(crate::diagnostics::network)
        .unwrap_or(if phase == "route" {
            "route_changed"
        } else if matches!(
            source.downcast_ref::<crate::error::Error>(),
            Some(crate::error::Error::Safe("OAuth response too large."))
        ) {
            "response_too_large"
        } else {
            "invalid_response"
        });
    let context = network
        .map(crate::diagnostics::network_context)
        .unwrap_or_default();
    event(log, stage, phase, class, status, &context);
    Box::new(Failure {
        stage,
        phase,
        class,
        status,
    })
}
