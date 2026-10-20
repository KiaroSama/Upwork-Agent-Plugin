use crate::{
    config::{self, Selection},
    error::{Error, Result},
};
use reqwest::{Client, Proxy, redirect::Policy};
use std::{
    future::Future,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(test)]
mod validation_cost;

#[derive(Clone)]
pub struct Route {
    path: PathBuf,
    identity: Selection,
    revision: Option<String>,
    revoked: Arc<AtomicBool>,
    validated_bytes: Arc<Mutex<Option<Vec<u8>>>>,
    #[cfg(test)]
    snapshot_parses: Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(test)]
    snapshot_reads: Arc<std::sync::atomic::AtomicUsize>,
}
impl Route {
    pub fn new(path: PathBuf) -> Result<Self> {
        let snapshot = config::read_snapshot(&path)?;
        let identity = Selection::from_config(&snapshot.connections)?;
        if matches!(identity, Selection::Blocked) {
            return Err(Error::Safe(
                "Connection blocked. Select a proxy in the menu first.",
            ));
        }
        Ok(Self {
            path,
            identity,
            revision: snapshot.revision,
            revoked: Arc::new(AtomicBool::new(false)),
            validated_bytes: Arc::new(Mutex::new(None)),
            #[cfg(test)]
            snapshot_parses: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
            #[cfg(test)]
            snapshot_reads: Arc::new(std::sync::atomic::AtomicUsize::new(0)),
        })
    }
    fn changed_error() -> Error {
        Error::Safe("Route changed. Reconnect Upwork-Agent-mcp to apply the new selection.")
    }
    pub fn check(&self) -> Result<()> {
        if self.revoked.load(Ordering::Acquire) {
            return Err(Self::changed_error());
        }
        // The durable revision records changes even when both writes happen
        // between watcher ticks. Inactive edits retain it and do not disconnect.
        #[cfg(test)]
        self.snapshot_reads.fetch_add(1, Ordering::Relaxed);
        let unchanged = self.validate_current_bytes().is_ok();
        if unchanged && !self.revoked.load(Ordering::Acquire) {
            Ok(())
        } else {
            // All clones remain revoked even when the old configuration is restored later.
            self.revoked.store(true, Ordering::Release);
            Err(Self::changed_error())
        }
    }
    fn validate_current_bytes(&self) -> Result<()> {
        // Read and validate the opened file every time. Only identical bytes can
        // reuse a prior parse; timestamps and writer cooperation are irrelevant.
        let bytes =
            crate::storage::optional_bytes(&self.path, false)?.ok_or_else(Self::changed_error)?;
        let mut validated = self
            .validated_bytes
            .lock()
            .map_err(|_| Self::changed_error())?;
        if validated.as_deref() == Some(bytes.as_slice()) {
            return Ok(());
        }
        #[cfg(test)]
        self.snapshot_parses.fetch_add(1, Ordering::Relaxed);
        let snapshot = config::parse_snapshot(&bytes)?;
        if snapshot.revision != self.revision
            || Selection::from_config(&snapshot.connections)? != self.identity
        {
            return Err(Self::changed_error());
        }
        *validated = Some(bytes);
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
        if let Selection::Proxy { url, .. } = &self.identity {
            builder = builder.proxy(Proxy::all(url)?);
        }
        Ok(builder)
    }
}
