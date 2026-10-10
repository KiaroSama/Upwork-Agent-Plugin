use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("rust cli ")
        .tempdir_in(".ci-work")
        .unwrap()
}
fn command(root: &std::path::Path, args: &[&str], input: &str) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_upwork-connector"));
    c.arg("--root")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    let mut child = c.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(4) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("CLI timeout");
        }
        // Bounded polling: std::process has no timed wait; leave CPU headroom.
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}
#[test]
fn clean_checkout_cli_blocked_invalid_and_no_token() {
    let dir = temp();
    let out = command(dir.path(), &["status"], "");
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap(),
        json!({"mode":"blocked","selected":null,"profiles":[]})
    );
    let blocked = command(dir.path(), &["bridge"], "");
    assert_eq!(blocked.status.code(), Some(1));
    assert!(blocked.stdout.is_empty());
    assert!(String::from_utf8_lossy(&blocked.stderr).contains("Connection blocked"));
    assert!(command(dir.path(), &["direct"], "").status.success());
    let out = command(dir.path(), &["bridge"], "");
    assert_eq!(out.status.code(), Some(3));
    assert!(out.stdout.is_empty());
    let before = fs::read(dir.path().join("config/connections.json")).unwrap();
    assert!(
        !command(dir.path(), &["select", "missing"], "")
            .status
            .success()
    );
    assert_eq!(
        fs::read(dir.path().join("config/connections.json")).unwrap(),
        before
    );
}
#[test]
fn desktop_config_and_registration_preserve_other_servers_and_skills() {
    let dir = temp();
    fs::create_dir(dir.path().join(".claude")).unwrap();
    fs::write(
        dir.path().join(".claude/settings.local.json"),
        r#"{"enabledPlugins":{"example":true}}"#,
    )
    .unwrap();
    fs::write(
        dir.path().join(".mcp.json"),
        r#"{"mcpServers":{"other":{"command":"example"}}}"#,
    )
    .unwrap();
    for _ in 0..2 {
        assert!(command(dir.path(), &["register"], "").status.success());
    }
    let settings: Value =
        serde_json::from_slice(&fs::read(dir.path().join(".claude/settings.local.json")).unwrap())
            .unwrap();
    assert_eq!(settings["enabledPlugins"], json!({"example":true}));
    assert_eq!(settings["deniedMcpServers"].as_array().unwrap().len(), 1);
    let mcp: Value =
        serde_json::from_slice(&fs::read(dir.path().join(".mcp.json")).unwrap()).unwrap();
    assert_eq!(mcp["mcpServers"]["other"]["command"], "example");
    let out = command(dir.path(), &["desktop-config"], "");
    assert!(out.status.success());
    let config: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(config["mcpServers"]["upwork-local"]["args"][2], "bridge");
    assert!(
        config["mcpServers"]["upwork-local"]["command"]
            .as_str()
            .unwrap()
            .contains("upwork-connector")
    );
    assert!(config["mcpServers"]["upwork-local"].get("type").is_none());
}
#[test]
fn native_menu_import_error_back_exit_and_bright_palette() {
    let dir = temp();
    let out = command(
        dir.path(),
        &["menu"],
        "2\n\"missing file.txt\"\n2\n0\nexit\n",
    );
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.matches("Upwork Main menu:").count(), 3);
    assert!(String::from_utf8_lossy(&out.stderr).contains("Cannot read proxy file"));
    assert!(!text.contains("profiles\""));
}
#[test]
fn credential_input_and_logs_do_not_expose_values() {
    let dir = temp();
    assert!(command(dir.path(),&["add"],r#"{"name":"fixture","url":"socks5://fixture-user:fixture-password@proxy.example:1080"}"#).status.success());
    let out = command(dir.path(), &["status"], "");
    assert!(!String::from_utf8_lossy(&out.stdout).contains("fixture-password"));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("fixture-user"));
    for file in fs::read_dir(dir.path().join("logs")).unwrap() {
        let text = fs::read_to_string(file.unwrap().path()).unwrap();
        assert!(!text.contains("fixture-password"));
        assert!(!text.contains("fixture-user"));
    }
}

#[test]
fn local_proxy_settings_cli_menu_and_safe_invalid_input() {
    let dir = temp();
    let out = command(dir.path(), &["local-settings"], "");
    assert!(out.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&out.stdout).unwrap()["url"],
        "socks5h://127.0.0.1:1595"
    );
    let out = command(dir.path(), &["menu"], "8\nhttp://127.0.0.1:1596\n9\nexit\n");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("default port 1595"));
    let out = command(dir.path(), &["status"], "");
    let status: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(status["selected"], "local-proxy");
    assert_eq!(status["profiles"][0]["url"], "http://127.0.0.1:1596/");
    let before = fs::read(dir.path().join("config/local-proxy.json")).unwrap();
    assert!(
        !command(
            dir.path(),
            &["local-settings", "set"],
            r#"{"url":"http://127.0.0.1:0"}"#
        )
        .status
        .success()
    );
    assert_eq!(
        before,
        fs::read(dir.path().join("config/local-proxy.json")).unwrap()
    );
}

#[cfg(unix)]
#[test]
fn unix_launcher_handles_space_paths_and_missing_binary() {
    let dir = temp();
    let script = dir.path().join("run.sh");
    fs::copy("run.sh", &script).unwrap();
    let missing = Command::new("sh")
        .arg(&script)
        .arg("preview")
        .output()
        .unwrap();
    assert!(!missing.status.success());
    fs::create_dir(dir.path().join("bin")).unwrap();
    let binary = dir.path().join("bin/upwork-connector");
    fs::copy(env!("CARGO_BIN_EXE_upwork-connector"), &binary).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700)).unwrap();
    let out = Command::new("sh")
        .arg(&script)
        .arg("preview")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Upwork Main menu:"));
}
