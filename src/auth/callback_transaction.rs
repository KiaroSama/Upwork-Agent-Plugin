use crate::credentials::Store;
use rmcp::transport::auth::{AuthError, AuthorizationSession, CredentialStore};

pub(super) async fn finish(
    store: &Store,
    session: &AuthorizationSession,
    code: &str,
    state: &str,
    issuer: Option<&str>,
) -> Result<(), AuthError> {
    // The SDK guards refresh transactions, but not authorization-code exchanges.
    // Use the same guard through exchange and persistence; never reacquire in save.
    let _guard = store.acquire_refresh_guard().await?;
    session
        .handle_callback_with_issuer(code, state, issuer)
        .await
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Lock;
    use rmcp::transport::auth::{
        AuthorizationManager, AuthorizationRequest, OAuthClientConfig, OAuthHttpClient,
        OAuthHttpClientFuture, OAuthHttpRequest,
    };
    use serde_json::{Value, json};
    use std::{
        fs,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use tokio::sync::Semaphore;

    struct Http {
        calls: AtomicUsize,
        hold_first: bool,
        release: Semaphore,
        reject: bool,
    }
    impl OAuthHttpClient for Http {
        fn execute(&self, operation: OAuthHttpRequest) -> OAuthHttpClientFuture<'_> {
            Box::pin(async move {
                assert!(operation.request.uri().path() == "/token");
                let index = self.calls.fetch_add(1, Ordering::SeqCst);
                let fields = url::form_urlencoded::parse(operation.request.body())
                    .into_owned()
                    .collect::<std::collections::HashMap<_, _>>();
                if fields
                    .get("grant_type")
                    .is_some_and(|value| value == "refresh_token")
                {
                    let expected = if index == 0 {
                        "synthetic-refresh-0"
                    } else {
                        "synthetic-refresh-1"
                    };
                    assert!(
                        fields
                            .get("refresh_token")
                            .is_some_and(|value| value == expected)
                    );
                }
                if self.hold_first && index == 0 {
                    self.release.acquire().await.unwrap().forget();
                }
                let (status, body) = if self.reject {
                    (400, json!({"error":"invalid_grant"}))
                } else {
                    (
                        200,
                        json!({"access_token":format!("synthetic-access-{}", index + 1),"token_type":"Bearer","expires_in":3600,"refresh_token":format!("synthetic-refresh-{}", index + 1)}),
                    )
                };
                Ok(http::Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(serde_json::to_vec(&body).unwrap())
                    .unwrap())
            })
        }
    }

    async fn fixture(
        hold_first: bool,
        reject: bool,
    ) -> (
        tempfile::TempDir,
        Store,
        Arc<Http>,
        AuthorizationSession,
        String,
    ) {
        fs::create_dir_all(".ci-work").unwrap();
        let root = tempfile::tempdir_in(".ci-work").unwrap();
        let store = Store {
            root: root.path().into(),
        };
        let initial = json!({"client_id":"fixture-client","token_response":{"access_token":"synthetic-access-0","token_type":"Bearer","expires_in":3600,"refresh_token":"synthetic-refresh-0"},"token_received_at":1,"granted_scopes":[],"issuer":"https://fixture.invalid"});
        store
            .save(serde_json::from_value(initial).unwrap())
            .await
            .unwrap();
        let http = Arc::new(Http {
            calls: AtomicUsize::new(0),
            hold_first,
            release: Semaphore::new(0),
            reject,
        });
        let manager = manager(store.clone(), http.clone()).await;
        let session = AuthorizationSession::new(
            manager,
            AuthorizationRequest::new("http://localhost:1/callback")
                .with_preregistered_client("fixture-client"),
        )
        .await
        .map_err(|_| ())
        .unwrap();
        let state = url::Url::parse(session.get_authorization_url())
            .unwrap()
            .query_pairs()
            .find(|(key, _)| key == "state")
            .unwrap()
            .1
            .into_owned();
        (root, store, http, session, state)
    }

    async fn manager(store: Store, http: Arc<Http>) -> AuthorizationManager {
        let mut manager =
            AuthorizationManager::new_with_oauth_http_client("https://fixture.invalid/mcp", http)
                .await
                .unwrap();
        manager.set_metadata(serde_json::from_value(json!({"issuer":"https://fixture.invalid","authorization_endpoint":"https://fixture.invalid/authorize","token_endpoint":"https://fixture.invalid/token","response_types_supported":["code"],"code_challenge_methods_supported":["S256"]})).unwrap());
        manager
            .configure_client(OAuthClientConfig::new(
                "fixture-client",
                "http://localhost:1/callback",
            ))
            .unwrap();
        manager.set_credential_store(store);
        manager
    }

    #[tokio::test]
    async fn manual_callback_waits_for_refresh_before_exchange_and_save() {
        let (_root, store, http, session, state) = fixture(false, false).await;
        let before = fs::read(store.path()).unwrap();
        let owner = store.acquire_refresh_guard().await.unwrap();
        let mut callback = Box::pin(finish(
            &store,
            &session,
            "synthetic-code",
            &state,
            Some("https://fixture.invalid"),
        ));
        assert!(futures::poll!(&mut callback).is_pending());
        assert_eq!(http.calls.load(Ordering::SeqCst), 0);
        assert!(fs::read(store.path()).unwrap() == before);
        drop(owner);
        tokio::time::timeout(Duration::from_secs(1), callback)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        assert!(fs::read(store.path()).unwrap() != before);
    }

    #[tokio::test]
    async fn callback_holds_guard_through_http_and_releases_after_save() {
        let (_root, store, http, session, state) = fixture(true, false).await;
        let before = fs::read(store.path()).unwrap();
        let mut callback = Box::pin(finish(
            &store,
            &session,
            "synthetic-code",
            &state,
            Some("https://fixture.invalid"),
        ));
        assert!(futures::poll!(&mut callback).is_pending());
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        assert!(
            Lock::try_acquire(&store.root.join(".state/rust/refresh.json"))
                .unwrap()
                .is_none()
        );
        assert!(fs::read(store.path()).unwrap() == before);
        http.release.add_permits(1);
        callback.await.unwrap();
        assert!(store.acquire_refresh_guard().await.unwrap().is_some());
        assert!(fs::read(store.path()).unwrap() != before);
    }

    #[tokio::test]
    async fn callback_cancellation_releases_guard_without_publishing_credentials() {
        let (_root, store, http, session, state) = fixture(true, false).await;
        let before = fs::read(store.path()).unwrap();
        let mut callback = Box::pin(finish(
            &store,
            &session,
            "synthetic-code",
            &state,
            Some("https://fixture.invalid"),
        ));
        assert!(futures::poll!(&mut callback).is_pending());
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        drop(callback);
        assert!(store.acquire_refresh_guard().await.unwrap().is_some());
        assert!(fs::read(store.path()).unwrap() == before);
    }

    #[tokio::test]
    async fn failed_exchange_preserves_session_and_releases_guard() {
        let (_root, store, http, session, state) = fixture(false, true).await;
        let before = fs::read(store.path()).unwrap();
        assert!(
            finish(
                &store,
                &session,
                "synthetic-code",
                &state,
                Some("https://fixture.invalid")
            )
            .await
            .is_err()
        );
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        assert!(fs::read(store.path()).unwrap() == before);
        assert!(store.acquire_refresh_guard().await.unwrap().is_some());
    }

    #[tokio::test]
    async fn forged_callback_has_no_exchange_and_valid_retry_remains_usable() {
        let (_root, store, http, session, state) = fixture(false, false).await;
        let before = fs::read(store.path()).unwrap();
        assert!(
            finish(
                &store,
                &session,
                "synthetic-code",
                &state,
                Some("https://forged.invalid")
            )
            .await
            .is_err()
        );
        assert_eq!(http.calls.load(Ordering::SeqCst), 0);
        assert!(fs::read(store.path()).unwrap() == before);
        finish(
            &store,
            &session,
            "synthetic-code",
            &state,
            Some("https://fixture.invalid"),
        )
        .await
        .unwrap();
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn refresh_waits_for_manual_exchange_and_reloads_its_saved_token() {
        let (_root, store, http, session, state) = fixture(true, false).await;
        let refresh_manager = manager(store.clone(), http.clone()).await;
        let mut callback = Box::pin(finish(
            &store,
            &session,
            "synthetic-code",
            &state,
            Some("https://fixture.invalid"),
        ));
        assert!(futures::poll!(&mut callback).is_pending());
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        let mut refresh = Box::pin(refresh_manager.refresh_token());
        assert!(futures::poll!(&mut refresh).is_pending());
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        http.release.add_permits(1);
        callback.await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), refresh)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(http.calls.load(Ordering::SeqCst), 2);
        let saved: Value = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert!(saved["token_response"]["refresh_token"] == "synthetic-refresh-2");
    }

    #[tokio::test]
    async fn concurrent_sdk_refreshes_wait_and_reload_the_rotated_token() {
        let (_root, store, http, session, _state) = fixture(true, false).await;
        let second_manager = manager(store.clone(), http.clone()).await;
        let mut first = Box::pin(session.auth_manager.refresh_token());
        assert!(futures::poll!(&mut first).is_pending());
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        let mut second = Box::pin(second_manager.refresh_token());
        assert!(futures::poll!(&mut second).is_pending());
        assert_eq!(http.calls.load(Ordering::SeqCst), 1);
        http.release.add_permits(1);
        first.await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), second)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(http.calls.load(Ordering::SeqCst), 2);
        let saved: Value = serde_json::from_slice(&fs::read(store.path()).unwrap()).unwrap();
        assert!(saved["token_response"]["refresh_token"] == "synthetic-refresh-2");
    }
}
