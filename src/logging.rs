use chrono::Utc;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
    sync::{Arc, Mutex},
};

#[derive(Clone)]
pub struct Log {
    file: Arc<Mutex<Option<File>>>,
    component: &'static str,
}
impl Log {
    pub fn new(root: &Path, component: &'static str) -> Self {
        let result = (|| {
            fs::create_dir_all(root.join("logs"))?;
            let name = format!(
                "{}_{}_UTC",
                component,
                Utc::now().format("%Y-%m-%d_%H-%M-%S")
            );
            for i in 0..100 {
                let suffix = if i == 0 {
                    String::new()
                } else {
                    format!("_{i}")
                };
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                match options.open(root.join("logs").join(format!("{name}{suffix}.log"))) {
                    Ok(f) => return Ok(f),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(std::io::Error::other("Log collision limit"))
        })();
        if result.is_err() {
            eprintln!("File logging unavailable; using safe stderr diagnostics.");
        }
        Self {
            file: Arc::new(Mutex::new(result.ok())),
            component,
        }
    }
    // Only fixed application messages enter this logger, never upstream errors or payloads.
    pub fn event(&self, level: &str, message: &str) {
        let line = format!(
            "[{} UTC] [{}] [{}] {}\n",
            Utc::now().format("%Y-%m-%d %H:%M:%S"),
            level,
            self.component,
            message
        );
        self.emit(&line, &mut std::io::stderr().lock());
    }

    fn emit(&self, line: &str, fallback: &mut impl Write) {
        let mut lock = self
            .file
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(file) = lock.as_mut() {
            if file
                .write_all(line.as_bytes())
                .and_then(|()| file.flush())
                .is_ok()
            {
                return;
            }
            *lock = None;
            let _ = fallback
                .write_all(b"Diagnostic log write failed; using safe stderr diagnostics.\n");
        }
        let _ = fallback.write_all(line.as_bytes());
        let _ = fallback.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_sink_is_disabled_and_poisoned_mutex_remains_usable() {
        fs::create_dir_all(".ci-work").unwrap();
        let dir = tempfile::tempdir_in(".ci-work").unwrap();
        let path = dir.path().join("sink");
        fs::write(&path, b"preserve").unwrap();
        let log = Log {
            file: Arc::new(Mutex::new(Some(File::open(&path).unwrap()))),
            component: "fixture",
        };
        let mut fallback = Vec::new();
        log.emit(
            "[ERROR] [fixture] Controlled safe fallback event.\n",
            &mut fallback,
        );
        log.emit("[WARNING] [fixture] Later safe event.\n", &mut fallback);
        let text = String::from_utf8(fallback).unwrap();
        assert_eq!(text.matches("Diagnostic log write failed").count(), 1);
        assert!(text.contains("[ERROR] [fixture] Controlled safe fallback event."));
        assert!(text.contains("[WARNING] [fixture] Later safe event."));
        assert!(log.file.lock().unwrap().is_none());
        assert_eq!(fs::read(&path).unwrap(), b"preserve");
        let shared = log.file.clone();
        let _ = std::thread::spawn(move || {
            let _guard = shared.lock().unwrap();
            panic!("controlled poison");
        })
        .join();
        let mut fallback = Vec::new();
        log.emit(
            "[WARNING] [fixture] Controlled event after poison.\n",
            &mut fallback,
        );
        assert!(
            String::from_utf8(fallback)
                .unwrap()
                .contains("Controlled event after poison.")
        );
    }
}
