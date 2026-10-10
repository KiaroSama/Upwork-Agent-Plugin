use crate::{
    ENDPOINT,
    error::{Error, Result},
    storage::{self, Lock},
};
use async_trait::async_trait;
use md5::{Digest, Md5};
use rmcp::transport::auth::{
    AuthError, CredentialRefreshGuard, CredentialStore, OAuthTokenResponse, StoredCredentials,
};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn path(&self) -> PathBuf {
        self.root.join(".state/rust/credentials.json")
    }
    pub fn legacy_path(&self, kind: &str) -> PathBuf {
        let hash = Md5::digest(ENDPOINT.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        self.root
            .join(".state/auth/mcp-remote-v1")
            .join(format!("{hash}_{kind}.json"))
    }
    pub fn legacy_client(&self) -> Result<Option<Value>> {
        optional_json(&self.legacy_path("client_info"))
    }
    pub fn client_registration(&self) -> Result<Option<Value>> {
        if let Some(v) = optional_json(&self.root.join(".state/rust/client.json"))? {
            return Ok(Some(v));
        }
        self.legacy_client()
    }
    pub fn save_registration(
        &self,
        config: &rmcp::transport::auth::OAuthClientConfig,
    ) -> Result<()> {
        storage::atomic_json(
            &self.root.join(".state/rust/client.json"),
            &serde_json::json!({
                "client_id":config.client_id,"client_secret":config.client_secret,
                "redirect_uris":[config.redirect_uri],"scopes":config.scopes,
                "application_type":config.application_type
            }),
        )
    }
    pub fn available(&self) -> Result<bool> {
        Ok(self.path().exists() || self.legacy_path("tokens").exists())
    }
    pub fn load_local(&self) -> Result<Option<StoredCredentials>> {
        if let Some(v) = optional_json(&self.path())? {
            return Ok(Some(serde_json::from_value(v)?));
        }
        let Some(mut token) = optional_json(&self.legacy_path("tokens"))? else {
            return Ok(None);
        };
        let Some(client) = self.legacy_client()? else {
            return Ok(None);
        };
        let client_id = client
            .get("client_id")
            .and_then(Value::as_str)
            .ok_or(Error::Safe("Saved client registration invalid."))?
            .to_owned();
        let expires = token
            .get("expires_at")
            .and_then(Value::as_u64)
            .map(|n| n / 1000);
        let lifetime = token.get("expires_in").and_then(Value::as_u64);
        let received = expires
            .zip(lifetime)
            .map(|(expiry, life)| expiry.saturating_sub(life));
        if let Some(expiry) = expires {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|_| Error::Safe("System clock invalid."))?
                .as_secs();
            if lifetime.is_none() {
                token["expires_in"] = serde_json::json!(expiry.saturating_sub(now));
            }
        }
        let token_response: OAuthTokenResponse = serde_json::from_value(token.clone())?;
        let scopes = token
            .get("requested_scope")
            .and_then(Value::as_str)
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        Ok(Some(
            StoredCredentials::new(
                client_id,
                Some(token_response),
                scopes,
                received.or_else(|| {
                    expires.map(|_| {
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    })
                }),
            )
            .with_issuer(
                token
                    .get("issuer")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            ),
        ))
    }
}
pub fn optional_json(path: &Path) -> Result<Option<Value>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(serde_json::from_str(&text)?)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn auth_error() -> AuthError {
    AuthError::CredentialStoreError(
        "Local credential storage failed; private details suppressed.".into(),
    )
}
#[async_trait]
impl CredentialStore for Store {
    async fn load(&self) -> std::result::Result<Option<StoredCredentials>, AuthError> {
        self.load_local().map_err(|_| auth_error())
    }
    async fn save(&self, value: StoredCredentials) -> std::result::Result<(), AuthError> {
        storage::atomic_json(&self.path(), &value).map_err(|_| auth_error())
    }
    async fn clear(&self) -> std::result::Result<(), AuthError> {
        // Preserve user credentials on an issuer mismatch; manual reauthorization writes a new state only.
        Err(AuthError::CredentialStoreError(
            "Stored issuer changed. Run explicit sign-in; original session preserved.".into(),
        ))
    }
    async fn acquire_refresh_guard(
        &self,
    ) -> std::result::Result<Option<CredentialRefreshGuard>, AuthError> {
        let lock =
            Lock::acquire(&self.root.join(".state/rust/refresh.json")).map_err(|_| auth_error())?;
        Ok(Some(CredentialRefreshGuard::new(lock)))
    }
}
