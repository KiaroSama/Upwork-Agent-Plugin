use crate::{
    auth,
    credentials::Store,
    error::{Error, Result},
    logging::Log,
    route::Route,
};
use futures::stream::{FuturesUnordered, StreamExt};
use rmcp::{
    model::ClientJsonRpcMessage,
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
    mut output: W,
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
    let result = route.guard(async {
        loop {
            tokio::select! {
                biased;
                count=read_bounded(&mut input,&mut line), if sends.len()<16=>{
                    if count?==0 {return Ok(());}
                    let message:ClientJsonRpcMessage=serde_json::from_slice(&line)?;
                    line.clear(); route.check()?; sends.push(send_owned(&mut remote,message));
                }
                Some(result)=sends.next(),if !sends.is_empty()=>{
                    if let Err(error) = result {
                        return Err(Error::Transport(crate::diagnostics::transport(&error)));
                    }
                }
                message=remote.receive()=>{
                    let message=message.ok_or(Error::Safe("Remote MCP connection closed. Reconnect or run Sign in."))?;
                    route.check()?;
                    let mut bytes=serde_json::to_vec(&message)?; bytes.push(b'\n');
                    output.write_all(&bytes).await?; output.flush().await?;
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
            result=auth::sign_in(route.clone(),store.clone(),endpoint,&log)=>result?,
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
                crate::http_route::McpRoute::new(route.clone())?.with_log(log.clone()),
                manager,
            ),
            StreamableHttpClientTransportConfig::with_uri(endpoint.to_owned()),
        );
        let mut transport = transport;
        let message: ClientJsonRpcMessage = serde_json::from_value(
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"upwork-native-sign-in","version":env!("CARGO_PKG_VERSION")}}}),
        )?;
        let verification = tokio::time::timeout(Duration::from_secs(30), async {
            let send = send_owned(&mut transport, message);
            let (sent, response) = tokio::join!(send, transport.receive());
            sent.map_err(|_| Error::Safe("Session saved but MCP initialization failed."))?;
            let response = response.ok_or(Error::Safe("MCP initialization did not return."))?;
            let initialized = serde_json::from_value(
                serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            )?;
            send_owned(&mut transport, initialized)
                .await
                .map_err(|_| Error::Safe("MCP initialization notification failed."))?;
            let v = serde_json::to_value(response)?;
            if v.get("result").and_then(|v| v.get("serverInfo")).is_none() {
                return Err(Error::Safe(
                    "MCP initialization rejected; sign-in not verified.",
                ));
            }
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
    let result = relay(
        client,
        route,
        endpoint,
        tokio::io::stdin(),
        tokio::io::stdout(),
    )
    .await;
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
