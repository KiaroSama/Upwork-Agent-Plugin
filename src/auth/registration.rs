use crate::{credentials::Store, error::Result};
use rmcp::transport::auth::{AuthorizationManager, CredentialStore};
use serde_json::Value;
use tokio::net::TcpListener;
use url::Url;

pub(super) async fn initialize(
    manager: &mut AuthorizationManager,
    store: &Store,
) -> Result<(Option<Value>, TcpListener, String, Url)> {
    // Existing registrations need no new transaction; first publication rereads under ownership.
    let ownership = if store.client_registration()?.is_none() {
        store.acquire_refresh_guard().await?
    } else {
        None
    };
    let mut client = store.client_registration()?;
    let saved = super::local_callback::stored_redirect(client.as_ref())?;
    let (listener, redirect, target) = super::local_callback::bind(saved).await?;
    if client.is_none() {
        let config = manager
            .register_client("Upwork Native Connector", &redirect, &[])
            .await?;
        store.save_registration(&config)?;
        client = store.client_registration()?;
    }
    drop(ownership);
    Ok((client, listener, redirect, target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::transport::auth::{OAuthHttpClient, OAuthHttpClientFuture, OAuthHttpRequest};
    use serde_json::json;
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
        entered: Semaphore,
        release: Semaphore,
        calls: AtomicUsize,
        reject: bool,
    }
    impl OAuthHttpClient for Http {
        fn execute(&self, request: OAuthHttpRequest) -> OAuthHttpClientFuture<'_> {
            Box::pin(async move {
                assert_eq!(request.request.uri().path(), "/register");
                self.calls.fetch_add(1, Ordering::SeqCst);
                self.entered.add_permits(1);
                self.release.acquire().await.unwrap().forget();
                let registration: Value = serde_json::from_slice(request.request.body()).unwrap();
                let body = if self.reject {
                    json!({"error":"invalid_client_metadata"})
                } else {
                    json!({"client_id":"synthetic-client","redirect_uris":registration["redirect_uris"],"token_endpoint_auth_method":"none"})
                };
                Ok(http::Response::builder()
                    .status(if self.reject { 400 } else { 201 })
                    .header("content-type", "application/json")
                    .body(serde_json::to_vec(&body).unwrap())
                    .unwrap())
            })
        }
    }
    async fn manager(http: Arc<Http>, store: &Store) -> AuthorizationManager {
        let mut manager =
            AuthorizationManager::new_with_oauth_http_client("https://fixture.invalid/mcp", http)
                .await
                .unwrap();
        manager.set_metadata(serde_json::from_value(json!({"issuer":"https://fixture.invalid",
            "authorization_endpoint":"https://fixture.invalid/authorize","token_endpoint":"https://fixture.invalid/token",
            "registration_endpoint":"https://fixture.invalid/register","response_types_supported":["code"],
            "code_challenge_methods_supported":["S256"],"token_endpoint_auth_methods_supported":["none"]})).unwrap());
        manager.set_credential_store(store.clone());
        manager
    }
    #[tokio::test]
    async fn concurrent_first_registration_rereads_and_releases_before_consent() {
        tokio::time::timeout(Duration::from_secs(3), async {
            fs::create_dir_all(".ci-work").unwrap();
            let root = tempfile::tempdir_in(".ci-work").unwrap();
            let store = Store {
                root: root.path().into(),
            };
            let http = Arc::new(Http {
                entered: Semaphore::new(0),
                release: Semaphore::new(0),
                calls: AtomicUsize::new(0),
                reject: false,
            });
            let (mut first, mut second) = (
                manager(http.clone(), &store).await,
                manager(http.clone(), &store).await,
            );
            let mut one = Box::pin(initialize(&mut first, &store));
            tokio::select! {
                _ = &mut one => panic!("Registration completed before its HTTP gate"),
                permit = http.entered.acquire() => permit.unwrap().forget(),
            }
            let mut two = Box::pin(initialize(&mut second, &store));
            assert!(futures::poll!(&mut two).is_pending());
            assert_eq!(http.calls.load(Ordering::SeqCst), 1);
            http.release.add_permits(1);
            let (client, listener, _, _) = one.await.unwrap();
            assert_eq!(client.unwrap()["client_id"], "synthetic-client");
            // The saved callback is occupied: the competing initializer safely refuses, not re-registers.
            assert!(two.await.is_err());
            assert_eq!(http.calls.load(Ordering::SeqCst), 1);
            assert!(store.acquire_refresh_guard().await.unwrap().is_some());
            drop(listener);
            assert!(initialize(&mut second, &store).await.is_ok());
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn existing_registration_does_not_wait_for_the_refresh_owner() {
        tokio::time::timeout(Duration::from_secs(2), async {
            fs::create_dir_all(".ci-work").unwrap();
            let root = tempfile::tempdir_in(".ci-work").unwrap();
            let store = Store {
                root: root.path().into(),
            };
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let redirect = format!(
                "http://127.0.0.1:{}/oauth/callback",
                listener.local_addr().unwrap().port()
            );
            store
                .save_registration(&rmcp::transport::auth::OAuthClientConfig::new(
                    "synthetic-client",
                    &redirect,
                ))
                .unwrap();
            drop(listener);
            let owner = store.acquire_refresh_guard().await.unwrap();
            let http = Arc::new(Http {
                entered: Semaphore::new(0),
                release: Semaphore::new(0),
                calls: AtomicUsize::new(0),
                reject: false,
            });
            let mut manager = manager(http.clone(), &store).await;
            assert!(initialize(&mut manager, &store).await.is_ok());
            assert_eq!(http.calls.load(Ordering::SeqCst), 0);
            drop(owner);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn cancelled_or_failed_registration_preserves_absence_and_releases_ownership() {
        for reject in [false, true] {
            tokio::time::timeout(Duration::from_secs(3), async {
                fs::create_dir_all(".ci-work").unwrap();
                let root = tempfile::tempdir_in(".ci-work").unwrap();
                let store = Store {
                    root: root.path().into(),
                };
                let http = Arc::new(Http {
                    entered: Semaphore::new(0),
                    release: Semaphore::new(0),
                    calls: AtomicUsize::new(0),
                    reject,
                });
                let mut manager = manager(http.clone(), &store).await;
                let mut pending = Box::pin(initialize(&mut manager, &store));
                tokio::select! {
                    _ = &mut pending => panic!("Registration completed before its HTTP gate"),
                    permit = http.entered.acquire() => permit.unwrap().forget(),
                }
                if reject {
                    http.release.add_permits(1);
                    assert!(pending.await.is_err());
                } else {
                    drop(pending);
                }
                assert!(store.client_registration().unwrap().is_none());
                assert!(store.acquire_refresh_guard().await.unwrap().is_some());
            })
            .await
            .unwrap();
        }
    }
}
