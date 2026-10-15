use super::auth_error;
use crate::storage::Lock;
use rmcp::transport::auth::AuthError;
use std::{path::Path, time::Duration};

const WAIT_LIMIT: Duration = Duration::from_secs(60);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

pub(super) async fn acquire(path: &Path) -> Result<Lock, AuthError> {
    acquire_for(path, WAIT_LIMIT).await
}

async fn acquire_for(path: &Path, limit: Duration) -> Result<Lock, AuthError> {
    // A competing refresh is normal contention, not a broken credential store.
    // Poll only the OS lock: never retry or replay a token request here.
    // No blocking worker is spawned, so cancellation cannot orphan an owner.
    tokio::time::timeout(limit, async {
        loop {
            match Lock::try_acquire(path) {
                Ok(Some(lock)) => return Ok(lock),
                Ok(None) => tokio::time::sleep(POLL_INTERVAL).await,
                Err(_) => return Err(auth_error()),
            }
        }
    })
    .await
    .map_err(|_| {
        AuthError::CredentialStoreError(
            "Credential refresh lock wait timed out; retry after the current refresh.".into(),
        )
    })?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp() -> tempfile::TempDir {
        fs::create_dir_all(".ci-work").unwrap();
        tempfile::tempdir_in(".ci-work").unwrap()
    }

    #[tokio::test(start_paused = true)]
    async fn uncontested_refresh_lock_is_immediate_and_exclusive() {
        let root = temp();
        let path = root.path().join("refresh.json");
        let start = tokio::time::Instant::now();
        let guard = acquire_for(&path, Duration::from_secs(1)).await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert!(Lock::try_acquire(&path).unwrap().is_none());
        drop(guard);
        assert!(Lock::try_acquire(&path).unwrap().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn busy_refresh_lock_has_a_total_deadline_and_keeps_owner_exclusive() {
        let root = temp();
        let path = root.path().join("refresh.json");
        let owner = Lock::acquire(&path).unwrap();
        let start = tokio::time::Instant::now();
        let result = acquire_for(&path, Duration::from_millis(150)).await;
        assert!(matches!(result, Err(AuthError::CredentialStoreError(_))));
        assert_eq!(start.elapsed(), Duration::from_millis(150));
        assert!(Lock::try_acquire(&path).unwrap().is_none());
        drop(owner);
        assert!(Lock::try_acquire(&path).unwrap().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn refresh_waiter_acquires_after_owner_release() {
        let root = temp();
        let path = root.path().join("refresh.json");
        let owner = Lock::acquire(&path).unwrap();
        let release = async {
            tokio::time::sleep(Duration::from_millis(100)).await;
            drop(owner);
        };
        let (result, ()) = tokio::join!(acquire_for(&path, Duration::from_secs(1)), release);
        let next = result.unwrap();
        assert!(Lock::try_acquire(&path).unwrap().is_none());
        drop(next);
        assert!(Lock::try_acquire(&path).unwrap().is_some());
    }

    #[tokio::test(start_paused = true)]
    async fn cancelled_waiter_never_acquires_later_or_leaks_a_lock() {
        let root = temp();
        let path = root.path().join("refresh.json");
        let owner = Lock::acquire(&path).unwrap();
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            acquire_for(&path, Duration::from_secs(60)),
        )
        .await;
        assert!(result.is_err());
        drop(owner);
        tokio::time::advance(Duration::from_secs(61)).await;
        let next = Lock::acquire(&path).unwrap();
        assert!(Lock::try_acquire(&path).unwrap().is_none());
        drop(next);
    }

    #[tokio::test(start_paused = true)]
    async fn invalid_lock_is_a_safe_immediate_error_not_contention() {
        let root = temp();
        let path = root.path().join("private-fixture-marker.json");
        fs::create_dir(path.with_extension("json.lock")).unwrap();
        let start = tokio::time::Instant::now();
        let result = acquire_for(&path, Duration::from_secs(1)).await;
        let Err(error) = result else {
            panic!("Invalid lock must fail")
        };
        assert!(matches!(error, AuthError::CredentialStoreError(_)));
        assert!(!error.to_string().contains("private-fixture-marker"));
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[cfg(unix)]
    #[tokio::test(start_paused = true)]
    async fn linked_lock_is_rejected_without_changing_its_target() {
        let root = temp();
        let path = root.path().join("refresh.json");
        let target = root.path().join("unrelated");
        fs::write(&target, b"preserve").unwrap();
        std::os::unix::fs::symlink(&target, path.with_extension("json.lock")).unwrap();
        assert!(acquire_for(&path, Duration::from_secs(1)).await.is_err());
        assert_eq!(fs::read(target).unwrap(), b"preserve");
    }
}
