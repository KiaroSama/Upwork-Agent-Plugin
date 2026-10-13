use crate::{
    config::{self, Connections, Mode, Profile},
    error::{Error, Result},
};
use regex::Regex;
use std::{collections::HashSet, fs, io::Read, path::Path};
use url::Url;

const MAX_IMPORT_BYTES: u64 = 1024 * 1024;

fn read_error() -> Error {
    Error::Safe("Cannot read proxy file. Check the path, permissions and UTF-8 encoding.")
}

fn validate_source(metadata: &fs::Metadata) -> Result<()> {
    if !metadata.is_file() {
        return Err(Error::Safe("Proxy input must be a regular UTF-8 file."));
    }
    if metadata.len() > MAX_IMPORT_BYTES {
        return Err(Error::Safe("Proxy file exceeds 1 MiB."));
    }
    Ok(())
}

fn bounded_bytes(reader: impl Read) -> Result<Vec<u8>> {
    let mut data = Vec::new();
    // The extra byte detects overflow without reading or allocating for the whole file.
    // Keep this bound even after checking metadata: a file may grow after it is opened.
    reader
        .take(MAX_IMPORT_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|_| read_error())?;
    if data.len() as u64 > MAX_IMPORT_BYTES {
        return Err(Error::Safe("Proxy file exceeds 1 MiB."));
    }
    Ok(data)
}

fn source_bytes(input: &str) -> Result<Vec<u8>> {
    // Reject ordinary FIFOs/devices before open, which can otherwise block indefinitely.
    validate_source(&fs::metadata(input).map_err(|_| read_error())?)?;
    let file = fs::File::open(input).map_err(|_| read_error())?;
    // Recheck the opened file, not just the earlier pathname metadata.
    validate_source(&file.metadata().map_err(|_| read_error())?)?;
    bounded_bytes(file)
}

pub fn records(text: &str) -> Result<Vec<Profile>> {
    let pattern =
        Regex::new(r"(?i)^(https?|socks5h?)://([^:\s]+|\[[^\]]+\]):(\d+):([^:]*):([^:]*):(.+)$")
            .map_err(|_| Error::Safe("Cannot initialize proxy parser."))?;
    let mut output = vec![];
    for (index, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let record = (|| -> Result<Profile> {
            let (url, label) = if let Some(c) = pattern.captures(line) {
                let mut url = Url::parse(&format!("{}://{}:{}", c[1].to_lowercase(), &c[2], &c[3]))
                    .map_err(|_| Error::ImportLine(index + 1))?;
                url.set_username(
                    &percent_encoding::utf8_percent_encode(
                        &c[4],
                        percent_encoding::NON_ALPHANUMERIC,
                    )
                    .to_string(),
                )
                .map_err(|_| Error::ImportLine(index + 1))?;
                url.set_password(Some(
                    &percent_encoding::utf8_percent_encode(
                        &c[5],
                        percent_encoding::NON_ALPHANUMERIC,
                    )
                    .to_string(),
                ))
                .map_err(|_| Error::ImportLine(index + 1))?;
                let authority = url.as_str().trim_end_matches('/');
                let authority = if url.port().is_none() && matches!(url.scheme(), "http" | "https")
                {
                    format!("{authority}:{}", &c[3])
                } else {
                    authority.to_owned()
                };
                (authority, c[6].trim().to_owned())
            } else {
                (line.to_owned(), format!("Proxy {}", index + 1))
            };
            let p = Profile {
                name: "import-check".into(),
                url,
                label: Some(label),
            };
            Connections {
                version: 1,
                mode: Mode::Blocked,
                selected: None,
                profiles: vec![p.clone()],
            }
            .validate()?;
            Ok(p)
        })()
        .map_err(|_| Error::ImportLine(index + 1))?;
        output.push(record);
    }
    if output.is_empty() {
        return Err(Error::Safe("Proxy file contains no records."));
    }
    Ok(output)
}
pub fn import_file(config: &Path, input: &str) -> Result<(usize, usize)> {
    let mut input = input.trim();
    if input.len() >= 2
        && ((input.starts_with('"') && input.ends_with('"'))
            || (input.starts_with('\'') && input.ends_with('\'')))
    {
        input = &input[1..input.len() - 1];
    }
    let data = source_bytes(input)?;
    let text = std::str::from_utf8(&data).map_err(|_| Error::Safe("Proxy file must use UTF-8."))?;
    let parsed = records(text)?;
    let mut imported = 0;
    let mut skipped = 0;
    config::update(config, |c| {
        let mut urls: HashSet<String> = c
            .profiles
            .iter()
            .map(|p| config::proxy_url(&p.url).map(|u| u.to_string()))
            .collect::<Result<_>>()?;
        let mut names: HashSet<String> = c.profiles.iter().map(|p| p.name.clone()).collect();
        let slug = Regex::new(r"[^a-zA-Z0-9_-]+")
            .map_err(|_| Error::Safe("Cannot initialize profile identifiers."))?;
        for mut p in parsed {
            let normalized = config::proxy_url(&p.url)?.to_string();
            if urls.contains(&normalized) {
                skipped += 1;
                continue;
            }
            let label = p.label.as_deref().unwrap_or("proxy");
            let base = slug
                .replace_all(label, "-")
                .trim_matches('-')
                .chars()
                .take(54)
                .collect::<String>();
            let base = if base.is_empty() {
                "proxy".to_owned()
            } else {
                base
            };
            let mut name = base.clone();
            let mut suffix = 2;
            while names.contains(&name) {
                name = format!("{base}-{suffix}");
                suffix += 1;
            }
            p.name = name.clone();
            names.insert(name);
            urls.insert(normalized);
            c.profiles.push(p);
            imported += 1;
        }
        Ok(())
    })?;
    Ok((imported, skipped))
}

#[cfg(test)]
mod bounded_read_tests {
    use super::*;
    use std::io::{self, Cursor};

    #[test]
    fn overflowing_reader_consumes_only_the_limit_plus_one_byte() {
        let mut reader = io::repeat(b'x');
        let mut counted = (&mut reader).take(MAX_IMPORT_BYTES * 4);
        assert!(matches!(
            bounded_bytes(&mut counted),
            Err(Error::Safe("Proxy file exceeds 1 MiB."))
        ));
        assert_eq!(counted.limit(), MAX_IMPORT_BYTES * 3 - 1);
    }

    #[test]
    fn empty_and_exact_limit_readers_are_not_misclassified_as_overflow() {
        assert!(bounded_bytes(io::empty()).unwrap().is_empty());
        let exact = vec![b'x'; MAX_IMPORT_BYTES as usize];
        assert_eq!(bounded_bytes(Cursor::new(&exact)).unwrap(), exact);
    }

    struct InterruptedThenShort {
        interrupted: bool,
        remaining: usize,
    }

    impl Read for InterruptedThenShort {
        fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::ErrorKind::Interrupted.into());
            }
            let count = bytes.len().min(self.remaining).min(7);
            bytes[..count].fill(b'x');
            self.remaining -= count;
            Ok(count)
        }
    }

    #[test]
    fn bounded_reader_handles_interrupted_and_short_reads() {
        let bytes = bounded_bytes(InterruptedThenShort {
            interrupted: false,
            remaining: 101,
        })
        .unwrap();
        assert_eq!(bytes, vec![b'x'; 101]);
    }

    struct Failed;
    impl Read for Failed {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("fixture private input must not escape"))
        }
    }

    #[test]
    fn read_failure_keeps_private_io_details_out_of_the_public_error() {
        let error = bounded_bytes(Failed).unwrap_err().to_string();
        assert_eq!(error, read_error().to_string());
        assert!(!error.contains("fixture private"));
    }
}
