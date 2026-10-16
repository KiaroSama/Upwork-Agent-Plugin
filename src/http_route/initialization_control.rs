use futures::StreamExt;
use http::{HeaderName, HeaderValue};
use rmcp::{
    model::{ServerJsonRpcMessage, ServerRequest},
    transport::streamable_http_client::{StreamableHttpError, StreamableHttpPostResponse},
};
use serde_json::{Value, json};
use std::{borrow::Cow, collections::HashMap, sync::Arc, time::Duration};

type Failure = StreamableHttpError<reqwest::Error>;

pub(super) struct Context {
    pub client: reqwest::Client,
    pub uri: Arc<str>,
    pub session: Option<Arc<str>>,
    pub auth: Option<String>,
    pub headers: HashMap<HeaderName, HeaderValue>,
}

fn invalid(message: &'static str) -> Failure {
    StreamableHttpError::UnexpectedServerResponse(Cow::Borrowed(message))
}

impl Context {
    async fn ping(&self, id: Value, session: Option<&str>) -> Result<(), Failure> {
        // This control response bypasses initialization interception and the SDK
        // request queue. Retain the exact wire ID, including numeric strings.
        let mut request = self.client.post(self.uri.as_ref());
        for (name, value) in &self.headers {
            request = request.header(name, value);
        }
        // Until InitializeResult arrives, the offered version is not negotiated.
        // Preserve an explicitly supplied version header, but never invent one.
        request = request.header("accept", "application/json, text/event-stream");
        if let Some(auth) = &self.auth {
            request = request.bearer_auth(auth);
        }
        if let Some(session) = session.or(self.session.as_deref()) {
            request = request.header("mcp-session-id", session);
        }
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            request
                .json(&json!({"jsonrpc":"2.0","id":id,"result":{}}))
                .send(),
        )
        .await
        .map_err(|_| invalid("Initialization ping reply timed out; no replay."))?
        .map_err(Failure::Client)?;
        if response.status() != reqwest::StatusCode::ACCEPTED {
            return Err(invalid(
                "Initialization ping reply was not accepted; no replay.",
            ));
        }
        Ok(())
    }
}

pub(super) async fn first_terminal(
    response: StreamableHttpPostResponse,
    context: Option<Context>,
) -> Result<StreamableHttpPostResponse, Failure> {
    let StreamableHttpPostResponse::Sse(mut stream, session) = response else {
        return Ok(response);
    };
    let context = context.ok_or_else(|| invalid("Initialization context missing."))?;
    let mut events = 0usize;
    let mut pings = 0usize;
    while let Some(event) = stream.next().await {
        events += 1;
        if events > 256 {
            return Err(invalid("Initialization control-event limit exceeded."));
        }
        let event = event?;
        let payload = event.data.unwrap_or_default();
        if payload.trim().is_empty() {
            tokio::task::yield_now().await;
            continue;
        }
        let raw: Value = serde_json::from_str(&payload)
            .map_err(|_| invalid("Invalid initialization response; private data suppressed."))?;
        let message = super::initialization::envelope(payload.as_bytes())
            .map_err(|_| invalid("Invalid initialization response; private data suppressed."))?;
        match message {
            ServerJsonRpcMessage::Response(_) | ServerJsonRpcMessage::Error(_) => {
                // Preserve the first terminal result/error and the original
                // session. Never rescue an error with a later success.
                return Ok(StreamableHttpPostResponse::Json(message, session));
            }
            ServerJsonRpcMessage::Request(request)
                if matches!(request.request, ServerRequest::PingRequest(_)) =>
            {
                pings += 1;
                if pings > 16 {
                    return Err(invalid("Initialization ping limit exceeded."));
                }
                let id = raw
                    .get("id")
                    .cloned()
                    .ok_or_else(|| invalid("Invalid ping ID."))?;
                context.ping(id, session.as_deref()).await?;
            }
            ServerJsonRpcMessage::Request(_) => {
                return Err(invalid(
                    "Unsupported request before initialization completed.",
                ));
            }
            _ => {}
        }
        // Finite buffered notifications must not starve the outer route guard.
        tokio::task::yield_now().await;
    }
    Err(StreamableHttpError::UnexpectedEndOfStream)
}
