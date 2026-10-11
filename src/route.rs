use crate::{
    config::{self, Connections, Mode},
    error::{Error, Result},
};
use reqwest::{Client, Proxy, redirect::Policy};
use std::{
    future::Future,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

// Never derive Debug: a proxy URL can contain credentials.
#[derive(Clone, PartialEq, Eq)]
enum Identity {
    Direct,
    Proxy { name: String, url: String },
}
impl Identity {
    fn from_config(config: &Connections) -> Result<Self> {
        match config.mode {
            Mode::Blocked => Err(Error::Safe(
                "Connection blocked. Select a proxy in the menu first.",
            )),
            Mode::Direct => Ok(Self::Direct),
            Mode::Proxy => {
                let profile = config
                    .profiles
                    .iter()
                    .find(|p| Some(&p.name) == config.selected.as_ref())
                    .ok_or(Error::Safe("Selected proxy missing; networking blocked."))?;
                let mut url = config::proxy_url(&profile.url)?;
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

#[derive(Clone)]
pub struct Route {
    path: PathBuf,
    identity: Identity,
    revoked: Arc<AtomicBool>,
}
impl Route {
    pub fn new(path: PathBuf) -> Result<Self> {
        let config = config::read(&path)?;
        Ok(Self {
            path,
            identity: Identity::from_config(&config)?,
            revoked: Arc::new(AtomicBool::new(false)),
        })
    }
    fn changed_error() -> Error {
        Error::Safe("Route changed. Reconnect upwork-local to apply the new selection.")
    }
    pub fn check(&self) -> Result<()> {
        if self.revoked.load(Ordering::Acquire) {
            return Err(Self::changed_error());
        }
        // Validate the complete file, but compare only fields that select the network route.
        let unchanged = config::read(&self.path)
            .and_then(|config| Identity::from_config(&config))
            .is_ok_and(|identity| identity == self.identity);
        if unchanged && !self.revoked.load(Ordering::Acquire) {
            Ok(())
        } else {
            // All clones remain revoked even when the old configuration is restored later.
            self.revoked.store(true, Ordering::Release);
            Err(Self::changed_error())
        }
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
    pub async fn guard<F: Future>(&self, operation: F) -> Result<F::Output> {
        self.check()?;
        let result = tokio::select! {
            biased;
            _ = self.changed() => return Err(Self::changed_error()),
            result = operation => result,
        };
        self.check()?;
        Ok(result)
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
        if let Identity::Proxy { url, .. } = &self.identity {
            builder = builder.proxy(Proxy::all(url)?);
        }
        Ok(builder)
    }
}
