mod device_context;
mod verification;

use crate::{
    auth,
    credentials::Store,
    error::{Error, Result},
    logging::Log,
    route::Route,
};
use futures::stream::{FuturesUnordered, StreamExt};
use rmcp::{
    model::{ClientJsonRpcMessage, ServerJsonRpcMessage, ServerRequest},
    transport::{
        AuthClient, StreamableHttpClientTransport, Transport,
        streamable_http_client::{StreamableHttpClient, StreamableHttpClientTransportConfig},
    },
};
use std::{path::Path, time::Duration};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn send_owned<T: Transport<rmcp::RoleClient>>(
    transport: &mut T,
    message: ClientJsonRpcMessage,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<(), T::Error>> + Send>>
{
    Box::pin(transport.send(message))
}

async fn read_bounded<R: tokio::io::AsyncRead + Unpin>(
    input: &mut BufReader<R>,
    line: &mut Vec<u8>,
) -> Result<usize> {
    loop {
        let buffer = input.fill_buf().await?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(0)
            } else {
                Err(Error::Safe("MCP input ended before its newline delimiter."))
            };
        }
        let count = buffer
            .iter()
            .position(|b| *b == b'\n')
            .map(|n| n + 1)
            .unwrap_or(buffer.len());
        if line.len() + count > 1024 * 1024 {
            return Err(Error::Safe("MCP input exceeds 1 MiB."));
        }
        let complete = buffer[count - 1] == b'\n';
        line.extend_from_slice(&buffer[..count]);
        input.consume(count);
        if complete {
            return Ok(line.len());
        }
    }
}

pub async fn relay<C, R, W>(
    client: C,
    route: Route,
    endpoint: &str,
    input: R,
    output: W,
) -> Result<()>
where
    C: StreamableHttpClient + Send + Sync,
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    relay_logged(client, route, endpoint, input, output, None).await
}

async fn relay_logged<C, R, W>(
    client: C,
    route: Route,
    endpoint: &str,
    input: R,
    mut output: W,
    log: Option<Log>,
) -> Result<()>
where
    C: StreamableHttpClient + Send + Sync,
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let config = StreamableHttpClientTransportConfig::with_uri(endpoint.to_owned())
        .max_concurrent_requests(2)
        .max_sse_event_size(1024 * 1024);
    let mut remote = StreamableHttpClientTransport::with_client(client, config);
    let mut input = BufReader::new(input);
    let mut line = Vec::new();
    let mut sends = FuturesUnordered::new();
    let mut pings = std::collections::HashSet::new();
    let mut ordinary = 0usize;
    let mut controls = 0usize;
    let result = route.guard(async {
        loop {
            tokio::select! {
                biased;
                Some((control,ping_reply,result))=sends.next(),if !sends.is_empty()=>{
                    if control { controls -= 1; } else { ordinary -= 1; }
                    if let Err(error) = result {
                        // The SDK releases this control slot and keeps the worker usable.
                        // Do not replay the reply or turn a heartbeat deadline into process death.
                        if ping_reply && matches!(error, rmcp::transport::streamable_http_client::StreamableHttpError::ControlRequestTimeout) {
                            if let Some(log) = &log {
                                log.event("WARNING", "MCP heartbeat reply class=control_post_timeout; same transport retained, no replay.");
                            }
                        } else {
                            return Err(Error::Transport(crate::diagnostics::transport(&error)));
                        }
                    }
                }
                message=remote.receive()=>{
                    let message=message.ok_or(Error::Safe("Remote MCP connection closed. Reconnect or run Sign in."))?;
                    route.check()?;
                    if let ServerJsonRpcMessage::Request(request) = &message
                        && matches!(request.request, ServerRequest::PingRequest(_))
                    {
                        if pings.len() >= 16 && !pings.contains(&request.id) {
                            return Err(Error::Safe("Too many unanswered MCP pings."));
                        }
                        pings.insert(request.id.clone());
                    }
                    let mut bytes=serde_json::to_vec(&message)?; bytes.push(b'\n');
                    output.write_all(&bytes).await?; output.flush().await?;
                }
                count=read_bounded(&mut input,&mut line)=>{
                    if count?==0 {return Ok(());}
                    let mut message:ClientJsonRpcMessage=serde_json::from_slice(&line)?;
                    line.clear(); route.check()?;
                    device_context::normalize(&mut message);
                    let control = matches!(&message, ClientJsonRpcMessage::Response(_) | ClientJsonRpcMessage::Error(_));
                    if (control && controls == 16) || (!control && ordinary == 16) {
                        return Err(Error::Safe("MCP relay admission limit exceeded; reconnect without replaying uncertain work."));
                    }
                    let ping_reply = match &message {
                        ClientJsonRpcMessage::Response(response) => pings.remove(&response.id),
                        ClientJsonRpcMessage::Error(error) => {
                            if let Some(id) = &error.id { pings.remove(id); }
                            false
                        }
                        _ => false,
                    };
                    if control { controls += 1; } else { ordinary += 1; }
                    let send = send_owned(&mut remote,message);
                    sends.push(async move { (control, ping_reply, send.await) });
                }
            }
        }
    }).await.and_then(|result| result);
    drop(sends);
    let _ = tokio::time::timeout(Duration::from_secs(2), remote.close()).await;
    result
}
pub async fn run(root: &Path, endpoint: &str, sign_in: bool) -> Result<()> {
    let log = Log::new(root, "upwork-bridge");
    log.event("INFO", "Native connector started.");
    let result = run_session(root, endpoint, sign_in, &log).await;
    if let Err(Error::Transport(class)) = &result {
        log.event(
            "ERROR",
            &format!("MCP relay failure class={class}; no request replay."),
        );
    }
    log.event(
        if result.is_ok() { "INFO" } else { "ERROR" },
        "Native connector stopped; no direct fallback.",
    );
    result
}

async fn run_session(root: &Path, endpoint: &str, sign_in: bool, log: &Log) -> Result<()> {
    let route = Route::new(root.join("config/connections.json"))?;
    let store = Store { root: root.into() };
    if !sign_in && !store.available()? {
        return Err(Error::Safe(
            "Upwork sign-in required. Open the menu and choose Sign in.",
        ));
    }
    let manager = if sign_in {
        tokio::select! {
            _=route.changed()=>return Err(Error::Safe("Route changed during sign-in; no session completion claimed.")),
            result=auth::sign_in(route.clone(),store.clone(),endpoint,log)=>result?,
        }
    } else {
        let mut m = auth::manager(route.clone(), store.clone(), endpoint).await?;
        tokio::select! {
            _=route.changed()=>return Err(Error::Safe("Route changed during authentication; reconnect MCP.")),
            result=auth::restore(&mut m,&store)=>result?,
        }
        m
    };
    if sign_in {
        manager.get_access_token().await?;
        let transport = StreamableHttpClientTransport::with_client(
            AuthClient::new(
                crate::http_route::McpRoute::new(route.clone())?
                    .with_log(log.clone())
                    .with_initialization_verification(),
                manager,
            ),
            StreamableHttpClientTransportConfig::with_uri(endpoint.to_owned()),
        );
        let mut transport = transport;
        let verification = tokio::time::timeout(Duration::from_secs(30), async {
            verification::initialize(&mut transport).await?;
            if store.load_local()?.and_then(|c| c.token_response).is_none() {
                return Err(Error::Safe("No usable session stored."));
            }
            Ok(())
        });
        let verified = tokio::select! {
            _=route.changed()=>Err(Error::Safe("Route changed during MCP verification; no completion claimed.")),
            result=verification=>result.map_err(|_|Error::Safe("MCP initialization timed out."))?,
        };
        let _ = tokio::time::timeout(Duration::from_secs(2), transport.close()).await;
        verified?;
        route.check()?;
        log.event(
            "INFO",
            "Sign-in complete; session saved and MCP initialization verified.",
        );
        println!("Sign-in complete. Session saved locally. Reconnect Upwork-Agent-mcp in Claude.");
        return Ok(());
    }
    let client = AuthClient::new(
        crate::http_route::McpRoute::new(route.clone())?.with_log(log.clone()),
        manager,
    );
    relay_logged(
        client,
        route,
        endpoint,
        tokio::io::stdin(),
        tokio::io::stdout(),
        Some(log.clone()),
    )
    .await
}

#[cfg(test)]
mod framing_tests {
    use super::*;

    #[tokio::test]
    async fn truncated_frame_errors_but_complete_frame_then_eof_is_clean() {
        let mut line = Vec::new();
        let mut truncated = BufReader::new(&b"{\"jsonrpc\":\"2.0\"}"[..]);
        let error = read_bounded(&mut truncated, &mut line).await.unwrap_err();
        assert!(matches!(
            error,
            Error::Safe("MCP input ended before its newline delimiter.")
        ));
        line.clear();
        let mut complete = BufReader::new(&b"{}\n"[..]);
        assert_eq!(read_bounded(&mut complete, &mut line).await.unwrap(), 3);
        assert_eq!(line, b"{}\n");
        line.clear();
        assert_eq!(read_bounded(&mut complete, &mut line).await.unwrap(), 0);
    }
}
