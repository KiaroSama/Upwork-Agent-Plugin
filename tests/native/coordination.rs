use super::{
    child::{NativeChild, Result, execute, invoke, require},
    fixture::{self, Fixture, Request, State, json_reply},
};
use serde_json::{Value, json};
use std::{fs, path::Path, time::Duration};
use tokio::{
    io::AsyncWrite,
    net::TcpListener,
    time::{Instant, timeout},
};

pub async fn token<W: AsyncWrite + Unpin>(
    stream: &mut W,
    request: &Request,
    state: &State,
) -> Result<bool> {
    let family = state.update(|d| d.family.clone());
    if family != "coordination" {
        json_reply(stream,json!({"access_token":"disposable-fixture-token","token_type":"Bearer","expires_in":3600,"refresh_token":"disposable-fixture-refresh"}),200).await?;
        return Ok(false);
    }
    require(
        !request.body.is_empty() && request.body.len() <= 8192,
        "Invalid token request size",
    )?;
    let fields: std::collections::HashMap<_, _> = url::form_urlencoded::parse(&request.body)
        .into_owned()
        .collect();
    let hold = state.update(|d| d.case == "refresh_during_manual");
    match fields.get("grant_type").map(String::as_str) {
        Some("refresh_token") => {
            let index = state.update(|d| {
                d.refreshes += 1;
                d.refreshes
            });
            if hold {
                state.mark("refresh_started", true);
                state.update(|d| {
                    d.valid &= fields.get("refresh_token").map(String::as_str)
                        == Some("synthetic-manual-refresh")
                });
            } else if index == 1 {
                state.mark("refresh_started", true);
                state.wait("release", Duration::from_secs(12)).await?;
            } else {
                state.update(|d| {
                    d.valid &= fields.get("refresh_token").map(String::as_str)
                        == Some("synthetic-refresh-1")
                });
            }
            json_reply(stream,json!({"access_token":format!("synthetic-refreshed-{index}"),"token_type":"Bearer","expires_in":3600,"refresh_token":format!("synthetic-refresh-{index}")}),200).await?;
        }
        Some("authorization_code") => {
            state.mark("manual_exchange", true);
            if hold {
                state.wait("release", Duration::from_secs(12)).await?;
            }
            json_reply(stream,json!({"access_token":"synthetic-manual-session","token_type":"Bearer","expires_in":3600,"refresh_token":"synthetic-manual-refresh"}),200).await?;
        }
        _ => json_reply(stream, Value::Null, 400).await?,
    }
    Ok(false)
}
async fn initialize(child: &mut NativeChild, id: i64) -> Result<()> {
    child.send(&json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"coordination-fixture","version":"1"}}})).await
}
async fn authorization(
    child: &mut NativeChild,
    port: u16,
) -> Result<tokio::task::JoinHandle<Result<bool>>> {
    let end = Instant::now() + Duration::from_secs(8);
    while let Some(line) = child.line(end).await {
        let text = std::str::from_utf8(&line)?;
        if !text.starts_with("https://") {
            continue;
        }
        let link = url::Url::parse(text.trim())?;
        let fields: std::collections::HashMap<_, _> = link.query_pairs().into_owned().collect();
        let mut redirect = url::Url::parse(fields.get("redirect_uri").ok_or("Missing redirect")?)?;
        require(
            redirect.host_str() == Some("127.0.0.1") && redirect.port() == Some(port),
            "Unexpected callback destination",
        )?;
        redirect
            .query_pairs_mut()
            .append_pair("code", "synthetic-code")
            .append_pair("state", fields.get("state").ok_or("Missing state")?)
            .append_pair("iss", fixture::ISSUER);
        return Ok(tokio::spawn(async move {
            let client = reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(8))
                .build()?;
            let response = client.get(redirect).send().await?;
            let accepted = response.status().as_u16() == 200;
            let _ = response.bytes().await?;
            Ok(accepted)
        }));
    }
    Err("No synthetic authorization link".into())
}
pub async fn run(binary: &Path, output: &Path) -> Result<Value> {
    let ca = fixture::temporary(output, "credential-certs-")?;
    fixture::certificates(ca.path()).await?;
    let mut rows = Vec::new();
    for mode in [
        "manual_during_refresh",
        "concurrent_refresh",
        "refresh_during_manual",
    ] {
        let root = fixture::temporary(output, "credential-race-")?;
        let root = root.path();
        let reserved = TcpListener::bind("127.0.0.1:0").await?;
        let port = reserved.local_addr()?.port();
        drop(reserved);
        let server = Fixture::start(ca.path(), "coordination", mode).await?;
        server.seed(root)?;
        fs::create_dir_all(root.join(".state/rust"))?;
        for (name, value) in [
            (
                "client.json",
                json!({"client_id":"fixture-client","redirect_uris":[format!("http://127.0.0.1:{port}/callback")]}),
            ),
            (
                "credentials.json",
                json!({"client_id":"fixture-client","token_response":{"access_token":"synthetic-old-session","token_type":"Bearer","expires_in":1,"refresh_token":"synthetic-refresh-0"},"token_received_at":1,"granted_scopes":[],"issuer":fixture::ISSUER}),
            ),
        ] {
            let path = root.join(".state/rust").join(name);
            fs::write(&path, serde_json::to_vec(&value)?)?;
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        let hold = mode == "refresh_during_manual";
        let mut children = Vec::new();
        let mut callback = None;
        let result: Result<Value> = async {
            children.push(NativeChild::spawn(binary,root,Some(ca.path()),&[if hold{"sign-in"}else{"bridge"}])?);
            if !hold{initialize(&mut children[0],41).await?;server.state.wait("refresh_started",Duration::from_secs(8)).await?;}
            if mode=="concurrent_refresh"{
                children.push(NativeChild::spawn(binary,root,Some(ca.path()),&["bridge"])?);initialize(&mut children[1],41).await?;
                let end=Instant::now()+Duration::from_millis(500);
                while Instant::now()<end&&children[1].process.try_wait()?.is_none(){tokio::time::sleep(Duration::from_millis(10)).await;}
                let early=children[1].process.try_wait()?.is_some();
                server.state.mark("release",true);
                let first=children[0].matching(41,8).await?;let second=children[1].matching(41,8).await?;
                let d=server.state.data.lock().unwrap();
                Ok(json!({"case":mode,"passed":!early&&first&&second&&d.valid&&d.refreshes==2,"refresh_requests":d.refreshes,"rotated_token_reloaded":d.valid,"first_bridge_initialized":first,"second_bridge_initialized":second,"second_exited_while_first_refresh_active":early}))
            }else{
                let manual=if hold{0}else{children.push(NativeChild::spawn(binary,root,Some(ca.path()),&["sign-in"])?);1};
                callback=Some(authorization(&mut children[manual],port).await?);
                let early=if hold{
                    server.state.wait("manual_exchange",Duration::from_secs(8)).await?;
                    children.push(NativeChild::spawn(binary,root,Some(ca.path()),&["bridge"])?);initialize(&mut children[1],41).await?;
                    server.state.wait("refresh_started",Duration::from_millis(500)).await.is_ok()
                }else{server.state.wait("manual_exchange",Duration::from_millis(500)).await.is_ok()};
                server.state.mark("release",true);
                let callback_ok=timeout(Duration::from_secs(9),callback.as_mut().ok_or("Missing callback task")?).await???;callback.take();
                let (exit,timed)=children[manual].finish(8.0,false).await?;
                let completion=children[manual].records.iter().any(|v|String::from_utf8_lossy(v).contains("Sign-in complete."));
                let bridge=usize::from(hold);let initialized=children[bridge].matching(41,8).await?;
                let saved:Value=serde_json::from_slice(&fs::read(root.join(".state/rust/credentials.json"))?)?;
                let d=server.state.data.lock().unwrap();let expected=if hold&&d.refreshes>0{"synthetic-refreshed-1"}else{"synthetic-manual-session"};let preserved=saved["token_response"]["access_token"]==expected;
                Ok(json!({"case":mode,"passed":callback_ok&&exit==0&&!timed&&completion&&initialized&&!early&&preserved&&(!hold||(d.valid&&d.refreshes<=1)),"callback_ok":callback_ok,"manual_exit_success":exit==0,"completion_claimed":completion,"first_bridge_initialized":initialized,"exchange_before_release":early,"manual_session_preserved":preserved,"rotated_token_reloaded":d.valid,"refresh_requests":d.refreshes}))
            }
        }.await;
        server.state.mark("release", true);
        for child in children.iter_mut().rev() {
            child.close().await?;
        }
        if let Some(task) = callback {
            task.abort();
            let _ = task.await;
        }
        server.close().await?;
        rows.push(result?);
    }
    Ok(json!({"cases":3,"passed":rows.iter().filter(|v|v["passed"]==true).count(),"results":rows}))
}

async fn signal(child: &NativeChild, name: &str) -> Result<()> {
    let pid = child.process.id().ok_or("Owned child exited")?.to_string();
    super::child::tool(
        "kill",
        &[std::ffi::OsStr::new(name), std::ffi::OsStr::new(&pid)],
        2,
    )
    .await?;
    Ok(())
}
async fn paused(child: &mut NativeChild) -> Result<()> {
    signal(child, "-STOP").await?;
    let pid = child.process.id().ok_or("Owned process missing")?;
    let end = Instant::now() + Duration::from_secs(3);
    loop {
        require(
            child.process.try_wait()?.is_none(),
            "Bridge exited before stop barrier",
        )?;
        let status = fs::read_to_string(format!("/proc/{pid}/status"))?;
        if status
            .lines()
            .any(|line| line.starts_with("State:") && line.contains("T (stopped)"))
        {
            return Ok(());
        }
        require(Instant::now() < end, "Owned process did not stop")?;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}
pub async fn route(binary: &Path, output: &Path) -> Result<Value> {
    let ca = fixture::temporary(output, "route-certs-")?;
    fixture::certificates(ca.path()).await?;
    let mut rows = Vec::new();
    for kind in ["block_then_restore", "inactive_profile_edit"] {
        let root = fixture::temporary(output, "route-revision-")?;
        let root = root.path();
        let server = Fixture::start(ca.path(), "route", "valid").await?;
        server.seed(root)?;
        let mut child = None;
        let mut stopped = false;
        let result=async{
            let prep=execute(binary,root,ca.path(),"sign-in",None).await?;require(prep["returncode"]==0&&prep["completion_claimed"]==true,"Route fixture authorization failed")?;
            server.state.mark("ready",false);
            child=Some(NativeChild::spawn(binary,root,Some(ca.path()),&["bridge"])?);let child=child.as_mut().unwrap();initialize(child,71).await?;require(child.matching(71,5).await?,"Route bridge initialization failed")?;
            child.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await?;server.state.wait("ready",Duration::from_secs(3)).await?;
            paused(child).await?;stopped=true;
            if kind=="block_then_restore"{for command in [vec!["block"],vec!["select","fixture"]]{let r=invoke(binary,root,&command,b"",5.0).await?;require(r.0==0&&!r.3,"Route command failed")?;}}
            else{let r=invoke(binary,root,&["add"],&serde_json::to_vec(&json!({"name":"inactive","url":"http://127.0.0.1:1"}))?,5.0).await?;require(r.0==0&&!r.3,"Inactive edit failed")?;}
            child.send(&json!({"jsonrpc":"2.0","id":73,"method":"ping"})).await?;signal(child,"-CONT").await?;stopped=false;
            let answered=child.matching(73,3).await?;
            let status=if kind=="block_then_restore"{timeout(Duration::from_secs(3),child.process.wait()).await.ok().transpose()?}else{child.process.try_wait()?};
            let pings=server.state.update(|d|d.ordinary);let exited=status.is_some();let failed=status.is_some_and(|s|!s.success());
            let passed=if kind=="block_then_restore"{exited&&failed&&!answered&&pings==0}else{!exited&&answered&&pings==1};
            Ok::<_,Box<dyn std::error::Error+Send+Sync>>(json!({"case":kind,"passed":passed,"stop_barrier_observed":true,"queued_ping_answered":answered,"outbound_ping_requests":pings,"bridge_exited":exited,"exit_nonzero":failed}))
        }.await;
        if let Some(mut child) = child {
            if stopped && child.process.try_wait()?.is_none() {
                signal(&child, "-CONT").await?;
            }
            child.close().await?;
        }
        server.close().await?;
        rows.push(result?);
    }
    Ok(json!({"cases":2,"passed":rows.iter().filter(|v|v["passed"]==true).count(),"results":rows}))
}
