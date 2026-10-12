use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Safe(&'static str),
    #[error(
        "MCP transport failed class={0}. Reconnect Upwork-Agent-mcp; no replay or direct fallback."
    )]
    Transport(&'static str),
    #[error("Invalid proxy record at line {0}. No profiles imported.")]
    ImportLine(usize),
    #[error("File operation failed. Check configuration permissions and available disk space.")]
    Io(#[from] std::io::Error),
    #[error("Invalid JSON configuration or protocol message.")]
    Json(#[from] serde_json::Error),
    #[error("Network operation failed through the selected route. No direct fallback enabled.")]
    Network(#[from] reqwest::Error),
    #[error("OAuth failed. Check the browser callback, selected proxy and sign-in log.")]
    Auth(#[from] rmcp::transport::auth::AuthError),
}
pub type Result<T> = std::result::Result<T, Error>;
