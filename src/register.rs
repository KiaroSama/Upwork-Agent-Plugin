use crate::{
    credentials::optional_json,
    error::{Error, Result},
    storage,
};
use serde_json::{Value, json};
use std::{env, path::Path};

fn same_path(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(l), Ok(r)) => l == r,
        _ => false,
    }
}
pub fn prepare_cache(root: &Path) -> Result<()> {
    let cache = root.join(".cache");
    let temp = cache.join("tmp");
    std::fs::create_dir_all(&temp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        for path in [&cache, &temp] {
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    Ok(())
}
pub fn desktop(root: &Path) -> Result<Value> {
    let executable = env::current_exe()?;
    let temp = root.join(".cache/tmp");
    let cache = root.join(".cache");
    Ok(json!({"mcpServers":{"upwork-local":{
        "command":executable,"args":["--root",root,"bridge"],
        "env":{"TEMP":temp,"TMP":temp,"TMPDIR":temp,"XDG_CACHE_HOME":cache}
    }}}))
}
pub fn register(root: &Path) -> Result<()> {
    let mcp_path = root.join(".mcp.json");
    let settings_path = root.join(".claude/settings.local.json");
    let mut mcp = optional_json(&mcp_path)?.unwrap_or(json!({}));
    let mut settings = optional_json(&settings_path)?.unwrap_or(json!({}));
    let entry = desktop(root)?["mcpServers"]["upwork-local"].clone();
    let old = &mcp["mcpServers"]["upwork-local"];
    if !old.is_null() {
        let legacy = root.join("src").join("bridge.ts");
        let owned = old["args"][0]
            .as_str()
            .is_some_and(|p| same_path(Path::new(p), &legacy))
            || old["command"] == entry["command"]
            || old["command"]
                .as_str()
                .is_some_and(|p| Path::new(p) == root.join("bin/upwork-connector.exe"));
        if !owned {
            return Err(Error::Safe(
                "Unrelated upwork-local registration exists; replacement refused.",
            ));
        }
    }
    if mcp.get("mcpServers").is_none() {
        mcp["mcpServers"] = json!({});
    }
    if !mcp["mcpServers"].is_object() {
        return Err(Error::Safe("Invalid existing MCP configuration."));
    }
    let mut entry = entry;
    entry["type"] = json!("stdio");
    mcp["mcpServers"]["upwork-local"] = entry;
    if settings.get("deniedMcpServers").is_none() {
        settings["deniedMcpServers"] = json!([]);
    }
    let deny = settings["deniedMcpServers"]
        .as_array_mut()
        .ok_or(Error::Safe("Invalid existing MCP deny configuration."))?;
    let entry = json!({"serverUrl":crate::ENDPOINT});
    if !deny.contains(&entry) {
        deny.push(entry);
    }
    storage::atomic_json(&mcp_path, &mcp)?;
    storage::atomic_json(&settings_path, &settings)?;
    Ok(())
}
