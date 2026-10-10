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
        if let Ok(mut lock) = self.file.lock() {
            if let Some(f) = lock.as_mut() {
                if f.write_all(line.as_bytes())
                    .and_then(|()| f.flush())
                    .is_err()
                {
                    eprintln!("Diagnostic log write failed.");
                }
            } else {
                eprint!("{line}");
            }
        }
    }
}
