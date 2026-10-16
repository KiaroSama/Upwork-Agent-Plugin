mod bounded;
pub use bounded::{MAX_STATE_BYTES, json_bytes, optional_bytes};

use crate::error::{Error, Result};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions, TryLockError},
    io::Write,
    path::Path,
};

pub struct Lock {
    _file: File,
}
impl Lock {
    pub fn acquire(path: &Path) -> Result<Self> {
        Self::try_acquire(path)?.ok_or(Error::Safe(
            "Configuration is busy. Retry after the current operation.",
        ))
    }
    pub fn try_acquire(path: &Path) -> Result<Option<Self>> {
        let path = path.with_extension("json.lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::symlink_metadata(&path) {
            Ok(meta) if crate::cleanup::linked(&meta) || !meta.is_file() => {
                return Err(Error::Safe("Linked or invalid state lock refused."));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let file = private_options()
            .read(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(error.into()),
        }
        // Keep the lock inode. Unlinking it lets waiters lock different inodes.
        // The OS reclaims ownership when the final underlying handle closes.
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        // A concurrent fork can retain this open file description until exec.
        // Explicit unlock releases ownership even before that inherited handle closes.
        let _ = self._file.unlock();
    }
}
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic_bytes(path, &json_bytes(value)?)
}
pub fn atomic_bytes(path: &Path, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_STATE_BYTES {
        return Err(Error::Safe(
            "Internal state exceeds 16 MiB; original data preserved.",
        ));
    }
    let parent = path.parent().ok_or(Error::Safe("Invalid state path."))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = private_options().create_new(true).open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        // Neither original nor destination is removed before the atomic replacement.
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn guard_release_unlocks_a_file_description_retained_by_an_inherited_handle() {
        fs::create_dir_all(".ci-work").unwrap();
        let dir = tempfile::tempdir_in(".ci-work").unwrap();
        let path = dir.path().join("config.json");
        let guard = Lock::acquire(&path).unwrap();
        // A fork or duplicated descriptor retains the same open file description.
        let inherited = guard._file.try_clone().unwrap();
        drop(guard);
        let replacement = Lock::acquire(&path)
            .expect("Releasing the guard must unlock even while an inherited handle is alive");
        drop(inherited);
        assert!(Lock::acquire(&path).is_err());
        drop(replacement);
        assert!(Lock::acquire(&path).is_ok());
    }
}
