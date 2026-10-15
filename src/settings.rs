use crate::{
    config::{self, Mode, Profile},
    error::{Error, Result},
    storage::{Lock, atomic_json},
};
use serde::{Deserialize, Serialize};
use std::path::Path;

const LOCAL_NAME: &str = "local-proxy";
const LOCAL_LABEL: &str = "Local proxy (settings)";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProxy {
    pub url: String,
}
impl Default for LocalProxy {
    fn default() -> Self {
        Self {
            url: "socks5h://127.0.0.1:1595".into(),
        }
    }
}
impl LocalProxy {
    pub fn validate(&self) -> Result<()> {
        config::proxy_url(&self.url)?;
        Ok(())
    }
    pub fn public_url(&self) -> Result<String> {
        config::public_proxy_url(&self.url)
    }
}
pub fn read(root: &Path) -> Result<LocalProxy> {
    let Some(bytes) = crate::storage::read_optional(&root.join("config/local-proxy.json"))? else {
        return Ok(LocalProxy::default());
    };
    let value: LocalProxy = serde_json::from_slice(&bytes)?;
    value.validate()?;
    Ok(value)
}
pub fn save(root: &Path, value: LocalProxy) -> Result<()> {
    value.validate()?;
    let path = root.join("config/local-proxy.json");
    let _lock = Lock::acquire(&path)?;
    atomic_json(&path, &value)
}
pub fn select(root: &Path) -> Result<()> {
    let local = read(root)?;
    config::update(&root.join("config/connections.json"), |c| {
        if let Some(profile) = c.profiles.iter_mut().find(|p| p.name == LOCAL_NAME) {
            if profile.label.as_deref() != Some(LOCAL_LABEL) {
                return Err(Error::Safe(
                    "Profile local-proxy already exists; preserve it and rename it before selecting local settings.",
                ));
            }
            profile.url = local.url;
        } else {
            c.profiles.push(Profile {
                name: LOCAL_NAME.into(),
                label: Some(LOCAL_LABEL.into()),
                url: local.url,
            });
        }
        c.mode = Mode::Proxy;
        c.selected = Some(LOCAL_NAME.into());
        Ok(())
    })?;
    Ok(())
}
