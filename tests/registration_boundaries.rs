use serde_json::{Value, json};
use std::{fs, path::Path};
use upwork_connector::register;

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("registration boundaries ")
        .tempdir_in(".ci-work")
        .unwrap()
}

fn write_json(path: &Path, value: &Value) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

#[test]
fn registration_refuses_another_root_using_the_same_executable() {
    let dir = temp();
    let other = temp();
    let root = fs::canonicalize(dir.path()).unwrap();
    let foreign = fs::canonicalize(other.path()).unwrap();
    let path = root.join(".mcp.json");
    let original = json!({"mcpServers":{"upwork-local":{
        "command":std::env::current_exe().unwrap(),
        "args":["--root",foreign,"bridge"]
    },"other":{"command":"preserve-me"}}});
    write_json(&path, &original);
    let before = fs::read(&path).unwrap();
    assert!(register::register(&root).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!root.join(".claude/settings.local.json").exists());
}

#[test]
fn registration_rejects_non_object_roots_without_panicking_or_writing() {
    for name in [".mcp.json", ".claude/settings.local.json"] {
        for value in [
            json!([]),
            json!(false),
            json!(42),
            json!("private"),
            Value::Null,
        ] {
            let dir = temp();
            let root = fs::canonicalize(dir.path()).unwrap();
            let path = root.join(name);
            write_json(&path, &value);
            let before = fs::read(&path).unwrap();
            let result = std::panic::catch_unwind(|| register::register(&root));
            assert!(result.is_ok(), "Malformed configuration must not panic");
            assert!(result.unwrap().is_err());
            assert_eq!(fs::read(&path).unwrap(), before);
        }
    }
}

#[test]
fn registration_refuses_invalid_shapes_without_mutating_either_file() {
    for (mcp, settings) in [
        (json!({"mcpServers":[]}), json!({})),
        (json!({}), json!({"deniedMcpServers":{}})),
    ] {
        let dir = temp();
        let root = fs::canonicalize(dir.path()).unwrap();
        let mcp_path = root.join(".mcp.json");
        let settings_path = root.join(".claude/settings.local.json");
        write_json(&mcp_path, &mcp);
        write_json(&settings_path, &settings);
        let before_mcp = fs::read(&mcp_path).unwrap();
        let before_settings = fs::read(&settings_path).unwrap();
        assert!(register::register(&root).is_err());
        assert_eq!(fs::read(&mcp_path).unwrap(), before_mcp);
        assert_eq!(fs::read(&settings_path).unwrap(), before_settings);
    }
}

#[test]
fn registration_of_own_root_is_idempotent_and_preserves_other_settings() {
    let dir = temp();
    let root = fs::canonicalize(dir.path()).unwrap();
    write_json(
        &root.join(".mcp.json"),
        &json!({"mcpServers":{"other":{"command":"keep"}}}),
    );
    write_json(
        &root.join(".claude/settings.local.json"),
        &json!({"enabledPlugins":{"keep":true}}),
    );
    register::register(&root).unwrap();
    let mcp = fs::read(root.join(".mcp.json")).unwrap();
    let settings = fs::read(root.join(".claude/settings.local.json")).unwrap();
    register::register(&root).unwrap();
    assert_eq!(fs::read(root.join(".mcp.json")).unwrap(), mcp);
    assert_eq!(
        fs::read(root.join(".claude/settings.local.json")).unwrap(),
        settings
    );
    let mcp: Value = serde_json::from_slice(&mcp).unwrap();
    let settings: Value = serde_json::from_slice(&settings).unwrap();
    assert_eq!(mcp["mcpServers"]["other"]["command"], "keep");
    assert_eq!(settings["enabledPlugins"]["keep"], true);
}

#[cfg(unix)]
#[test]
fn registration_refuses_linked_settings_directory_without_external_writes() {
    let dir = temp();
    let outside = temp();
    let root = fs::canonicalize(dir.path()).unwrap();
    let external = fs::canonicalize(outside.path()).unwrap();
    write_json(
        &root.join(".mcp.json"),
        &json!({"mcpServers":{"other":{"command":"keep"}}}),
    );
    let before = fs::read(root.join(".mcp.json")).unwrap();
    std::os::unix::fs::symlink(&external, root.join(".claude")).unwrap();
    assert!(register::register(&root).is_err());
    assert_eq!(fs::read(root.join(".mcp.json")).unwrap(), before);
    assert_eq!(fs::read_dir(external).unwrap().count(), 0);
}
