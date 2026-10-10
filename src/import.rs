use crate::{
    config::{self, Connections, Mode, Profile},
    error::{Error, Result},
};
use regex::Regex;
use std::{collections::HashSet, fs, path::Path};
use url::Url;

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
    let data = fs::read(input).map_err(|_| {
        Error::Safe("Cannot read proxy file. Check the path, permissions and UTF-8 encoding.")
    })?;
    if data.len() > 1024 * 1024 {
        return Err(Error::Safe("Proxy file exceeds 1 MiB."));
    }
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
