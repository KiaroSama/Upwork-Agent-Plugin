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
}
impl McpRoute {
    pub fn new(route: Route) -> crate::error::Result<Self> {
        Ok(Self {
            client: route.client(false, None)?,
            route,
        })
    }
    async fn guarded<T>(
        &self,
        future: impl std::future::Future<Output = Result<T, StreamableHttpError<reqwest::Error>>>,
    ) -> Result<T, StreamableHttpError<reqwest::Error>> {
        let changed = || {
            StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(
                "Route changed; MCP operation blocked.",
            ))
        };
        self.route.guard(future).await.map_err(|_| changed())?
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
        self.guarded(self.client.delete_session(uri, session, auth, headers))
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
        self.guarded(self.client.get_stream(uri, session, last, auth, headers))
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
            self.client
                .get_stream_with_max_sse_event_size(uri, session, last, auth, headers, max),
        )
        .await
    }
}
