use crate::{
    error::{Error, Result},
    storage,
};
use serde_json::{Value, json};
use std::{env, fs, path::Path};

fn same_path(left: &Path, right: &Path) -> bool {
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(l), Ok(r)) => l == r,
        _ => false,
    }
}
pub fn prepare_cache(root: &Path) -> Result<()> {
    let cache = root.join(".cache");
    let temp = cache.join("tmp");
    for path in [&cache, &temp] {
        if let Ok(meta) = std::fs::symlink_metadata(path)
            && (crate::cleanup::linked(&meta) || !meta.is_dir())
        {
            return Err(Error::Safe("Linked or invalid cache directory refused."));
        }
    }
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
    let args = if executable.parent().is_some_and(|parent| {
        parent.file_name().is_some_and(|name| name == "bin") && same_path(parent, &root.join("bin"))
    }) {
        json!(["bridge"])
    } else {
        json!(["--root", root, "bridge"])
    };
    let temp = root.join(".cache/tmp");
    let cache = root.join(".cache");
    Ok(json!({"mcpServers":{(crate::MCP_NAME):{
        "command":executable,"args":args,
        "env":{"TEMP":temp,"TMP":temp,"TMPDIR":temp,"XDG_CACHE_HOME":cache}
    }}}))
}
fn check_destination(path: &Path, directory: bool) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta)
            if crate::cleanup::linked(&meta)
                || (directory && !meta.is_dir())
                || (!directory && !meta.is_file()) =>
        {
            Err(Error::Safe(
                "Linked or invalid registration destination refused.",
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn same_or_exact(left: &Path, right: &Path) -> bool {
    left == right || same_path(left, right)
}
fn owned_registration(root: &Path, old: &Value, entry: &Value) -> bool {
    if old.get("type").is_some_and(|kind| kind != "stdio") {
        return false;
    }
    let Some(command) = old.get("command").and_then(Value::as_str) else {
        return false;
    };
    let Some(args) = old.get("args").and_then(Value::as_array) else {
        return false;
    };
    let command = Path::new(command);
    let native = [
        root.join("bin/upwork-connector"),
        root.join("bin/upwork-connector.exe"),
    ];
    let installed = native.iter().any(|path| same_or_exact(command, path));
    let current = entry
        .get("command")
        .and_then(Value::as_str)
        .is_some_and(|path| same_or_exact(command, Path::new(path)));
    if installed || current {
        return (installed && args.as_slice() == [json!("bridge")])
            || (args.len() == 3
                && args[0] == "--root"
                && args[2] == "bridge"
                && args[1]
                    .as_str()
                    .is_some_and(|path| same_path(Path::new(path), root)));
    }
    // An exact legacy path is still recognizable after the TypeScript source was retired.
    let node = command
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("node") || name.eq_ignore_ascii_case("node.exe")
        });
    node && args.len() == 1
        && args[0]
            .as_str()
            .is_some_and(|path| same_or_exact(Path::new(path), &root.join("src/bridge.ts")))
}
fn snapshot(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
fn object(bytes: &Option<Vec<u8>>) -> Result<Value> {
    let value: Value = match bytes {
        Some(bytes) => serde_json::from_slice(bytes)?,
        None => json!({}),
    };
    if !value.is_object() {
        return Err(Error::Safe(
            "Existing registration configuration must be a JSON object.",
        ));
    }
    Ok(value)
}
pub fn register(root: &Path) -> Result<()> {
    let root = fs::canonicalize(root)?;
    let mcp_path = root.join(".mcp.json");
    let settings_dir = root.join(".claude");
    let settings_path = settings_dir.join("settings.local.json");
    check_destination(&mcp_path, false)?;
    check_destination(&settings_dir, true)?;
    check_destination(&settings_path, false)?;
    // Always acquire in this order, before reading either file.
    let _mcp_lock = storage::Lock::acquire(&mcp_path)?;
    let _settings_lock = storage::Lock::acquire(&settings_path)?;
    let original_mcp = snapshot(&mcp_path)?;
    let original_settings = snapshot(&settings_path)?;
    let mut mcp = object(&original_mcp)?;
    let mut settings = object(&original_settings)?;
    if mcp
        .get("mcpServers")
        .is_some_and(|servers| !servers.is_object())
    {
        return Err(Error::Safe("Invalid existing MCP configuration."));
    }
    let mut entry = desktop(&root)?["mcpServers"][crate::MCP_NAME].clone();
    for name in [crate::MCP_NAME, "upwork-local"] {
        if let Some(old) = mcp["mcpServers"].get(name)
            && !owned_registration(&root, old, &entry)
        {
            return Err(Error::Safe(
                "Unrelated Upwork MCP registration exists; replacement refused.",
            ));
        }
    }
    if mcp.get("mcpServers").is_none() {
        mcp["mcpServers"] = json!({});
    }
    entry["type"] = json!("stdio");
    mcp["mcpServers"]
        .as_object_mut()
        .expect("Validated MCP object")
        .remove("upwork-local");
    mcp["mcpServers"][crate::MCP_NAME] = entry;
    if settings.get("deniedMcpServers").is_none() {
        settings["deniedMcpServers"] = json!([]);
    }
    let deny = settings["deniedMcpServers"]
        .as_array_mut()
        .ok_or(Error::Safe("Invalid existing MCP deny configuration."))?;
    let denied = json!({"serverUrl":crate::ENDPOINT});
    if !deny.contains(&denied) {
        deny.push(denied);
    }
    check_destination(&mcp_path, false)?;
    check_destination(&settings_dir, true)?;
    check_destination(&settings_path, false)?;
    if snapshot(&mcp_path)? != original_mcp || snapshot(&settings_path)? != original_settings {
        return Err(Error::Safe(
            "Registration configuration changed concurrently; retry after reviewing it.",
        ));
    }
    publish_pair(&settings_path, &settings, &original_settings, || {
        if snapshot(&mcp_path)? != original_mcp {
            return Err(Error::Safe(
                "MCP configuration changed concurrently; replacement refused.",
            ));
        }
        storage::atomic_json(&mcp_path, &mcp)
    })
}

fn publish_pair(
    settings_path: &Path,
    settings: &Value,
    original_settings: &Option<Vec<u8>>,
    publish_mcp: impl FnOnce() -> Result<()>,
) -> Result<()> {
    // Publish the direct-route denial first. A crash cannot install the connector before its denial.
    let mut published = serde_json::to_vec_pretty(settings)?;
    published.push(b'\n');
    storage::atomic_bytes(settings_path, &published)?;
    if let Err(error) = publish_mcp() {
        // Equivalent JSON can still be an intervening edit; compare our exact bytes.
        let current = snapshot(settings_path)?;
        if current.as_deref() != Some(published.as_slice()) {
            return Err(Error::Safe(
                "MCP registration failed and settings changed concurrently; review both files.",
            ));
        }
        let rollback = match original_settings {
            Some(bytes) => storage::atomic_bytes(settings_path, bytes),
            None => fs::remove_file(settings_path).map_err(Error::from),
        };
        if rollback.is_err() {
            return Err(Error::Safe(
                "MCP registration failed and settings restoration failed; review both files.",
            ));
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_publication_failure_restores_exact_original_bytes_or_absence() {
        fs::create_dir_all(".ci-work").unwrap();
        for original in [None, Some(b"{ \"preserved\": true }\n".to_vec())] {
            let dir = tempfile::tempdir_in(".ci-work").unwrap();
            let path = dir.path().join("settings.json");
            if let Some(bytes) = &original {
                fs::write(&path, bytes).unwrap();
            }
            let published = json!({"deniedMcpServers":[{"serverUrl":crate::ENDPOINT}]});
            let result = publish_pair(&path, &published, &original, || {
                assert_eq!(object(&snapshot(&path).unwrap()).unwrap(), published);
                Err(Error::Safe("Injected second-file publication failure."))
            });
            assert!(result.is_err());
            assert_eq!(snapshot(&path).unwrap(), original);
        }
    }

    #[test]
    fn rollback_preserves_a_concurrent_equivalent_json_edit() {
        fs::create_dir_all(".ci-work").unwrap();
        let dir = tempfile::tempdir_in(".ci-work").unwrap();
        let path = dir.path().join("settings.json");
        let concurrent = b"{ \"deniedMcpServers\": [] }\n";
        let result = publish_pair(&path, &json!({"deniedMcpServers":[]}), &None, || {
            fs::write(&path, concurrent).unwrap();
            Err(Error::Safe(
                "Injected failure after an equivalent settings edit.",
            ))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(path).unwrap(), concurrent);
    }

    #[test]
    fn rollback_never_overwrites_a_concurrent_settings_edit() {
        fs::create_dir_all(".ci-work").unwrap();
        let dir = tempfile::tempdir_in(".ci-work").unwrap();
        let path = dir.path().join("settings.json");
        let concurrent = b"{\"concurrent\":true}";
        let result = publish_pair(&path, &json!({"deniedMcpServers":[]}), &None, || {
            fs::write(&path, concurrent).unwrap();
            Err(Error::Safe(
                "Injected publication failure after another writer.",
            ))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(path).unwrap(), concurrent);
    }
}
