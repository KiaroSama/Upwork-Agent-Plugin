use axum::{Json, Router, extract::Form, http::StatusCode, response::IntoResponse, routing::post};
use rmcp::transport::auth::{
    AuthorizationManager, CredentialStore, OAuthClientConfig, StoredCredentials,
};
use serde_json::json;
use std::{
    collections::HashMap,
    fs,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::{sync::Notify, task::JoinHandle};
use upwork_connector::{
    auth,
    config::{self, Mode},
    credentials::Store,
    route::Route,
    storage::Lock,
};

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::tempdir_in(".ci-work").unwrap()
}

struct Server(JoinHandle<()>);
impl Drop for Server {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn competing_store_refresh_waits_without_changing_configuration_lock_policy() {
    let root = temp();
    let store = Store {
        root: root.path().into(),
    };
    let owner = store.acquire_refresh_guard().await.unwrap().unwrap();
    let second = Store {
        root: root.path().into(),
    };
    let waiting = second.acquire_refresh_guard();
    tokio::pin!(waiting);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), &mut waiting)
            .await
            .is_err(),
        "A concurrent refresh must wait, not fail immediately as a storage error"
    );
    drop(owner);
    let next = tokio::time::timeout(Duration::from_secs(2), &mut waiting)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        Lock::try_acquire(&root.path().join(".state/rust/refresh.json"))
            .unwrap()
            .is_none()
    );
    drop(next);
    let path = root.path().join("config/connections.json");
    let config_owner = Lock::acquire(&path).unwrap();
    assert!(
        Lock::acquire(&path).is_err(),
        "Ordinary configuration writes must remain fail-fast"
    );
    drop(config_owner);
}

async fn manager(root: &std::path::Path, origin: &str) -> AuthorizationManager {
    let path = root.join("config/connections.json");
    let mut manager = auth::manager(
        Route::new(path).unwrap(),
        Store { root: root.into() },
        &format!("{origin}/mcp"),
    )
    .await
    .unwrap();
    manager.set_metadata(
        serde_json::from_value(json!({
            "issuer": origin,
            "authorization_endpoint": format!("{origin}/authorize"),
            "token_endpoint": format!("{origin}/token"),
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "code_challenge_methods_supported": ["S256"],
            "token_endpoint_auth_methods_supported": ["none"]
        }))
        .unwrap(),
    );
    manager
        .configure_client(OAuthClientConfig::new(
            "fixture-client",
            "http://localhost:1/callback",
        ))
        .unwrap();
    manager
}

#[tokio::test]
async fn independent_oauth_managers_serialize_rotating_refreshes_without_disconnect() {
    tokio::time::timeout(Duration::from_secs(8), async {
        let root = temp();
        config::update(&root.path().join("config/connections.json"), |value| {
            value.mode = Mode::Direct;
            Ok(())
        }).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let entered = Arc::new(Notify::new());
        let release = Arc::new(Notify::new());
        let seen = calls.clone();
        let started = entered.clone();
        let released = release.clone();
        let app = Router::new().route("/token", post(move |Form(fields): Form<HashMap<String, String>>| {
            let seen = seen.clone();
            let started = started.clone();
            let released = released.clone();
            async move {
                let index = seen.fetch_add(1, Ordering::SeqCst);
                let expected = if index == 0 { "fixture-refresh-0" } else { "fixture-refresh-1" };
                if index > 1 || fields.get("grant_type").map(String::as_str) != Some("refresh_token")
                    || fields.get("refresh_token").map(String::as_str) != Some(expected) {
                    return (StatusCode::BAD_REQUEST, Json(json!({"error":"invalid_grant"}))).into_response();
                }
                if index == 0 {
                    started.notify_one();
                    released.notified().await;
                }
                Json(json!({
                    "access_token": format!("fixture-access-{}", index + 1),
                    "token_type": "bearer", "expires_in": 3600,
                    "refresh_token": format!("fixture-refresh-{}", index + 1)
                })).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let _server = Server(tokio::spawn(async move { let _ = axum::serve(listener, app).await; }));
        let store = Store { root: root.path().into() };
        let token = serde_json::from_value(json!({
            "access_token":"fixture-access-0", "token_type":"bearer", "expires_in":1,
            "refresh_token":"fixture-refresh-0"
        })).unwrap();
        store.save(StoredCredentials::new("fixture-client".into(), Some(token), vec![], Some(1))
            .with_issuer(Some(origin.clone()))).await.unwrap();
        let first = manager(root.path(), &origin).await;
        let second = manager(root.path(), &origin).await;
        let first_request = first.get_access_token();
        tokio::pin!(first_request);
        tokio::select! {
            _ = entered.notified() => {},
            _ = &mut first_request => panic!("First refresh must reach the gated token endpoint"),
        }
        let second_request = second.get_access_token();
        tokio::pin!(second_request);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut second_request).await.is_err(),
            "The second manager must wait for the first refresh instead of returning CredentialStoreError"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        release.notify_one();
        let (one, two) = tokio::join!(&mut first_request, &mut second_request);
        assert!(one.is_ok(), "First refresh must succeed");
        assert!(two.is_ok(), "Waiting refresh must use the newly stored refresh token");
        assert!(one.unwrap() == "fixture-access-1");
        assert!(two.unwrap() == "fixture-access-2");
        // The pinned SDK refreshes again after acquiring its guard. These are
        // serialized exchanges using successive tokens, not a replay or dedup claim.
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        let final_token = second.get_access_token().await;
        assert!(final_token.is_ok());
        assert!(final_token.unwrap() == "fixture-access-2");
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }).await.expect("Refresh concurrency fixture must remain bounded");
}
