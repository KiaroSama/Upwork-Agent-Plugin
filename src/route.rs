use crate::{
    config::{self, Selection},
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

#[cfg(test)]
mod validation_cost;

#[derive(Clone)]
pub struct Route {
    path: PathBuf,
    identity: Selection,
    revision: Option<String>,
    revoked: Arc<AtomicBool>,
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
        let unchanged = config::read_snapshot(&self.path).is_ok_and(|snapshot| {
            snapshot.revision == self.revision
                && Selection::from_config(&snapshot.connections)
                    .is_ok_and(|identity| identity == self.identity)
        });
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
        if let Selection::Proxy { url, .. } = &self.identity {
            builder = builder.proxy(Proxy::all(url)?);
        }
        Ok(builder)
    }
}
