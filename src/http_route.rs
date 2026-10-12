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
}
impl McpRoute {
    pub fn new(route: Route) -> crate::error::Result<Self> {
        Ok(Self {
            client: route.client(false, None)?,
            route,
            log: None,
        })
    }
    pub fn with_log(mut self, log: crate::logging::Log) -> Self {
        self.log = Some(log);
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
        self.guarded(
            "post",
            self.client
                .post_message(uri, message, session, auth, headers),
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
        self.guarded(
            "post",
            self.client
                .post_message_with_max_sse_event_size(uri, message, session, auth, headers, max),
        )
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
