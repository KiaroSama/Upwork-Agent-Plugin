use crate::{
    error::{Error, Result},
    storage::{Lock, atomic_json},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};
use url::Url;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Blocked,
    Direct,
    Proxy,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub name: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Connections {
    pub version: u8,
    pub mode: Mode,
    pub selected: Option<String>,
    pub profiles: Vec<Profile>,
}
impl Default for Connections {
    fn default() -> Self {
        Self {
            version: 1,
            mode: Mode::Blocked,
            selected: None,
            profiles: vec![],
        }
    }
}
impl Connections {
    pub fn validate(&self) -> Result<()> {
        if self.version != 1 {
            return Err(Error::Safe(
                "Invalid connection version; networking blocked.",
            ));
        }
        let mut names = HashSet::new();
        for p in &self.profiles {
            if p.name.is_empty()
                || p.name.len() > 64
                || !p
                    .name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                || !names.insert(&p.name)
            {
                return Err(Error::Safe(
                    "Proxy names must be unique identifiers (1-64 letters, digits, _ or -).",
                ));
            }
            if p.label.as_ref().is_some_and(|s| {
                s.trim().is_empty()
                    || s.encode_utf16().count() > 128
                    || s.chars().any(|c| c.is_ascii_control())
            }) {
                return Err(Error::Safe(
                    "Proxy display names must contain 1-128 printable characters.",
                ));
            }
            proxy_url(&p.url)?;
        }
        if self.mode == Mode::Proxy
            && !self
                .profiles
                .iter()
                .any(|p| Some(&p.name) == self.selected.as_ref())
        {
            return Err(Error::Safe(
                "Selected proxy does not exist; networking blocked.",
            ));
        }
        Ok(())
    }
    pub fn status(&self) -> Result<serde_json::Value> {
        let mut profiles = vec![];
        for p in &self.profiles {
            let url = public_proxy_url(&p.url)?;
            profiles.push(serde_json::json!({"name":p.name,"label":p.label.as_ref().unwrap_or(&p.name),"url":url}));
        }
        Ok(serde_json::json!({"mode":self.mode,"selected":self.selected,"profiles":profiles}))
    }
}
/// Remove credentials while retaining an explicit port accepted by the input validator.
pub fn public_proxy_url(raw: &str) -> Result<String> {
    let mut url = proxy_url(raw)?;
    let port = url
        .port_or_known_default()
        .ok_or(Error::Safe("Proxy port is missing."))?;
    url.set_username("")
        .map_err(|_| Error::Safe("Cannot sanitize proxy URL."))?;
    url.set_password(None)
        .map_err(|_| Error::Safe("Cannot sanitize proxy URL."))?;
    if url.port().is_some() {
        return Ok(url.into());
    }
    // URL serialization omits HTTP(S) default ports. Restore them without
    // rebuilding the authority, so IPv6 brackets and encoded hosts remain intact.
    let authority = &url[..url::Position::AfterHost];
    let tail = &url[url::Position::AfterHost..];
    Ok(format!("{authority}:{port}{tail}"))
}

pub fn proxy_url(raw: &str) -> Result<Url> {
    let url = Url::parse(raw).map_err(|_| Error::Safe("Invalid proxy URL."))?;
    let tail = raw.trim_end_matches('/').rsplit_once(':').map(|(_, p)| p);
    let explicit = tail.and_then(|s| s.parse::<u16>().ok()).filter(|p| *p > 0);
    if !matches!(url.scheme(), "http" | "https" | "socks5" | "socks5h")
        || url.host_str().is_none()
        || explicit.is_none()
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        return Err(Error::Safe(
            "Proxy URL requires HTTP, HTTPS or SOCKS5, a host and port, without path/query/fragment.",
        ));
    }
    for field in [url.username(), url.password().unwrap_or("")] {
        let bytes = field.as_bytes();
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'%'
                && (i + 2 >= bytes.len()
                    || !bytes[i + 1].is_ascii_hexdigit()
                    || !bytes[i + 2].is_ascii_hexdigit())
            {
                return Err(Error::Safe("Invalid proxy credential encoding."));
            }
        }
        percent_encoding::percent_decode_str(field)
            .decode_utf8()
            .map_err(|_| Error::Safe("Invalid proxy credential encoding."))?;
    }
    Ok(url)
}
pub fn read(path: &Path) -> Result<Connections> {
    let Some(bytes) = crate::storage::read_optional(path)? else {
        return Ok(Connections::default());
    };
    let c: Connections = serde_json::from_slice(&bytes)?;
    c.validate()?;
    Ok(c)
}
pub fn update(
    path: &Path,
    change: impl FnOnce(&mut Connections) -> Result<()>,
) -> Result<Connections> {
    let _lock = Lock::acquire(path)?;
    let mut c = read(path)?;
    change(&mut c)?;
    c.validate()?;
    atomic_json(path, &c)?;
    Ok(c)
}
