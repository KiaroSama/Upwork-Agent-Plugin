use super::{Connections, Mode, proxy_url};
use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// This value can contain proxy credentials. Deliberately do not implement Debug.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Selection {
    Blocked,
    Direct,
    Proxy { name: String, url: String },
}
impl Selection {
    pub(crate) fn from_config(config: &Connections) -> Result<Self> {
        match config.mode {
            Mode::Blocked => Ok(Self::Blocked),
            Mode::Direct => Ok(Self::Direct),
            Mode::Proxy => {
                let profile = config
                    .profiles
                    .iter()
                    .find(|profile| Some(&profile.name) == config.selected.as_ref())
                    .ok_or(Error::Safe("Selected proxy missing; networking blocked."))?;
                let mut url = proxy_url(&profile.url)?;
                if url.scheme() == "socks5" {
                    url.set_scheme("socks5h")
                        .map_err(|_| Error::Safe("Invalid SOCKS route."))?;
                }
                Ok(Self::Proxy {
                    name: profile.name.clone(),
                    url: url.into(),
                })
            }
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Snapshot {
    #[serde(flatten)]
    pub(crate) connections: Connections,
    #[serde(
        default,
        rename = "_route_revision",
        skip_serializing_if = "Option::is_none"
    )]
    pub(crate) revision: Option<String>,
}

pub(crate) fn read_snapshot(path: &Path) -> Result<Snapshot> {
    let Some(bytes) = crate::storage::optional_bytes(path, false)? else {
        return Ok(Snapshot::default());
    };
    parse_snapshot(&bytes)
}

pub(crate) fn parse_snapshot(bytes: &[u8]) -> Result<Snapshot> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| Error::Safe("Connection state must be valid UTF-8; networking blocked."))?;
    let snapshot: Snapshot = serde_json::from_str(text)?;
    snapshot.connections.validate()?;
    if snapshot
        .revision
        .as_ref()
        .is_some_and(|revision| uuid::Uuid::parse_str(revision).is_err())
    {
        return Err(Error::Safe("Invalid route revision; networking blocked."));
    }
    Ok(snapshot)
}
