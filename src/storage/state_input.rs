use crate::error::{Error, Result};
use std::{fs, io::Read, path::Path};

// Persisted JSON may aggregate many imports; do not reuse the 1 MiB import limit.
pub const MAX_STATE_BYTES: u64 = 16 * 1024 * 1024;

pub(super) fn validate_length(length: u64) -> Result<()> {
    if length > MAX_STATE_BYTES {
        return Err(Error::Safe(
            "JSON state exceeds 16 MiB; original file preserved.",
        ));
    }
    Ok(())
}

fn validate(metadata: &fs::Metadata) -> Result<()> {
    if !metadata.is_file() {
        return Err(Error::Safe(
            "JSON state must be a regular file; special input refused.",
        ));
    }
    validate_length(metadata.len())
}

fn bounded(reader: impl Read, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(Error::Safe(
            "JSON state exceeds its read limit; original file preserved.",
        ));
    }
    Ok(bytes)
}

fn from_file(file: fs::File) -> Result<Vec<u8>> {
    // Validate the descriptor as well as the earlier pathname. A file may grow
    // between checks, so bounded reading remains mandatory after metadata.
    validate(&file.metadata()?)?;
    bounded(file, MAX_STATE_BYTES)
}

pub fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    // Following a regular-file link is supported. A dangling link is invalid
    // state, not absence that permits default settings or legacy fallback.
    validate(&fs::metadata(path)?)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // A pathname swapped to a FIFO after metadata must not block in open.
        options.custom_flags(libc::O_NONBLOCK);
    }
    from_file(options.open(path)?).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config, credentials, settings, storage};
    use std::io::Write;

    fn fixture() -> tempfile::TempDir {
        fs::create_dir_all(".ci-work").unwrap();
        tempfile::tempdir_in(".ci-work").unwrap()
    }

    #[test]
    fn missing_input_is_absent_and_does_not_create_parents() {
        let root = fixture();
        let path = root.path().join("absent/credentials.json");
        assert!(read_optional(&path).unwrap().is_none());
        assert!(!path.parent().unwrap().exists());
    }

    #[test]
    fn regular_bytes_are_preserved_and_callers_reject_invalid_json() {
        let root = fixture();
        let path = root.path().join("state.json");
        let original = b" { \"sentinel\": true } \n";
        fs::write(&path, original).unwrap();
        assert_eq!(read_optional(&path).unwrap().unwrap(), original);
        fs::write(&path, b"\xff").unwrap();
        assert!(credentials::optional_json(&path).is_err());
        assert_eq!(fs::read(path).unwrap(), b"\xff");
    }

    #[test]
    fn directory_is_not_optional_absence() {
        let root = fixture();
        assert!(read_optional(root.path()).is_err());
        assert!(credentials::optional_json(root.path()).is_err());
    }

    #[test]
    fn missing_callers_keep_their_existing_defaults() {
        let root = fixture();
        assert!(config::read(&root.path().join("config/connections.json")).is_ok());
        assert!(settings::read(root.path()).is_ok());
        assert!(
            credentials::Store {
                root: root.path().into()
            }
            .load_local()
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn aggregates_larger_than_an_import_still_load() {
        let root = fixture();
        let path = root.path().join("connections.json");
        let mut file = fs::File::create(&path).unwrap();
        file.write_all(br#"{"version":1,"mode":"blocked","selected":null,"profiles":[]}"#)
            .unwrap();
        file.write_all(&vec![b' '; 1024 * 1024 + 1]).unwrap();
        drop(file);
        assert!(config::read(&path).is_ok());
    }

    #[test]
    fn exact_limit_is_readable_and_next_byte_is_refused() {
        let root = fixture();
        let path = root.path().join("large.json");
        let file = fs::File::create(&path).unwrap();
        file.set_len(MAX_STATE_BYTES).unwrap();
        assert_eq!(
            read_optional(&path).unwrap().unwrap().len() as u64,
            MAX_STATE_BYTES
        );
        file.set_len(MAX_STATE_BYTES + 1).unwrap();
        assert!(read_optional(&path).is_err());
        assert_eq!(file.metadata().unwrap().len(), MAX_STATE_BYTES + 1);
    }

    #[test]
    fn growing_or_endless_reader_is_cut_off_after_one_extra_byte() {
        struct Counting {
            count: u64,
        }
        impl Read for Counting {
            fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
                output.fill(b'x');
                self.count += output.len() as u64;
                Ok(output.len())
            }
        }
        let mut reader = Counting { count: 0 };
        assert!(bounded(&mut reader, 64).is_err());
        assert_eq!(reader.count, 65);
        assert_eq!(bounded(&b"exact"[..], 5).unwrap(), b"exact");
    }

    #[test]
    fn oversized_publication_preserves_bytes_and_does_not_create_directories() {
        let root = fixture();
        let path = root.path().join("state.json");
        fs::write(&path, b"preserve").unwrap();
        let large = vec![b' '; MAX_STATE_BYTES as usize + 1];
        assert!(storage::atomic_bytes(&path, &large).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"preserve");
        assert!(storage::atomic_bytes(&root.path().join("absent/state.json"), &large).is_err());
        assert!(!root.path().join("absent").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    #[cfg(unix)]
    fn fifo(path: &Path) {
        assert!(
            std::process::Command::new("mkfifo")
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
    }

    #[cfg(unix)]
    #[test]
    fn regular_links_remain_supported_but_dangling_links_fail_closed() {
        let root = fixture();
        let target = fs::canonicalize(root.path()).unwrap().join("target.json");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("state.json");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(read_optional(&link).unwrap().unwrap(), b"{}");
        fs::remove_file(target).unwrap();
        assert!(read_optional(&link).is_err());
        assert!(fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    }

    #[cfg(unix)]
    #[test]
    fn fifo_and_linked_fifo_are_refused_without_a_writer() {
        let root = fixture();
        let path = fs::canonicalize(root.path()).unwrap().join("state.json");
        fifo(&path);
        assert!(read_optional(&path).is_err());
        let link = root.path().join("linked.json");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(read_optional(&link).is_err());
        assert!(credentials::optional_json(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn opened_fifo_and_device_descriptors_are_not_read() {
        use std::os::unix::fs::OpenOptionsExt;
        let root = fixture();
        let path = root.path().join("state.json");
        fifo(&path);
        let file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .unwrap();
        assert!(from_file(file).is_err());
        assert!(from_file(fs::File::open("/dev/null").unwrap()).is_err());
        assert!(read_optional(Path::new("/dev/zero")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn all_saved_credential_and_client_json_paths_reject_special_files() {
        for kind in ["credentials", "client", "tokens", "client_info"] {
            let root = fixture();
            let store = credentials::Store {
                root: root.path().into(),
            };
            let path = match kind {
                "credentials" => store.path(),
                "client" => root.path().join(".state/rust/client.json"),
                _ => store.legacy_path(kind),
            };
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fifo(&path);
            assert!(credentials::optional_json(&path).is_err());
            if kind == "credentials" || kind == "tokens" {
                assert!(store.load_local().is_err());
            } else {
                assert!(store.client_registration().is_err());
            }
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn route_guard_revokes_instead_of_blocking_on_a_replaced_config() {
        let root = fixture();
        let path = root.path().join("config/connections.json");
        config::update(&path, |value| {
            value.mode = config::Mode::Direct;
            Ok(())
        })
        .unwrap();
        let route = crate::route::Route::new(path.clone()).unwrap();
        let mut guarded = Box::pin(route.guard(std::future::pending::<()>()));
        assert!(futures::poll!(&mut guarded).is_pending());
        fs::remove_file(&path).unwrap();
        fifo(&path);
        let result = tokio::time::timeout(std::time::Duration::from_secs(1), guarded)
            .await
            .unwrap();
        assert!(result.is_err());
        assert!(route.check().is_err());
    }
}
