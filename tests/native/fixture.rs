use super::child::{Result, require, tool};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::Notify,
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use tokio_rustls::{
    TlsAcceptor,
    rustls::{
        self,
        pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    },
};
pub const ISSUER: &str = "https://mcp.upwork.com";
pub const ENDPOINT: &str = "https://mcp.upwork.com/mcp";

#[derive(Default)]
pub struct Data {
    pub family: String,
    pub case: String,
    pub initialized: usize,
    pub pings: usize,
    pub initializations: usize,
    pub responses: usize,
    pub response_bytes: usize,
    pub gets: usize,
    pub ordinary: usize,
    pub requests: usize,
    pub refreshes: usize,
    pub expected: Value,
    pub valid: bool,
    pub ready_version_ok: bool,
    pub flags: HashMap<String, bool>,
}
#[derive(Clone)]
pub struct State {
    pub data: Arc<Mutex<Data>>,
    changed: Arc<Notify>,
}
impl State {
    pub fn update<T>(&self, f: impl FnOnce(&mut Data) -> T) -> T {
        let value = f(&mut self.data.lock().expect("Fixture state poisoned"));
        self.changed.notify_waiters();
        value
    }
    pub fn flag(&self, key: &str) -> bool {
        self.data
            .lock()
            .expect("Fixture state poisoned")
            .flags
            .get(key)
            .copied()
            .unwrap_or(false)
    }
    pub fn mark(&self, key: &str, value: bool) {
        self.update(|d| {
            d.flags.insert(key.into(), value);
        });
    }
    pub async fn wait(&self, key: &str, budget: Duration) -> Result<()> {
        timeout(budget, async {
            loop {
                let notified = self.changed.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                if self.flag(key) {
                    break;
                }
                notified.await;
            }
        })
        .await
        .map_err(|_| "Fixture barrier expired")?;
        Ok(())
    }
    pub fn set_case(&self, case: &str) {
        self.update(|d| {
            d.case = case.into();
            d.initialized = 0;
            d.initializations = 0;
            d.responses = 0;
            d.flags.remove("ready");
        });
    }
}
pub struct Fixture {
    pub state: State,
    pub port: u16,
    task: JoinHandle<()>,
}
impl Fixture {
    pub async fn start(ca: &Path, family: &str, case: &str) -> Result<Self> {
        let certs = CertificateDer::pem_file_iter(ca.join("leaf.pem"))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let key = PrivateKeyDer::from_pem_file(ca.join("leaf.key"))?;
        let acceptor = TlsAcceptor::from(Arc::new(
            rustls::ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(certs, key)?,
        ));
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let state = State {
            data: Arc::new(Mutex::new(Data {
                family: family.into(),
                case: case.into(),
                valid: true,
                ready_version_ok: true,
                ..Data::default()
            })),
            changed: Arc::new(Notify::new()),
        };
        let shared = state.clone();
        let task = tokio::spawn(async move {
            let mut children = JoinSet::new();
            loop {
                tokio::select! {
                    value=listener.accept()=>{
                        let Ok((stream,_))=value else {break};
                        if children.len()>=64 {break;}
                        let acceptor=acceptor.clone();let state=shared.clone();
                        children.spawn(async move {let _=timeout(Duration::from_secs(20),connection(stream,acceptor,state)).await;});
                    }
                    _=children.join_next(),if !children.is_empty()=>{}
                    _=shared.wait("stop",Duration::from_secs(180))=>break,
                }
            }
            children.abort_all();
            while children.join_next().await.is_some() {}
        });
        Ok(Self { state, port, task })
    }
    pub fn seed(&self, root: &Path) -> Result<()> {
        std::fs::create_dir_all(root.join("config"))?;
        std::fs::write(
            root.join("config/connections.json"),
            serde_json::to_vec(
                &json!({"version":1,"mode":"proxy","selected":"fixture","profiles":[{"name":"fixture","url":format!("http://127.0.0.1:{}",self.port)}]}),
            )?,
        )?;
        Ok(())
    }
    pub async fn close(self) -> Result<()> {
        self.state.mark("stop", true);
        timeout(Duration::from_secs(2), self.task).await??;
        Ok(())
    }
}

pub async fn certificates(root: &Path) -> Result<()> {
    std::fs::create_dir_all(root.join("empty"))?;
    let ca = root.join("ca.pem");
    let ca_key = root.join("ca.key");
    let key = root.join("leaf.key");
    let csr = root.join("leaf.csr");
    let leaf = root.join("leaf.pem");
    let extension = root.join("leaf.ext");
    std::fs::write(
        &extension,
        "basicConstraints=critical,CA:FALSE\nsubjectAltName=DNS:mcp.upwork.com\nextendedKeyUsage=serverAuth\n",
    )?;
    for args in [
        vec![
            OsStr::new("req"),
            OsStr::new("-x509"),
            OsStr::new("-newkey"),
            OsStr::new("rsa:2048"),
            OsStr::new("-nodes"),
            OsStr::new("-sha256"),
            OsStr::new("-days"),
            OsStr::new("2"),
            OsStr::new("-subj"),
            OsStr::new("/CN=Disposable Upwork Audit Fixture"),
            OsStr::new("-addext"),
            OsStr::new("basicConstraints=critical,CA:TRUE"),
            OsStr::new("-keyout"),
            ca_key.as_os_str(),
            OsStr::new("-out"),
            ca.as_os_str(),
        ],
        vec![
            OsStr::new("req"),
            OsStr::new("-new"),
            OsStr::new("-newkey"),
            OsStr::new("rsa:2048"),
            OsStr::new("-nodes"),
            OsStr::new("-subj"),
            OsStr::new("/CN=mcp.upwork.com"),
            OsStr::new("-keyout"),
            key.as_os_str(),
            OsStr::new("-out"),
            csr.as_os_str(),
        ],
        vec![
            OsStr::new("x509"),
            OsStr::new("-req"),
            OsStr::new("-in"),
            csr.as_os_str(),
            OsStr::new("-CA"),
            ca.as_os_str(),
            OsStr::new("-CAkey"),
            ca_key.as_os_str(),
            OsStr::new("-CAcreateserial"),
            OsStr::new("-days"),
            OsStr::new("2"),
            OsStr::new("-sha256"),
            OsStr::new("-extfile"),
            extension.as_os_str(),
            OsStr::new("-out"),
            leaf.as_os_str(),
        ],
    ] {
        tool("openssl", &args, 10).await?;
    }
    use std::os::unix::fs::PermissionsExt;
    for path in [ca_key, key] {
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}
impl Request {
    pub fn json(&self) -> Result<Value> {
        Ok(serde_json::from_slice(&self.body)?)
    }
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.get(key).map(String::as_str)
    }
}
async fn read_request<R: tokio::io::AsyncRead + Unpin>(
    stream: &mut BufReader<R>,
) -> Result<Option<Request>> {
    let mut line = String::new();
    if stream.read_line(&mut line).await? == 0 {
        return Ok(None);
    }
    let mut fields = line.split_whitespace();
    let method = fields.next().ok_or("Missing method")?.into();
    let path = fields.next().ok_or("Missing path")?.into();
    let mut headers = HashMap::new();
    let mut size = line.len();
    loop {
        line.clear();
        require(stream.read_line(&mut line).await? > 0, "Truncated headers")?;
        size += line.len();
        require(size <= 65536, "Oversized headers")?;
        if line == "\r\n" {
            break;
        }
        let (key, value) = line.split_once(':').ok_or("Invalid header")?;
        headers.insert(key.to_ascii_lowercase(), value.trim().into());
    }
    let length: usize = headers
        .get("content-length")
        .map(String::as_str)
        .unwrap_or("0")
        .parse()?;
    require(length <= 1024 * 1024, "Oversized fixture request")?;
    let mut body = vec![0; length];
    stream.read_exact(&mut body).await?;
    Ok(Some(Request {
        method,
        path,
        headers,
        body,
    }))
}
pub async fn response<W: AsyncWrite + Unpin>(
    stream: &mut W,
    status: u16,
    headers: &[(&str, String)],
    body: &[u8],
    length: Option<usize>,
) -> Result<()> {
    let mut head = format!("HTTP/1.1 {status} Fixture\r\n");
    for (key, value) in headers {
        head.push_str(&format!("{key}: {value}\r\n"));
    }
    if let Some(length) = length {
        head.push_str(&format!("Content-Length: {length}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    Ok(())
}
pub async fn json_reply<W: AsyncWrite + Unpin>(
    stream: &mut W,
    value: Value,
    status: u16,
) -> Result<()> {
    let body = if value.is_null() {
        Vec::new()
    } else {
        serde_json::to_vec(&value)?
    };
    response(
        stream,
        status,
        &[("Content-Type", "application/json".into())],
        &body,
        Some(body.len()),
    )
    .await
}
async fn connection(mut stream: TcpStream, acceptor: TlsAcceptor, state: State) -> Result<()> {
    // Read exactly through CONNECT headers, never consume TLS bytes into a discarded buffer.
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        let mut byte = [0];
        stream.read_exact(&mut byte).await?;
        head.push(byte[0]);
        require(head.len() <= 65536, "CONNECT header bound")?;
    }
    if !head.starts_with(b"CONNECT mcp.upwork.com:443 HTTP/1.") {
        stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\n\r\n")
            .await?;
        return Ok(());
    }
    stream
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .await?;
    let tls = timeout(Duration::from_secs(3), acceptor.accept(stream)).await??;
    let mut stream = BufReader::new(tls);
    while let Some(request) = read_request(&mut stream).await? {
        state.update(|d| d.requests += 1);
        if dispatch(stream.get_mut(), request, &state).await? {
            break;
        }
    }
    let _ = timeout(Duration::from_secs(1), stream.get_mut().shutdown()).await;
    Ok(())
}
async fn dispatch<W: AsyncWrite + Unpin>(
    stream: &mut W,
    request: Request,
    state: &State,
) -> Result<bool> {
    let path = request.path.split('?').next().unwrap_or("");
    if request.method == "GET" {
        if path.contains("oauth-protected-resource") {
            json_reply(
                stream,
                json!({"resource":ENDPOINT,"authorization_servers":[ISSUER]}),
                200,
            )
            .await?;
        } else if path.contains("oauth-authorization-server")
            || path.contains("openid-configuration")
        {
            json_reply(stream,json!({"issuer":ISSUER,"authorization_endpoint":format!("{ISSUER}/authorize"),"token_endpoint":format!("{ISSUER}/token"),"registration_endpoint":format!("{ISSUER}/register"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"]}),200).await?;
        } else if path == "/mcp"
            && request.header("mcp-session-id") == Some("ordinary-fixture-session")
        {
            return super::ordinary::get(stream, state).await;
        } else {
            json_reply(stream, Value::Null, 405).await?;
        }
        return Ok(false);
    }
    if request.method == "DELETE" {
        json_reply(stream, Value::Null, 204).await?;
        return Ok(false);
    }
    require(request.method == "POST", "Unexpected fixture method")?;
    if path == "/register" {
        let value = request.json()?;
        json_reply(stream,json!({"client_id":"fixture-client","redirect_uris":value["redirect_uris"],"token_endpoint_auth_method":"none"}),201).await?;
        return Ok(false);
    }
    if path == "/token" {
        return super::coordination::token(stream, &request, state).await;
    }
    require(path == "/mcp", "Unexpected fixture path")?;
    super::initialization::respond(stream, &request, state).await
}
pub fn temporary(parent: &Path, prefix: &str) -> Result<tempfile::TempDir> {
    Ok(tempfile::Builder::new().prefix(prefix).tempdir_in(parent)?)
}
pub fn root_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
