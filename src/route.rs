use crate::{
    config::{self, Connections, Mode},
    error::{Error, Result},
};
use reqwest::{Client, Proxy, redirect::Policy};
use std::{path::PathBuf, time::Duration};

#[derive(Clone)]
pub struct Route {
    path: PathBuf,
    snapshot: String,
    config: Connections,
}
impl Route {
    pub fn new(path: PathBuf) -> Result<Self> {
        let config = config::read(&path)?;
        if config.mode == Mode::Blocked {
            return Err(Error::Safe(
                "Connection blocked. Select a proxy in the menu first.",
            ));
        }
        Ok(Self {
            path,
            snapshot: serde_json::to_string(&config)?,
            config,
        })
    }
    pub fn check(&self) -> Result<()> {
        if serde_json::to_string(&config::read(&self.path)?)? != self.snapshot {
            return Err(Error::Safe(
                "Route changed. Reconnect upwork-local to apply the new selection.",
            ));
        }
        Ok(())
    }
    pub async fn changed(&self) {
        let mut timer = tokio::time::interval(Duration::from_millis(100));
        loop {
            timer.tick().await;
            if self.check().is_err() {
                return;
            }
        }
    }
    pub fn client(&self, follow: bool, timeout: Option<Duration>) -> Result<Client> {
        Ok(self.builder(follow, timeout)?.build()?)
    }
    pub fn builder(
        &self,
        follow: bool,
        timeout: Option<Duration>,
    ) -> Result<reqwest::ClientBuilder> {
        self.check()?;
        let mut builder = Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .redirect(if follow {
                Policy::limited(5)
            } else {
                Policy::none()
            });
        if let Some(timeout) = timeout {
            builder = builder.timeout(timeout);
        }
        if self.config.mode == Mode::Proxy {
            let profile = self
                .config
                .profiles
                .iter()
                .find(|p| Some(&p.name) == self.config.selected.as_ref())
                .ok_or(Error::Safe("Selected proxy missing; networking blocked."))?;
            let mut url = config::proxy_url(&profile.url)?;
            if url.scheme() == "socks5" {
                url.set_scheme("socks5h")
                    .map_err(|_| Error::Safe("Invalid SOCKS route."))?;
            }
            let proxy = Proxy::all(url.as_str())?;
            builder = builder.proxy(proxy);
        }
        Ok(builder)
    }
}
