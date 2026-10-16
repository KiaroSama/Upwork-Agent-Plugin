use crate::error::{Error, Result};
use serde::Serialize;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

// Shared read/write contract. Larger pre-existing state is refused unchanged,
// never truncated. This allows accumulated imports well beyond one input file.
pub const MAX_STATE_BYTES: usize = 16 * 1024 * 1024;

fn invalid() -> Error {
    Error::Safe("Internal state must be a regular file within 16 MiB; original data preserved.")
}

fn validate(metadata: &fs::Metadata) -> Result<()> {
    if !metadata.is_file() || metadata.len() > MAX_STATE_BYTES as u64 {
        return Err(invalid());
    }
    Ok(())
}

fn read_limited(reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Error::Safe("Internal state read failed; private details suppressed."))?;
    if bytes.len() > limit {
        return Err(invalid());
    }
    Ok(bytes)
}

fn open_regular(path: &Path, refuse_links: bool) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Handle validation follows open. O_NONBLOCK prevents a FIFO swapped
        // into the name between preflight and open from hanging this thread.
        let flags = libc::O_NONBLOCK | if refuse_links { libc::O_NOFOLLOW } else { 0 };
        options.custom_flags(flags);
    }
    #[cfg(windows)]
    if refuse_links {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options
        .open(path)
        .map_err(|_| Error::Safe("Internal state open failed; private details suppressed."))?;
    let metadata = file
        .metadata()
        .map_err(|_| Error::Safe("Internal state metadata unavailable."))?;
    if refuse_links && crate::cleanup::linked(&metadata) {
        return Err(invalid());
    }
    validate(&metadata)?;
    Ok(file)
}

pub fn optional_bytes(path: &Path, refuse_links: bool) -> Result<Option<Vec<u8>>> {
    let metadata = if refuse_links {
        fs::symlink_metadata(path)
    } else {
        fs::metadata(path)
    };
    let metadata = match metadata {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(Error::Safe("Internal state metadata unavailable.")),
    };
    if refuse_links && crate::cleanup::linked(&metadata) {
        return Err(invalid());
    }
    validate(&metadata)?;
    let file = open_regular(path, refuse_links)?;
    read_limited(file, MAX_STATE_BYTES).map(Some)
}

struct BoundedOutput(Vec<u8>);
impl Write for BoundedOutput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_STATE_BYTES.saturating_sub(self.0.len()) {
            return Err(io::Error::other("Internal state size limit exceeded."));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn json_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    let mut output = BoundedOutput(Vec::new());
    serde_json::to_writer_pretty(&mut output, value).map_err(|_| {
        Error::Safe(
            "Internal state serialization failed or exceeds 16 MiB; original data preserved.",
        )
    })?;
    output.write_all(b"\n").map_err(|_| invalid())?;
    Ok(output.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp() -> tempfile::TempDir {
        fs::create_dir_all(".ci-work").unwrap();
        tempfile::tempdir_in(".ci-work").unwrap()
    }

    #[test]
    fn bounded_reader_consumes_only_limit_plus_one() {
        let mut source = io::repeat(b'x').take(1000);
        assert!(read_limited(&mut source, 16).is_err());
        assert_eq!(source.limit(), 983);
        assert!(read_limited(io::empty(), 16).unwrap().is_empty());
        assert_eq!(
            read_limited(&b"1234567890123456"[..], 16).unwrap().len(),
            16
        );
    }

    struct Short {
        interrupted: bool,
        left: usize,
    }
    impl Read for Short {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let count = self.left.min(out.len()).min(3);
            out[..count].fill(b' ');
            self.left -= count;
            Ok(count)
        }
    }
    #[test]
    fn interrupted_short_reads_and_growth_remain_bounded() {
        assert_eq!(
            read_limited(
                Short {
                    interrupted: false,
                    left: 13
                },
                16
            )
            .unwrap()
            .len(),
            13
        );
        assert!(
            read_limited(
                Short {
                    interrupted: false,
                    left: 17
                },
                16
            )
            .is_err()
        );
    }
    #[test]
    fn missing_empty_directory_and_size_boundaries() {
        let root = temp();
        let path = root.path().join("state.json");
        assert!(optional_bytes(&path, false).unwrap().is_none());
        fs::write(&path, []).unwrap();
        assert!(optional_bytes(&path, false).unwrap().unwrap().is_empty());
        let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_len(MAX_STATE_BYTES as u64).unwrap();
        assert_eq!(
            optional_bytes(&path, false).unwrap().unwrap().len(),
            MAX_STATE_BYTES
        );
        file.set_len(MAX_STATE_BYTES as u64 + 1).unwrap();
        assert!(optional_bytes(&path, false).is_err());
        assert!(optional_bytes(root.path(), false).is_err());
    }
    #[test]
    fn bounded_serialization_round_trip_and_rejected_write_preserve_bytes() {
        let root = temp();
        let path = root.path().join("state.json");
        let large_valid = json!({"value":"x".repeat(2 * 1024 * 1024)});
        super::super::atomic_json(&path, &large_valid).unwrap();
        let original = optional_bytes(&path, false).unwrap().unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&original).unwrap(),
            large_valid
        );
        let oversized = json!({"value":"x".repeat(MAX_STATE_BYTES)});
        assert!(super::super::atomic_json(&path, &oversized).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(super::super::atomic_bytes(&path, &vec![b'x'; MAX_STATE_BYTES + 1]).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
    }
    #[test]
    fn utf8_and_shape_errors_are_left_to_each_json_reader() {
        let root = temp();
        let path = root.path().join("state.json");
        for bytes in [&[0xff][..], b"{", b"null"] {
            fs::write(&path, bytes).unwrap();
            assert_eq!(optional_bytes(&path, false).unwrap().unwrap(), bytes);
        }
    }
    #[cfg(unix)]
    #[test]
    fn regular_links_preserved_where_allowed_and_registration_links_refused() {
        let root = temp();
        let target = fs::canonicalize(root.path()).unwrap().join("target");
        fs::write(&target, b"{}").unwrap();
        let link = root.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(optional_bytes(&link, false).unwrap().unwrap(), b"{}");
        assert!(optional_bytes(&link, true).is_err());
        assert_eq!(fs::read(target).unwrap(), b"{}");
    }
    #[cfg(unix)]
    #[test]
    fn fifo_is_refused_before_open_and_by_nonblocking_handle_validation() {
        let root = temp();
        let path = root.path().join("fifo");
        assert!(
            std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        assert!(optional_bytes(&path, false).is_err());
        // Directly exercise the post-preflight path to model a substituted FIFO.
        assert!(open_regular(&path, false).is_err());
        assert!(open_regular(&path, true).is_err());
    }
}
