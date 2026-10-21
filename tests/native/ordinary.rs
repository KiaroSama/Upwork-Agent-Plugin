use super::{
    child::{INITIALIZE, NativeChild, Result, execute, logs, require},
    fixture::{self, Fixture, State, json_reply, response},
    oracle,
};
use serde_json::{Value, json};
use std::{path::Path, time::Duration};
use tokio::{
    io::{AsyncWrite, AsyncWriteExt},
    time::Instant,
};

pub async fn get<W: AsyncWrite + Unpin>(stream: &mut W, state: &State) -> Result<bool> {
    state.update(|d| d.gets += 1);
    response(
        stream,
        200,
        &[
            ("Content-Type", "Text/Event-Stream; charset=utf-8".into()),
            ("Connection", "close".into()),
        ],
        b"data: {\"jsonrpc\":\"2.0\",\"id\":\"ordinary-get-ping\",\"method\":\"ping\"}\n\n",
        None,
    )
    .await?;
    let _ = state.wait("stop", Duration::from_secs(10)).await;
    Ok(true)
}
pub async fn post<W: AsyncWrite + Unpin>(
    stream: &mut W,
    message: &Value,
    state: &State,
) -> Result<bool> {
    match message.get("method").and_then(Value::as_str) {
        Some("initialize") => {
            state.update(|d| d.initializations += 1);
            let body = serde_json::to_vec(
                &json!({"jsonrpc":"2.0","id":message["id"],"result":{"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"ordinary-fixture","version":"1"}}}),
            )?;
            response(
                stream,
                200,
                &[
                    ("Content-Type", "application/json".into()),
                    ("Mcp-Session-Id", "ordinary-fixture-session".into()),
                ],
                &body,
                Some(body.len()),
            )
            .await?;
            Ok(false)
        }
        Some("notifications/initialized") => {
            state.update(|d| d.initialized += 1);
            json_reply(stream, Value::Null, 202).await?;
            Ok(false)
        }
        None => {
            if message["id"] == "ordinary-get-ping" && message["result"] == json!({}) {
                state.mark("control", true);
            }
            json_reply(stream, Value::Null, 202).await?;
            Ok(false)
        }
        Some("tools/list") => {
            state.update(|d| d.ordinary += 1);
            let case = state.update(|d| d.case.clone());
            let mut body = serde_json::to_vec(
                &json!({"jsonrpc":"2.0","id":message["id"],"result":{"tools":[]}}),
            )?;
            if case == "large_json" {
                body.resize(2 * 1024 * 1024, b' ');
            }
            if case == "chunked_overflow" {
                body.resize(16 * 1024 * 1024 + 1, b' ');
            }
            let mut headers = vec![
                (
                    "Content-Type",
                    if case == "wrong_media" {
                        "application/json-extra"
                    } else {
                        "Application/JSON"
                    }
                    .into(),
                ),
                ("Connection", "close".into()),
            ];
            if case == "chunked_overflow" {
                headers.push(("Transfer-Encoding", "chunked".into()));
                response(stream, 200, &headers, &[], None).await?;
                for part in body.chunks(65536) {
                    stream
                        .write_all(format!("{:x}\r\n", part.len()).as_bytes())
                        .await?;
                    stream.write_all(part).await?;
                    stream.write_all(b"\r\n").await?;
                }
                stream.write_all(b"0\r\n\r\n").await?;
                stream.flush().await?;
            } else {
                response(
                    stream,
                    200,
                    &headers,
                    &body,
                    Some(body.len() + if case == "truncated" { 100 } else { 0 }),
                )
                .await?;
            }
            Ok(true)
        }
        _ => Err("Unexpected ordinary method".into()),
    }
}
fn trailing(line: &[u8]) -> Result<()> {
    let message: Value = serde_json::from_slice(line)?;
    require(
        message.is_object() && message["jsonrpc"] == "2.0",
        "Invalid trailing envelope",
    )?;
    if message["method"] == "ping" {
        require(
            message["id"] == "ordinary-get-ping"
                && message.get("result").is_none()
                && message.get("error").is_none(),
            "Invalid trailing ping",
        )
    } else {
        require(
            oracle::kind(&message) == "notification",
            "Unexpected or duplicate terminal after exit",
        )
    }
}
pub async fn run(binary: &Path, output: &Path) -> Result<Value> {
    let ca = fixture::temporary(output, "ordinary-certs-")?;
    fixture::certificates(ca.path()).await?;
    let mut rows = Vec::new();
    for case in [
        "json",
        "large_json",
        "wrong_media",
        "chunked_overflow",
        "truncated",
    ] {
        let root = fixture::temporary(output, "ordinary-http-")?;
        let root = root.path();
        let server = Fixture::start(ca.path(), "ordinary", "prepare").await?;
        server.seed(root)?;
        let mut child = None;
        let result=async{
            let prep=execute(binary,root,ca.path(),"sign-in",None).await?;require(prep["returncode"]==0&&prep["completion_claimed"]==true,"Ordinary credential preparation failed")?;
            server.state.set_case(case);
            child=Some(NativeChild::spawn(binary,root,Some(ca.path()),&["bridge"])?);let child=child.as_mut().unwrap();child.bytes(&format!("{INITIALIZE}\n")).await?;
            let end=Instant::now()+Duration::from_secs(8);let mut initialized=false;let mut terminal=false;
            while let Some(line)=child.line(end).await{
                let message:Value=serde_json::from_slice(&line)?;require(message.is_object(),"Ordinary protocol message is not object")?;
                if message["method"]=="ping"{
                    require(message["jsonrpc"]=="2.0"&&message["id"]=="ordinary-get-ping"&&message.get("result").is_none()&&message.get("error").is_none(),"Invalid listening GET ping")?;
                    child.send(&json!({"jsonrpc":"2.0","id":"ordinary-get-ping","result":{}})).await?;
                }else if !initialized{
                    require(oracle::terminal(&message)==Some("result"),"Ordinary initialization failed")?;initialized=true;
                    child.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await?;
                    child.send(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).await?;
                }else if message["id"].as_i64()==Some(2){
                    require(!terminal&&message["jsonrpc"]=="2.0"&&message.get("error").is_none()&&message.get("method").is_none()&&message["result"]==json!({"tools":[]}),"Invalid or duplicate ordinary response")?;terminal=true;
                }else{require(oracle::kind(&message)=="notification","Unexpected ordinary message")?;}
                if terminal && message["method"]=="ping" { server.state.wait("control",end.saturating_duration_since(Instant::now())).await?; }
                if terminal && server.state.flag("control") {break;}
            }
            let positive=matches!(case,"json"|"large_json");
            if positive{server.state.wait("control",Duration::from_secs(1)).await?;require(terminal&&server.state.update(|d|d.gets>=1),"Ordinary result or listening GET control missing")?;}
            else{require(!terminal,"Rejected body produced successful terminal")?;}
            let processed=child.records.len();let (exit,timed)=child.finish(3.0,positive).await?;
            require(!timed&&exit==if positive{0}else{1},"Ordinary exit contract failed")?;
            if !positive{
                let expected=match case{"truncated"=>"class=response_decode","wrong_media"=>"class=unexpected_content_type",_=>"class=unexpected_http_response"};
                require(logs(root)?.contains(expected),"Ordinary rejection lacks causal diagnostic")?;
            }
            require(!child.reader_invalid,"Ordinary reader failed")?;
            for line in &child.records[processed..]{trailing(line)?;}
            let errors=std::str::from_utf8(&child.errors)?;require(!errors.contains("fixture-session")&&!errors.contains("disposable-fixture-token"),"Private fixture value leaked")?;
            let d=server.state.data.lock().unwrap();require(initialized&&d.initializations==1&&d.ordinary==1&&d.initialized==1,"Ordinary request count drift")?;
            Ok::<_,Box<dyn std::error::Error+Send+Sync>>(json!({"case":case,"passed":true,"ordinary_posts":d.ordinary,"initializations":d.initializations,"listening_gets":d.gets,"get_control_replied":d.flags.get("control").copied().unwrap_or(false),"exit":exit,"reaped":true}))
        }.await;
        if let Some(mut child) = child {
            child.close().await?;
        }
        server.close().await?;
        rows.push(result?);
    }
    Ok(json!({"cases":5,"passed":5,"results":rows}))
}
