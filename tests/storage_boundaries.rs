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
fn interrupted_staging_preserves_original_and_retains_unidentified_orphan() {
    use std::io::{BufRead, BufReader};
    let dir = temp();
    let root = fs::canonicalize(dir.path()).unwrap();
    let path = root.join("config.json");
    fs::write(&path, b"original-state").unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "staging_crash_child", "--nocapture"])
        .env("UPWORK_STAGING_FIXTURE", &root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.unwrap().contains("STAGING_READY") {
                let _ = tx.send(());
                break;
            }
        }
    });
    let ready = rx.recv_timeout(Duration::from_secs(3));
    let exclusion = if ready.is_ok() {
        Lock::try_acquire(&path)
    } else {
        Ok(None)
    };
    let killed = child.kill();
    let status = child.wait().unwrap();
    reader.join().unwrap();
    ready.unwrap();
    killed.unwrap();
    assert!(
        !status.success(),
        "Killed staging is not a successful write"
    );
    assert!(
        exclusion.unwrap().is_none(),
        "Live writer ownership must remain exclusive"
    );
    assert_eq!(fs::read(&path).unwrap(), b"original-state");
    assert!(Lock::acquire(&path).is_ok());
    let orphans: Vec<_> = fs::read_dir(&root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "tmp"))
        .collect();
    assert_eq!(orphans.len(), 1);
    upwork_connector::register::prepare_cache(&root).unwrap();
    upwork_connector::cleanup::sweep(
        &root,
        true,
        std::time::SystemTime::now() + Duration::from_secs(25 * 3600),
    )
    .unwrap();
    assert_eq!(fs::read(&orphans[0]).unwrap(), b"staged-dummy-state");
}

#[test]
fn staging_crash_child() {
    use std::io::{Read, Write};
    let Some(root) = std::env::var_os("UPWORK_STAGING_FIXTURE") else {
        return;
    };
    let root = std::path::Path::new(&root);
    let _guard = Lock::acquire(&root.join("config.json")).unwrap();
    // Model the documented pre-rename boundary with dummy bytes; no production crash hook.
    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut staged = options
        .open(root.join(format!("{}.tmp", uuid::Uuid::new_v4())))
        .unwrap();
    staged.write_all(b"staged-dummy-state").unwrap();
    staged.sync_all().unwrap();
    println!("STAGING_READY");
    std::io::stdout().flush().unwrap();
    let mut byte = [0];
    let _ = std::io::stdin().read(&mut byte);
    std::process::exit(24);
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
