use crate::error::{Error, Result};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    time::{Duration, SystemTime},
};

pub const STALE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Default, Serialize)]
pub struct Report {
    pub candidates: usize,
    pub removed: usize,
    pub skipped: usize,
}

pub(crate) fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

pub fn lease(root: &Path, exclusive: bool) -> Result<File> {
    let cache = root.join(".cache");
    if linked(&fs::symlink_metadata(&cache)?) {
        return Err(Error::Safe("Linked cache directory refused."));
    }
    let path = cache.join("maintenance.lock");
    if let Ok(meta) = fs::symlink_metadata(&path)
        && (linked(&meta) || !meta.is_file())
    {
        return Err(Error::Safe("Invalid maintenance lock."));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    let locked = if exclusive {
        file.try_lock()
    } else {
        file.try_lock_shared()
    };
    locked.map_err(|_| {
        Error::Safe("Connector is active or maintenance is busy. Retry after closing it.")
    })?;
    Ok(file)
}

pub fn sweep(root: &Path, apply: bool, now: SystemTime) -> Result<Report> {
    let temp = root.join(".cache/tmp");
    if linked(&fs::symlink_metadata(&root.join(".cache"))?) || linked(&fs::symlink_metadata(&temp)?)
    {
        return Err(Error::Safe("Linked temporary directory refused."));
    }
    let mut report = Report::default();
    for entry in fs::read_dir(temp)?.take(1000) {
        let entry = entry?;
        let path = entry.path();
        let meta = fs::symlink_metadata(&path)?;
        let name = entry.file_name();
        let owned = name
            .to_str()
            .and_then(|name| name.strip_prefix("upwork-")?.strip_suffix(".tmp"));
        if !meta.is_file()
            || linked(&meta)
            || owned.is_none_or(|id| uuid::Uuid::parse_str(id).is_err())
            || now.duration_since(meta.modified()?).unwrap_or_default() < STALE_AGE
        {
            report.skipped += 1;
            continue;
        }
        let file = match OpenOptions::new().read(true).write(true).open(&path) {
            Ok(file) => file,
            Err(_) => {
                report.skipped += 1;
                continue;
            }
        };
        if file.try_lock().is_err() {
            report.skipped += 1;
            continue;
        }
        report.candidates += 1;
        if apply {
            // Windows cannot unlink our open handle; the project maintenance lease
            // excludes cooperating connector runs while the stale file is removed.
            drop(file);
            if fs::remove_file(path).is_ok() {
                report.removed += 1;
            } else {
                report.skipped += 1;
            }
        }
    }
    Ok(report)
}
