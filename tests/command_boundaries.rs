use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use upwork_connector::config;

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("command boundaries ")
        .tempdir_in(".ci-work")
        .unwrap()
}

fn run(root: &Path, args: &[&str], input: &str) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_upwork-connector"));
    command
        .arg("--root")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().unwrap();
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
            panic!("Bounded command did not exit");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn invalid_command_shapes_have_no_filesystem_or_network_side_effects() {
    let cases: &[&[&str]] = &[
        &["direct", "--help"],
        &["direct", "--root", "another-installation"],
        &["block", "extra"],
        &["local-proxy", "extra"],
        &["add", "extra"],
        &["import"],
        &["import", ""],
        &["import", "missing-file", "extra"],
        &["select"],
        &["select", ""],
        &["select", "profile", "extra"],
        &["local-settings", "unexpected"],
        &["local-settings", "set", "extra"],
        &["sign-in", "extra"],
        &["bridge", "extra"],
        &["register", "extra"],
        &["desktop-config", "extra"],
        &["status", "extra"],
        &["preview", "extra"],
        &["menu", "extra"],
        &["cleanup", "--apply", "extra"],
        &["cleanup", "unexpected"],
        &["--version", "extra"],
        &["unknown-command"],
    ];
    for args in cases {
        let dir = temp();
        let output = run(dir.path(), args, "");
        assert_eq!(output.status.code(), Some(1), "case: {args:?}");
        assert!(output.stdout.is_empty(), "case: {args:?}");
        assert!(String::from_utf8_lossy(&output.stderr).contains("Invalid command arguments"));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
    }
}

#[test]
fn rejected_direct_invocations_preserve_an_existing_selected_proxy() {
    let dir = temp();
    let path = dir.path().join("config/connections.json");
    fs::create_dir(path.parent().unwrap()).unwrap();
    let bytes = br#"{"version":1,"mode":"proxy","selected":"keep","profiles":[{"name":"keep","url":"http://proxy.invalid:8080"}]}"#;
    fs::write(&path, bytes).unwrap();
    for args in [vec!["direct", "extra"], vec!["direct", "--root", "other"]] {
        assert!(!run(dir.path(), &args, "").status.success());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!dir.path().join(".cache").exists());
        assert!(!dir.path().join("logs").exists());
    }
}

#[test]
fn valid_direct_and_block_commands_still_require_explicit_dispatch() {
    let dir = temp();
    assert!(run(dir.path(), &["direct"], "").status.success());
    let direct: Value = serde_json::from_slice(&run(dir.path(), &["status"], "").stdout).unwrap();
    assert_eq!(direct["mode"], "direct");
    assert!(run(dir.path(), &["block"], "").status.success());
    let blocked: Value = serde_json::from_slice(&run(dir.path(), &["status"], "").stdout).unwrap();
    assert_eq!(blocked["mode"], "blocked");
    assert!(run(dir.path(), &["menu"], "exit\n").status.success());
    assert!(run(dir.path(), &[], "exit\n").status.success());
}

#[test]
fn sanitized_proxy_urls_retain_ports_credentials_are_removed_and_ipv6_is_preserved() {
    for (scheme, port) in [
        ("http", 80),
        ("https", 443),
        ("socks5", 1080),
        ("socks5h", 1080),
    ] {
        for host in ["proxy.invalid", "127.0.0.1", "[::1]"] {
            let original = format!("{scheme}://fixture-user:fixture-password@{host}:{port}");
            let public = config::public_proxy_url(&original).unwrap();
            let parsed = config::proxy_url(&public).unwrap();
            assert_eq!(parsed.port_or_known_default(), Some(port));
            assert_eq!(parsed.host(), config::proxy_url(&original).unwrap().host());
            assert!(parsed.username().is_empty());
            assert!(parsed.password().is_none());
            assert!(!public.contains("fixture-user"));
            assert!(!public.contains("fixture-password"));
            assert_eq!(config::public_proxy_url(&public).unwrap(), public);
        }
    }
}

#[test]
fn local_settings_default_port_output_can_be_set_again_without_losing_the_port() {
    let dir = temp();
    for url in [
        "http://127.0.0.1:80",
        "https://[::1]:443",
        "http://proxy.invalid:8080",
    ] {
        let input = json!({"url":url}).to_string();
        let set = run(dir.path(), &["local-settings", "set"], &input);
        assert!(set.status.success());
        let output = run(dir.path(), &["local-settings"], "");
        assert!(output.status.success());
        let public = String::from_utf8(output.stdout).unwrap();
        let set = run(dir.path(), &["local-settings", "set"], &public);
        assert!(set.status.success());
        let second = run(dir.path(), &["local-settings"], "");
        assert_eq!(second.stdout, public.as_bytes());
    }
}

#[test]
fn status_default_port_output_remains_valid_and_does_not_change_private_storage() {
    let dir = temp();
    let path = dir.path().join("config/connections.json");
    let profile = json!({
        "name": "keep",
        "url": "http://fixture-user:fixture-password@proxy.invalid:80"
    });
    assert!(
        run(dir.path(), &["add"], &profile.to_string())
            .status
            .success()
    );
    let before = fs::read(&path).unwrap();
    let output = run(dir.path(), &["status"], "");
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let public = value["profiles"][0]["url"].as_str().unwrap();
    let parsed = config::proxy_url(public).unwrap();
    assert_eq!(parsed.port_or_known_default(), Some(80));
    assert!(!public.contains("fixture-"));
    assert_eq!(fs::read(path).unwrap(), before);
}
