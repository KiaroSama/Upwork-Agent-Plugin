use std::{
    fs,
    time::{Duration, SystemTime},
};
use upwork_connector::{cleanup, register};

fn root() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    let dir = tempfile::tempdir_in(".ci-work").unwrap();
    register::prepare_cache(dir.path()).unwrap();
    dir
}

#[test]
fn dry_run_stale_owned_only_and_locked_files_survive() {
    let dir = root();
    let owned = dir
        .path()
        .join(".cache/tmp")
        .join(format!("upwork-{}.tmp", uuid::Uuid::new_v4()));
    fs::write(&owned, "disposable").unwrap();
    let unknown = dir.path().join(".cache/tmp/input.txt");
    fs::write(&unknown, "user input").unwrap();
    let now = SystemTime::now() + Duration::from_secs(25 * 3600);
    assert_eq!(
        cleanup::sweep(dir.path(), false, now).unwrap().candidates,
        1
    );
    assert!(owned.exists());
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&owned)
        .unwrap();
    file.try_lock().unwrap();
    assert_eq!(cleanup::sweep(dir.path(), true, now).unwrap().removed, 0);
    drop(file);
    assert_eq!(
        cleanup::sweep(dir.path(), true, SystemTime::now())
            .unwrap()
            .removed,
        0
    );
    assert_eq!(cleanup::sweep(dir.path(), true, now).unwrap().removed, 1);
    assert!(unknown.exists());
}

#[test]
fn cleanup_discloses_truncated_directory_coverage() {
    let dir = root();
    for index in 0..1000 {
        fs::write(
            dir.path().join(format!(".cache/tmp/unknown-{index}")),
            b"keep",
        )
        .unwrap();
    }
    assert!(
        !cleanup::sweep(dir.path(), false, SystemTime::now())
            .unwrap()
            .inspection_truncated
    );
    fs::write(dir.path().join(".cache/tmp/extra"), b"keep").unwrap();
    let report = cleanup::sweep(dir.path(), false, SystemTime::now()).unwrap();
    assert!(report.inspection_truncated);
    assert_eq!(report.skipped, 1000);
    assert_eq!(report.removed, 0);
    assert_eq!(
        fs::read_dir(dir.path().join(".cache/tmp")).unwrap().count(),
        1001
    );
}

#[test]
fn maintenance_exclusive_refuses_active_runtime() {
    let dir = root();
    let _runtime = cleanup::lease(dir.path(), false).unwrap();
    assert!(cleanup::lease(dir.path(), true).is_err());
    let _second = cleanup::lease(dir.path(), false).unwrap();
}

#[cfg(unix)]
#[test]
fn linked_temp_root_and_linked_entries_are_never_deleted() {
    let dir = root();
    let outside = tempfile::tempdir_in(".ci-work").unwrap();
    let original = outside.path().join("keep");
    fs::write(&original, "keep").unwrap();
    let link = dir
        .path()
        .join(".cache/tmp")
        .join(format!("upwork-{}.tmp", uuid::Uuid::new_v4()));
    std::os::unix::fs::symlink(&original, &link).unwrap();
    let now = SystemTime::now() + Duration::from_secs(25 * 3600);
    assert_eq!(cleanup::sweep(dir.path(), true, now).unwrap().removed, 0);
    assert!(original.exists());
    fs::remove_file(link).unwrap();
    fs::remove_dir(dir.path().join(".cache/tmp")).unwrap();
    std::os::unix::fs::symlink(outside.path(), dir.path().join(".cache/tmp")).unwrap();
    assert!(cleanup::sweep(dir.path(), true, now).is_err());
}
