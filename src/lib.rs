pub mod auth;
pub mod bridge;
pub mod callback;
pub mod cleanup;
pub mod config;
pub mod credentials;
pub mod diagnostics;
pub mod error;
pub mod http_route;
pub mod import;
pub mod logging;
pub mod menu;
pub mod register;
pub mod route;
pub mod settings;
pub mod storage;

pub const MCP_NAME: &str = "Upwork-Agent-mcp";
pub const ENDPOINT: &str = "https://mcp.upwork.com/mcp";
