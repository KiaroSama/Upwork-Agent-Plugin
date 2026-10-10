use crate::error::{Error, Result};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub struct Lock {
    path: PathBuf,
    file: Option<File>,
}
impl Lock {
    pub fn acquire(path: &Path) -> Result<Self> {
        let path = path.with_extension("json.lock");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = private_options()
            .create_new(true)
            .open(&path)
            .map_err(|_| Error::Safe("Configuration is busy. Retry or inspect its stale lock."))?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }
}
impl Drop for Lock {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
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
    let parent = path.parent().ok_or(Error::Safe("Invalid state path."))?;
    fs::create_dir_all(parent)?;
    let temp = parent.join(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = private_options().create_new(true).open(&temp)?;
        serde_json::to_writer_pretty(&mut file, value)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
        drop(file);
        // Windows rename replaces files atomically; neither original nor destination is removed first.
        fs::rename(&temp, path)?;
        Ok(())
    })();
    if temp.exists() {
        let _ = fs::remove_file(&temp);
    }
    result
}
