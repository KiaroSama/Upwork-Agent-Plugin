use serde_json::{Value, json};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, Command},
    sync::mpsc,
    task::JoinHandle,
    time::{Instant, timeout, timeout_at},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub fn require(condition: bool, message: &'static str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(std::io::Error::other(message).into())
    }
}
pub const INITIALIZE: &str = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"isolated-audit","version":"1"}}}"#;

pub struct NativeChild {
    pub process: Child,
    lines: mpsc::UnboundedReceiver<Vec<u8>>,
    readers: Vec<JoinHandle<std::io::Result<Vec<u8>>>>,
    pub records: Vec<Vec<u8>>,
    pub errors: Vec<u8>,
    pub reader_invalid: bool,
}
fn reader(
    stream: impl AsyncRead + Unpin + Send + 'static,
    sender: Option<mpsc::UnboundedSender<Vec<u8>>>,
) -> JoinHandle<std::io::Result<Vec<u8>>> {
    tokio::spawn(async move {
        let mut stream = BufReader::new(stream);
        let mut all = Vec::new();
        loop {
            let mut line = Vec::new();
            let count = stream.read_until(b'\n', &mut line).await?;
            if count == 0 {
                break;
            }
            if all.len() + count > 32 * 1024 * 1024 {
                return Err(std::io::Error::other("Fixture output exceeded bound"));
            }
            std::str::from_utf8(&line)
                .map_err(|_| std::io::Error::other("Invalid UTF-8 fixture output"))?;
            all.extend_from_slice(&line);
            if let Some(sender) = &sender {
                let _ = sender.send(line);
            }
        }
        Ok(all)
    })
}
impl NativeChild {
    pub fn spawn(binary: &Path, root: &Path, ca: Option<&Path>, args: &[&str]) -> Result<Self> {
        let mut command = Command::new(binary);
        command.arg("--root").arg(root).args(args);
        for key in ["TEMP", "TMP", "TMPDIR"] {
            command.env(key, root);
        }
        if let Some(ca) = ca {
            command
                .env("SSL_CERT_FILE", ca.join("ca.pem"))
                .env("SSL_CERT_DIR", ca.join("empty"));
        }
        Self::spawn_command(command)
    }
    pub fn spawn_command(mut command: Command) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut process = command.spawn()?;
        let (tx, lines) = mpsc::unbounded_channel();
        let stdout = reader(process.stdout.take().ok_or("Missing stdout")?, Some(tx));
        let stderr = reader(process.stderr.take().ok_or("Missing stderr")?, None);
        Ok(Self {
            process,
            lines,
            readers: vec![stdout, stderr],
            records: Vec::new(),
            errors: Vec::new(),
            reader_invalid: false,
        })
    }
    pub async fn send(&mut self, value: &Value) -> Result<()> {
        self.bytes(&(serde_json::to_string(value)? + "\n")).await
    }
    pub async fn bytes(&mut self, text: &str) -> Result<()> {
        timeout(Duration::from_secs(2), async {
            let stdin = self
                .process
                .stdin
                .as_mut()
                .ok_or_else(|| std::io::Error::other("Closed fixture input"))?;
            stdin.write_all(text.as_bytes()).await?;
            stdin.flush().await
        })
        .await??;
        Ok(())
    }
    pub async fn line(&mut self, deadline: Instant) -> Option<Vec<u8>> {
        let line = timeout_at(deadline, self.lines.recv())
            .await
            .ok()
            .flatten()?;
        self.records.push(line.clone());
        Some(line)
    }
    pub async fn matching(&mut self, id: i64, seconds: u64) -> Result<bool> {
        let end = Instant::now() + Duration::from_secs(seconds);
        while let Some(line) = self.line(end).await {
            if let Ok(value) = serde_json::from_slice::<Value>(&line)
                && value["id"].as_i64() == Some(id)
                && value.get("result").is_some()
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
    pub async fn finish(&mut self, seconds: f64, close_input: bool) -> Result<(i32, bool)> {
        if close_input {
            self.process.stdin.take();
        }
        let timed = timeout(Duration::from_secs_f64(seconds), self.process.wait()).await;
        let timed_out = timed.is_err();
        let status = match timed {
            Ok(value) => value?,
            Err(_) => {
                self.process.start_kill()?;
                timeout(Duration::from_secs(3), self.process.wait()).await??
            }
        };
        self.drain().await?;
        use std::os::unix::process::ExitStatusExt;
        Ok((
            status
                .code()
                .unwrap_or_else(|| -status.signal().unwrap_or(1)),
            timed_out,
        ))
    }
    async fn drain(&mut self) -> Result<()> {
        let mut failed = false;
        for (index, mut reader) in self.readers.drain(..).enumerate() {
            match timeout(Duration::from_secs(1), &mut reader).await {
                Ok(Ok(Ok(bytes))) => {
                    if index == 1 {
                        self.errors = bytes;
                    }
                }
                Ok(_) => self.reader_invalid = true,
                Err(_) => {
                    reader.abort();
                    let _ = reader.await;
                    failed = true;
                }
            }
        }
        while let Ok(line) = self.lines.try_recv() {
            self.records.push(line);
        }
        require(!failed, "Fixture reader did not drain")
    }
    pub async fn close(&mut self) -> Result<()> {
        self.process.stdin.take();
        if self.process.try_wait()?.is_none() {
            self.process.start_kill()?;
        }
        timeout(Duration::from_secs(3), self.process.wait()).await??;
        self.drain().await
    }
}

pub async fn callback(line: &[u8]) -> Result<bool> {
    let text = std::str::from_utf8(line)?.trim();
    if !text.starts_with("https://") {
        return Ok(false);
    }
    let link = url::Url::parse(text)?;
    let fields: std::collections::HashMap<_, _> = link.query_pairs().into_owned().collect();
    let mut redirect = url::Url::parse(fields.get("redirect_uri").ok_or("Missing callback")?)?;
    require(
        matches!(redirect.host_str(), Some("127.0.0.1" | "localhost")),
        "Non-loopback fixture callback",
    )?;
    redirect
        .query_pairs_mut()
        .append_pair("code", "fixture-code")
        .append_pair("state", fields.get("state").ok_or("Missing state")?)
        .append_pair("iss", super::fixture::ISSUER);
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(3))
        .build()?;
    let response = client.get(redirect).send().await?;
    let _ = response.bytes().await?;
    Ok(true)
}

pub async fn invoke(
    binary: &Path,
    root: &Path,
    args: &[&str],
    data: &[u8],
    seconds: f64,
) -> Result<(i32, Vec<u8>, Vec<u8>, bool)> {
    let mut child = NativeChild::spawn(binary, root, None, args)?;
    let result = async {
        if !data.is_empty() {
            timeout(
                Duration::from_secs(2),
                child
                    .process
                    .stdin
                    .as_mut()
                    .ok_or("Missing input")?
                    .write_all(data),
            )
            .await??;
        }
        let (exit, timed) = child.finish(seconds, true).await?;
        Ok((exit, child.records.concat(), child.errors.clone(), timed))
    }
    .await;
    child.close().await?;
    result
}

pub async fn import_measured(
    binary: &Path,
    root: &Path,
    source: &Path,
) -> Result<(i32, Vec<u8>, bool, f64)> {
    let timing = root.join("child-cpu.txt");
    let mut command = Command::new("/usr/bin/time");
    command
        .args(["-f", "%U %S", "-o"])
        .arg(&timing)
        .arg("--")
        .arg(binary)
        .arg("--root")
        .arg(root)
        .arg("import")
        .arg(source);
    for key in ["TEMP", "TMP", "TMPDIR"] {
        command.env(key, root);
    }
    let mut child = NativeChild::spawn_command(command)?;
    let result = child.finish(60.0, true).await;
    child.close().await?;
    let (exit, timed) = result?;
    let cpu = std::fs::read_to_string(timing)?
        .split_whitespace()
        .map(str::parse::<f64>)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    require(cpu.len() == 2, "Invalid child CPU measurement")?;
    Ok((exit, child.records.concat(), timed, cpu.iter().sum()))
}

pub async fn tool(command: &str, args: &[&std::ffi::OsStr], seconds: u64) -> Result<Vec<u8>> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdout = child.stdout.take().ok_or("Missing tool output")?;
    let read = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await?;
        Ok::<_, std::io::Error>(bytes)
    });
    let status = match timeout(Duration::from_secs(seconds), child.wait()).await {
        Ok(status) => status?,
        Err(_) => {
            child.start_kill()?;
            timeout(Duration::from_secs(3), child.wait()).await??;
            read.abort();
            let _ = read.await;
            return Err("Fixture tool deadline exceeded".into());
        }
    };
    let mut read = read;
    let bytes = match timeout(Duration::from_secs(1), &mut read).await {
        Ok(value) => value??,
        Err(_) => {
            read.abort();
            let _ = read.await;
            return Err("Fixture tool reader deadline exceeded".into());
        }
    };
    require(status.success(), "Fixture tool failed")?;
    Ok(bytes)
}

pub async fn execute(
    binary: &Path,
    root: &Path,
    ca: &Path,
    mode: &str,
    ready: Option<&super::fixture::State>,
) -> Result<Value> {
    let before = logs(root)?;
    let mut child = NativeChild::spawn(binary, root, Some(ca), &[mode])?;
    let result=async {
        let started=Instant::now(); let end=started+Duration::from_secs(4);
        let mut observed=super::oracle::Observation::default(); let mut callback_sent=false;
        if mode=="bridge" { child.bytes(&format!("{INITIALIZE}\n")).await?; }
        while let Some(line)=child.line(end).await {
            if mode=="sign-in" { if callback(&line).await? { callback_sent=true; } }
            else {
                observed.line(&line);
                if observed.first.is_some() {
                    if observed.first==Some("result") && let Some(state)=ready {
                        child.send(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).await?;
                        state.wait("ready",end.saturating_duration_since(Instant::now())).await?;
                    }
                    break;
                }
            }
        }
        let processed=child.records.len();
        let (code,timed)=child.finish(if mode=="bridge" {3.0}else{1.0},mode=="bridge").await?;
        if mode=="bridge" { for line in &child.records[processed..] { observed.line(line); } }
        let diagnostic=logs(root)?;
        let new_logs=diagnostic.strip_prefix(before.as_str()).unwrap_or(&diagnostic);
        Ok(json!({"returncode":code,"callback_sent":callback_sent,
            "protocol_invalid":observed.invalid||child.reader_invalid,
            "transport_rejected":["class=unexpected_http_response","class=unexpected_content_type"].iter().any(|class|new_logs.contains(class)),
            "first_terminal_message":observed.first,"completion_claimed":child.records.iter().any(|line|line.windows(17).any(|v|v==b"Sign-in complete.")),
            "controller_timeout":timed,"elapsed_seconds":started.elapsed().as_secs_f64(),
            "private_marker_in_stderr":String::from_utf8_lossy(&child.errors).contains("private-fixture-marker")}))
    }.await;
    child.close().await?;
    result
}
pub fn logs(root: &Path) -> Result<String> {
    let mut result = String::new();
    if root.join("logs").exists() {
        let mut paths = std::fs::read_dir(root.join("logs"))?
            .map(|e| e.map(|e| e.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            if path.extension().is_some_and(|v| v == "log") {
                result.push_str(&std::fs::read_to_string(path)?);
            }
        }
    }
    Ok(result)
}
