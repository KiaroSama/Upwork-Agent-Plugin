use rmcp::transport::auth::CredentialStore;
use std::{
    fs,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use upwork_connector::{
    config::{self, Mode},
    credentials::Store,
    route::Route,
    storage::Lock,
};

const CHILD_ROOT: &str = "UPWORK_TEST_REFRESH_OWNER_ROOT";

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::tempdir_in(".ci-work").unwrap()
}

struct OwnerProcess(Child);
impl Drop for OwnerProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn refresh_lock_owner_child() {
    let Some(root) = std::env::var_os(CHILD_ROOT) else {
        return;
    };
    let root = fs::canonicalize(root).unwrap();
    assert!(root.starts_with(fs::canonicalize(".ci-work").unwrap()));
    let store = Store { root: root.clone() };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let guard = runtime
        .block_on(store.acquire_refresh_guard())
        .unwrap()
        .unwrap();
    fs::write(root.join("owner-ready"), b"ready").unwrap();
    let start = Instant::now();
    while !root.join("owner-release").exists() {
        assert!(start.elapsed() < Duration::from_secs(20));
        std::thread::sleep(Duration::from_millis(10));
    }
    drop(guard);
}

async fn spawn_owner(root: &Path) -> OwnerProcess {
    let mut child = OwnerProcess(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "refresh_lock_owner_child", "--test-threads=1"])
            .env(CHILD_ROOT, root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while !root.join("owner-ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "Owner exited before acquiring its lock"
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("Owner acquisition must be bounded");
    child
}

async fn process_case(terminate: bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        let dir = temp();
        let root = fs::canonicalize(dir.path()).unwrap();
        let mut child = spawn_owner(&root).await;
        let store = Store { root: root.clone() };
        let waiting = store.acquire_refresh_guard();
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut waiting)
                .await
                .is_err()
        );
        if terminate {
            child.0.kill().unwrap();
        } else {
            fs::write(root.join("owner-release"), b"release").unwrap();
        }
        let acquired = tokio::time::timeout(Duration::from_secs(3), &mut waiting)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            Lock::try_acquire(&root.join(".state/rust/refresh.json"))
                .unwrap()
                .is_none()
        );
        let exit = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    break status;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(exit.success(), !terminate);
        drop(acquired);
        assert!(
            Lock::try_acquire(&root.join(".state/rust/refresh.json"))
                .unwrap()
                .is_some()
        );
    })
    .await
    .expect("Cross-process refresh ownership must remain bounded");
}

#[tokio::test]
async fn refresh_waiter_acquires_after_another_process_releases() {
    process_case(false).await;
}

#[tokio::test]
async fn refresh_waiter_recovers_after_the_owner_process_is_terminated() {
    process_case(true).await;
}

#[tokio::test]
async fn blocking_the_route_cancels_a_refresh_waiter_without_orphan_ownership() {
    let root = temp();
    let path = root.path().join("config/connections.json");
    config::update(&path, |value| {
        value.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let route = Route::new(path.clone()).unwrap();
    let store = Store {
        root: root.path().into(),
    };
    let owner = store.acquire_refresh_guard().await.unwrap().unwrap();
    {
        let waiting = route.guard(store.acquire_refresh_guard());
        tokio::pin!(waiting);
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut waiting)
                .await
                .is_err()
        );
        config::update(&path, |value| {
            value.mode = Mode::Blocked;
            Ok(())
        })
        .unwrap();
        let result = tokio::time::timeout(Duration::from_secs(2), &mut waiting)
            .await
            .unwrap();
        assert!(
            result.is_err(),
            "Blocked route must cancel the outer authentication guard"
        );
    }
    drop(owner);
    let next = tokio::time::timeout(Duration::from_secs(2), store.acquire_refresh_guard())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    drop(next);
    assert!(
        Lock::try_acquire(&root.path().join(".state/rust/refresh.json"))
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn different_installation_roots_do_not_share_refresh_ownership() {
    let first = temp();
    let second = temp();
    let one = Store {
        root: first.path().into(),
    };
    let two = Store {
        root: second.path().into(),
    };
    let owner = one.acquire_refresh_guard().await.unwrap().unwrap();
    let other = tokio::time::timeout(Duration::from_secs(2), two.acquire_refresh_guard())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(
        Lock::try_acquire(&first.path().join(".state/rust/refresh.json"))
            .unwrap()
            .is_none()
    );
    assert!(
        Lock::try_acquire(&second.path().join(".state/rust/refresh.json"))
            .unwrap()
            .is_none()
    );
    drop((owner, other));
}
