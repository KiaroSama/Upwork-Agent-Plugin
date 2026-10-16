mod event_limit;
mod initialization;
mod initialization_control;
mod initialization_response;
#[cfg(test)]
mod ping_tests;

use crate::route::Route;
use http::{HeaderName, HeaderValue};
use rmcp::{
    model::ClientJsonRpcMessage,
    transport::{
        common::client_side_sse::BoxedSseResponse,
        streamable_http_client::{
            StreamableHttpClient, StreamableHttpError, StreamableHttpPostResponse,
        },
    },
};
use std::{borrow::Cow, collections::HashMap, sync::Arc};

#[derive(Clone)]
pub struct McpRoute {
    client: reqwest::Client,
    route: Route,
    log: Option<crate::logging::Log>,
    verify_initialization: bool,
}
impl McpRoute {
    pub fn new(route: Route) -> crate::error::Result<Self> {
        Ok(Self {
            client: route.client(false, None)?,
            route,
            log: None,
            verify_initialization: false,
        })
    }
    pub fn with_log(mut self, log: crate::logging::Log) -> Self {
        self.log = Some(log);
        self
    }
    pub(crate) fn with_initialization_verification(mut self) -> Self {
        self.verify_initialization = true;
        self
    }
    async fn guarded<T>(
        &self,
        stage: &'static str,
        future: impl std::future::Future<Output = Result<T, StreamableHttpError<reqwest::Error>>>,
    ) -> Result<T, StreamableHttpError<reqwest::Error>> {
        let changed = || {
            StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
                "Route changed; MCP operation blocked.",
            ))
        };
        let started = std::time::Instant::now();
        let result = self.route.guard(future).await.map_err(|_| changed())?;
        if let (Err(error), Some(log)) = (&result, &self.log) {
            let class = match error {
                StreamableHttpError::Client(error) => crate::diagnostics::network(error),
                _ => crate::diagnostics::transport(error),
            };
            log.event(
                "ERROR",
                &format!(
                    "MCP HTTP failure stage={stage} class={class} elapsed_ms={}",
                    started.elapsed().as_millis()
                ),
            );
        }
        result
    }
}
impl StreamableHttpClient for McpRoute {
    type Error = reqwest::Error;
    async fn post_message(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session: Option<Arc<str>>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        self.post_message_with_max_sse_event_size(
            uri,
            message,
            session,
            auth,
            headers,
            16 * 1024 * 1024,
        )
        .await
    }
    async fn post_message_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        message: ClientJsonRpcMessage,
        session: Option<Arc<str>>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        max: usize,
    ) -> Result<StreamableHttpPostResponse, StreamableHttpError<Self::Error>> {
        let is_initialize = matches!(&message, ClientJsonRpcMessage::Request(request)
            if matches!(request.request, rmcp::model::ClientRequest::InitializeRequest(_)));
        self.guarded("post", async {
            let context = if is_initialize {
                let value = serde_json::to_value(&message).map_err(|_| {
                    StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
                        "Cannot encode initialization context.",
                    ))
                })?;
                let protocol = value["params"]["protocolVersion"]
                    .as_str()
                    .ok_or_else(|| {
                        StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
                            "Initialization protocol missing.",
                        ))
                    })?
                    .to_owned();
                Some(initialization_control::Context {
                    client: self.client.clone(),
                    uri: uri.clone(),
                    session: session.clone(),
                    auth: auth.clone(),
                    headers: headers.clone(),
                    protocol,
                })
            } else {
                None
            };
            let exchange = async {
                let response = if self.verify_initialization && is_initialize {
                    initialization::post(&self.client, uri, message, session, auth, (headers, max))
                        .await?
                } else {
                    self.client
                        .post_message_with_max_sse_event_size(
                            uri, message, session, auth, headers, max,
                        )
                        .await?
                };
                if is_initialize {
                    // The SDK's SSE initializer ignores JSON-RPC errors. Select the
                    // first terminal reply before that filter, on both client paths.
                    initialization_response::first_terminal(response, context).await
                } else {
                    Ok(response)
                }
            };
            if is_initialize {
                tokio::time::timeout(std::time::Duration::from_secs(30), exchange)
                    .await
                    .map_err(|_| {
                        StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
                            "MCP initialization deadline exceeded; no replay.",
                        ))
                    })?
            } else {
                exchange.await
            }
        })
        .await
    }
    async fn delete_session(
        &self,
        uri: Arc<str>,
        session: Arc<str>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<(), StreamableHttpError<Self::Error>> {
        self.guarded(
            "delete",
            self.client.delete_session(uri, session, auth, headers),
        )
        .await
    }
    async fn get_stream(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last: Option<String>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        self.guarded(
            "get",
            self.client.get_stream(uri, session, last, auth, headers),
        )
        .await
    }
    async fn get_stream_with_max_sse_event_size(
        &self,
        uri: Arc<str>,
        session: Option<Arc<str>>,
        last: Option<String>,
        auth: Option<String>,
        headers: HashMap<HeaderName, HeaderValue>,
        max: usize,
    ) -> Result<BoxedSseResponse, StreamableHttpError<Self::Error>> {
        self.guarded(
            "get",
            self.client
                .get_stream_with_max_sse_event_size(uri, session, last, auth, headers, max),
        )
        .await
    }
}
