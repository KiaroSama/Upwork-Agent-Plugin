use axum::{
    Json, Router,
    extract::{Form, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::{
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};
use tokio_rustls::{TlsAcceptor, rustls};

#[derive(Clone, Default)]
pub struct Observed {
    pub targets: Arc<AtomicUsize>,
    pub tokens: Arc<AtomicUsize>,
    pub domains: Arc<Mutex<Vec<String>>>,
    pub auth: Arc<Mutex<Vec<String>>>,
    pub refusal: Arc<AtomicBool>,
}
pub struct Fixture {
    pub target: SocketAddr,
    pub secure_target: SocketAddr,
    pub http: SocketAddr,
    pub https: SocketAddr,
    pub socks: SocketAddr,
    pub observed: Observed,
    tasks: Vec<JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}
async fn mcp(State(o): State<Observed>, Json(v): Json<Value>) -> axum::response::Response {
    o.targets.fetch_add(1, Ordering::SeqCst);
    let Some(id) = v.get("id") else {
        return StatusCode::ACCEPTED.into_response();
    };
    let result = match v["method"].as_str().unwrap_or("") {
        "initialize" => {
            json!({"protocolVersion":"2025-11-25","capabilities":{"tools":{}},"serverInfo":{"name":"local-fixture","version":"1.0.0"}})
        }
        "tools/list" => json!({"tools":[]}),
        _ => json!({}),
    };
    Json(json!({"jsonrpc":"2.0","id":id,"result":result})).into_response()
}
async fn target(State(o): State<Observed>) -> &'static str {
    o.targets.fetch_add(1, Ordering::SeqCst);
    "routed"
}
async fn token(
    State(o): State<Observed>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<Value> {
    assert!(matches!(
        form.get("grant_type").map(String::as_str),
        Some("authorization_code" | "refresh_token")
    ));
    o.tokens.fetch_add(1, Ordering::SeqCst);
    Json(
        json!({"access_token":"fixture-access","token_type":"Bearer","expires_in":3600,"refresh_token":"fixture-refresh"}),
    )
}
pub async fn fixture() -> Fixture {
    let o = Observed::default();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_addr = listener.local_addr().unwrap();
    let metadata = json!({"issuer":format!("http://{target_addr}"),"authorization_endpoint":format!("http://{target_addr}/authorize"),"token_endpoint":format!("http://{target_addr}/token"),"registration_endpoint":format!("http://{target_addr}/register"),"response_types_supported":["code"],"grant_types_supported":["authorization_code","refresh_token"],"code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"]});
    let router=Router::new().route("/",get(target)).route("/done",get(target)).route("/redirect",get(||async{(StatusCode::FOUND,[("location","/done")])})).route("/mcp",post(mcp).get(||async{StatusCode::METHOD_NOT_ALLOWED})).route("/token",post(token)).route("/.well-known/oauth-authorization-server",get(move||{let m=metadata.clone();async move{Json(m)}})).route("/register",post(||async{Json(json!({"client_id":"fixture-client","redirect_uris":["http://localhost:1/callback"],"token_endpoint_auth_method":"none"}))})).with_state(o.clone());
    let target_task = tokio::spawn(async move {
        let _ = axum::serve(listener, router).await;
    });
    let (http, http_task) = proxy(o.clone(), target_addr, false).await;
    let (https, https_task) = proxy(o.clone(), target_addr, true).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let socks = listener.local_addr().unwrap();
    let obs = o.clone();
    let socks_task = tokio::spawn(async move {
        let mut children = tokio::task::JoinSet::new();
        loop {
            tokio::select! {Ok((socket,_))=listener.accept()=>{let obs=obs.clone();children.spawn(async move{let _=tokio::time::timeout(std::time::Duration::from_secs(8),socks_connection(socket,obs)).await;});},_=children.join_next(),if !children.is_empty()=>{}}
        }
    });
    let secure_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let secure_target = secure_listener.local_addr().unwrap();
    let secure_acceptor = acceptor();
    let secure_obs = o.clone();
    let secure_task = tokio::spawn(async move {
        let mut children = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                Ok((socket,_))=secure_listener.accept()=>{let acceptor=secure_acceptor.clone();let o=secure_obs.clone();children.spawn(async move {
                    let _=tokio::time::timeout(std::time::Duration::from_secs(8),async move {
                        if let Ok(mut stream)=acceptor.accept(socket).await {let mut data=vec![];let mut b=[0];while !data.ends_with(b"\r\n\r\n")&&data.len()<8192 {if stream.read_exact(&mut b).await.is_err(){return;}data.push(b[0]);}o.targets.fetch_add(1,Ordering::SeqCst);let _=stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecure").await;}
                    }).await;
                });},
                _=children.join_next(),if !children.is_empty()=>{}
            }
        }
    });
    Fixture {
        target: target_addr,
        secure_target,
        http,
        https,
        socks,
        observed: o,
        tasks: vec![target_task, http_task, https_task, socks_task, secure_task],
    }
}
fn acceptor() -> TlsAcceptor {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
    let certs = CertificateDer::pem_slice_iter(include_bytes!("../fixtures/localhost-test.crt"))
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    let key =
        PrivateKeyDer::from_pem_slice(include_bytes!("../fixtures/localhost-test.key")).unwrap();
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .unwrap();
    TlsAcceptor::from(Arc::new(config))
}
async fn proxy(o: Observed, target: SocketAddr, tls: bool) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let acceptor = acceptor();
    let task = tokio::spawn(async move {
        let mut children = tokio::task::JoinSet::new();
        loop {
            tokio::select! {Ok((socket,_))=listener.accept()=>{let o=o.clone();let acceptor=acceptor.clone();children.spawn(async move{let future=async{if tls {if let Ok(stream)=acceptor.accept(socket).await{http_connection(stream,o,target).await;}}else{http_connection(socket,o,target).await;}};let _=tokio::time::timeout(std::time::Duration::from_secs(8),future).await;});},_=children.join_next(),if !children.is_empty()=>{}}
        }
    });
    (addr, task)
}
async fn http_connection<S: AsyncRead + AsyncWrite + Unpin>(
    mut socket: S,
    o: Observed,
    target: SocketAddr,
) {
    let mut data = vec![];
    let mut byte = [0];
    while !data.ends_with(b"\r\n\r\n") && data.len() < 16384 {
        if socket.read_exact(&mut byte).await.is_err() {
            return;
        }
        data.push(byte[0]);
    }
    let headers = String::from_utf8_lossy(&data);
    let first = headers.lines().next().unwrap_or("");
    for line in headers.lines() {
        if line.to_lowercase().starts_with("proxy-authorization:") {
            o.auth.lock().unwrap().push(line.to_owned());
        }
    }
    if o.refusal.load(Ordering::SeqCst) {
        let _=socket.write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        return;
    }
    let Some(destination) = first.split_whitespace().nth(1) else {
        return;
    };
    if first.starts_with("CONNECT ") {
        o.domains.lock().unwrap().push(destination.into());
        let port = destination
            .rsplit_once(':')
            .and_then(|(_, p)| p.parse::<u16>().ok())
            .unwrap_or(target.port());
        if let Ok(mut upstream) = TcpStream::connect(("127.0.0.1", port)).await {
            let _ = socket
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .await;
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
        }
    } else {
        let Ok(url) = url::Url::parse(destination) else {
            return;
        };
        o.domains
            .lock()
            .unwrap()
            .push(url.host_str().unwrap_or("").into());
        if let Ok(mut upstream) = TcpStream::connect(target).await {
            let path = if let Some(q) = url.query() {
                format!("{}?{q}", url.path())
            } else {
                url.path().to_owned()
            };
            let request = headers.replacen(destination, &path, 1);
            let _ = upstream.write_all(request.as_bytes()).await;
            let _ = tokio::io::copy_bidirectional(&mut socket, &mut upstream).await;
        }
    }
}
async fn socks_connection(mut socket: TcpStream, o: Observed) -> std::io::Result<()> {
    let mut header = [0; 2];
    socket.read_exact(&mut header).await?;
    let mut methods = vec![0; header[1] as usize];
    socket.read_exact(&mut methods).await?;
    if methods.contains(&2) {
        socket.write_all(&[5, 2]).await?;
        let mut auth = [0; 2];
        socket.read_exact(&mut auth).await?;
        let mut user = vec![0; auth[1] as usize];
        socket.read_exact(&mut user).await?;
        let length = socket.read_u8().await?;
        let mut pass = vec![0; length as usize];
        socket.read_exact(&mut pass).await?;
        o.auth.lock().unwrap().push(format!(
            "{}:{}",
            String::from_utf8_lossy(&user),
            String::from_utf8_lossy(&pass)
        ));
        socket.write_all(&[1, 0]).await?;
    } else {
        socket.write_all(&[5, 0]).await?;
    }
    let mut request = [0; 4];
    socket.read_exact(&mut request).await?;
    let host = match request[3] {
        3 => {
            let len = socket.read_u8().await?;
            let mut host = vec![0; len as usize];
            socket.read_exact(&mut host).await?;
            String::from_utf8_lossy(&host).into_owned()
        }
        1 => {
            let mut host = [0; 4];
            socket.read_exact(&mut host).await?;
            std::net::Ipv4Addr::from(host).to_string()
        }
        _ => return Err(std::io::Error::other("unsupported address")),
    };
    let port = socket.read_u16().await?;
    o.domains.lock().unwrap().push(host);
    if o.refusal.load(Ordering::SeqCst) {
        socket.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
        return Ok(());
    }
    let mut upstream = TcpStream::connect(("127.0.0.1", port)).await?;
    socket.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).await?;
    tokio::io::copy_bidirectional(&mut socket, &mut upstream).await?;
    Ok(())
}
pub fn direct(dir: &std::path::Path) -> upwork_connector::route::Route {
    let p = dir.join("config/connections.json");
    upwork_connector::config::update(&p, |c| {
        c.mode = upwork_connector::config::Mode::Direct;
        Ok(())
    })
    .unwrap();
    upwork_connector::route::Route::new(p).unwrap()
}
pub fn route(dir: &std::path::Path, proxy: String) -> upwork_connector::route::Route {
    let p = dir.join("config/connections.json");
    upwork_connector::config::update(&p, |c| {
        c.mode = upwork_connector::config::Mode::Proxy;
        c.selected = Some("fixture".into());
        c.profiles = vec![upwork_connector::config::Profile {
            name: "fixture".into(),
            label: None,
            url: proxy,
        }];
        Ok(())
    })
    .unwrap();
    upwork_connector::route::Route::new(p).unwrap()
}
pub fn temp() -> tempfile::TempDir {
    std::fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("rust-network-")
        .tempdir_in(".ci-work")
        .unwrap()
}
