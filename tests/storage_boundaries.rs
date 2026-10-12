use std::{
    fs,
    process::Command,
    time::{Duration, Instant},
};
use upwork_connector::storage::Lock;

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("storage boundaries ")
        .tempdir_in(".ci-work")
        .unwrap()
}

#[test]
fn abrupt_exit_releases_the_operating_system_lock() {
    let dir = temp();
    let path = fs::canonicalize(dir.path()).unwrap().join("config.json");
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "crash_lock_child", "--nocapture"])
        .env("UPWORK_AUDIT_LOCK_PATH", &path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() <= Duration::from_secs(3) => {
                // std::process has no timed wait; bounded polling matches the CLI fixture.
                std::thread::sleep(Duration::from_millis(10));
            }
            result => {
                let _ = child.kill();
                child.wait().unwrap();
                panic!("Crash fixture wait failed or exceeded its deadline: {result:?}");
            }
        }
    };
    assert!(status.success());
    assert!(
        Lock::acquire(&path).is_ok(),
        "A dead owner must not leave configuration permanently busy"
    );
}

#[test]
fn crash_lock_child() {
    let Some(path) = std::env::var_os("UPWORK_AUDIT_LOCK_PATH") else {
        return;
    };
    let _guard = Lock::acquire(std::path::Path::new(&path)).unwrap();
    // Terminate without running destructors, as after an abrupt application exit.
    std::process::exit(0);
}

#[test]
fn a_live_owner_remains_exclusive_and_a_released_lock_can_be_reused() {
    let dir = temp();
    let path = dir.path().join("config.json");
    for _ in 0..3 {
        let owner = Lock::acquire(&path).unwrap();
        assert!(Lock::acquire(&path).is_err());
        drop(owner);
    }
}

#[cfg(unix)]
#[test]
fn linked_lock_files_are_rejected_without_touching_the_target() {
    let dir = temp();
    let target = dir.path().join("external");
    std::fs::write(&target, b"preserve").unwrap();
    let target = std::fs::canonicalize(target).unwrap();
    let path = dir.path().join("connections.json");
    std::os::unix::fs::symlink(&target, path.with_extension("json.lock")).unwrap();
    assert!(upwork_connector::storage::Lock::acquire(&path).is_err());
    assert_eq!(std::fs::read(target).unwrap(), b"preserve");
}
