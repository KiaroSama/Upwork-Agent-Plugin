use serde_json::{Value, json};
use std::fs;
use upwork_connector::{
    config::{self, Connections, Mode},
    credentials::Store,
    import,
    logging::Log,
    storage::Lock,
};
fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("rust-config-")
        .tempdir_in(".ci-work")
        .unwrap()
}

#[test]
fn source_generated_profile_oracle() {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/config-oracle.json")).unwrap();
    let cases = oracle["cases"].as_array().unwrap();
    assert!(!cases.is_empty());
    for case in cases {
        let c = serde_json::from_value::<Connections>(case["input"].clone());
        let valid = c.as_ref().ok().is_some_and(|c| c.validate().is_ok());
        assert_eq!(valid, case["valid"].as_bool().unwrap());
        if valid {
            assert_eq!(c.unwrap().status().unwrap(), case["status"]);
        }
    }
}
#[test]
fn config_atomic_errors_preserve_previous_bytes() {
    let dir = temp();
    let file = dir.path().join("connections.json");
    assert!(config::read(&file).unwrap().mode == Mode::Blocked);
    config::update(&file, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let before = fs::read(&file).unwrap();
    assert!(
        config::update(&file, |c| {
            c.mode = Mode::Proxy;
            c.selected = Some("missing".into());
            Ok(())
        })
        .is_err()
    );
    assert_eq!(before, fs::read(&file).unwrap());
    assert!(
        config::update(&file, |c| {
            c.mode = Mode::Blocked;
            Ok(())
        })
        .is_ok()
    );
    let lock = Lock::acquire(&file).unwrap();
    assert!(config::update(&file, |_| Ok(())).is_err());
    drop(lock);
    assert!(config::update(&file, |_| Ok(())).is_ok());
}
#[test]
fn invalid_config_never_defaults_direct() {
    let dir = temp();
    let file = dir.path().join("config.json");
    for data in [
        "not-json",
        r#"{"version":1,"mode":"unknown","profiles":[],"selected":null}"#,
    ] {
        fs::write(&file, data).unwrap();
        assert!(config::read(&file).is_err());
    }
    for url in [
        "http://proxy.example:0",
        "http://proxy.example:65536",
        "http://u:%xx@proxy.example:80",
        "http://proxy.example:80/path",
        "socks5://proxy.example",
        "ftp://proxy.example:21",
    ] {
        assert!(config::proxy_url(url).is_err());
    }
    for url in [
        "http://proxy.example:80",
        "https://proxy.example:443",
        "socks5://[::1]:1080",
        "socks5h://u:p@proxy.example:1080",
    ] {
        assert!(config::proxy_url(url).is_ok());
    }
}
#[test]
fn import_transaction_unicode_duplicates_and_name_collisions() {
    let dir = temp();
    let file = dir.path().join("config.json");
    let list = dir.path().join("Proxy list.txt");
    config::update(&file, |c| {
        c.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    fs::write(&list,"\u{feff}# comment\r\nsocks5://proxy.example:12004:example-user:p@ss%word:UK 5\r\nhttp://proxy.example:80:user:pass:UK 5\r\nhttp://proxy.example:81:user:pass:نام\r\n").unwrap();
    assert_eq!(
        import::import_file(&file, &format!("\"{}\"", list.display())).unwrap(),
        (3, 0)
    );
    let c = config::read(&file).unwrap();
    assert!(c.mode == Mode::Direct);
    assert_eq!(c.profiles[0].name, "UK-5");
    assert_eq!(c.profiles[1].name, "UK-5-2");
    assert_eq!(c.profiles[2].label.as_deref(), Some("نام"));
    let u = url::Url::parse(&c.profiles[0].url).unwrap();
    assert_eq!(
        percent_encoding::percent_decode_str(u.password().unwrap())
            .decode_utf8()
            .unwrap(),
        "p@ss%word"
    );
    assert_eq!(
        import::import_file(&file, list.to_str().unwrap()).unwrap(),
        (0, 3)
    );
    let before = fs::read(&file).unwrap();
    fs::write(
        &list,
        "http://other.example:80:u:p:New\nprivate malformed record",
    )
    .unwrap();
    let error = import::import_file(&file, list.to_str().unwrap())
        .unwrap_err()
        .to_string();
    assert!(error.contains("line 2"));
    assert!(!error.contains("private"));
    assert_eq!(fs::read(&file).unwrap(), before);
    fs::write(&list, [255]).unwrap();
    assert!(import::import_file(&file, list.to_str().unwrap()).is_err());
    assert_eq!(fs::read(&file).unwrap(), before);
}
#[test]
fn log_collision_utc_format_and_safe_failure() {
    let dir = temp();
    let first = Log::new(dir.path(), "upwork-test");
    first.event("INFO", "Started");
    let second = Log::new(dir.path(), "upwork-test");
    second.event("WARNING", "Controlled warning");
    second.event("ERROR", "Controlled failure");
    drop(first);
    drop(second);
    let files = fs::read_dir(dir.path().join("logs"))
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(files.len(), 2);
    for file in files {
        let file = file.unwrap();
        let name = file.file_name().to_string_lossy().into_owned();
        assert!(name.starts_with("upwork-test_"));
        assert!(name.contains("_UTC"));
        let text = fs::read_to_string(file.path()).unwrap();
        assert!(text.contains("UTC] ["));
        assert!(!text.contains("token"));
        fs::remove_file(file.path()).unwrap();
    }
    let blocked = dir.path().join("no-dir");
    fs::write(&blocked, "fixture").unwrap();
    let log = Log::new(&blocked, "upwork-test");
    log.event("ERROR", "Safe fallback");
}
#[test]
fn legacy_session_conversion_is_read_only() {
    let dir = temp();
    let store = Store {
        root: dir.path().into(),
    };
    fs::create_dir_all(store.legacy_path("tokens").parent().unwrap()).unwrap();
    let token = json!({"access_token":"test-only-access","token_type":"Bearer","expires_in":3600,"expires_at":chrono::Utc::now().timestamp_millis()+3600000,"refresh_token":"test-only-refresh","issuer":"https://issuer.example","requested_scope":"read write"});
    fs::write(
        store.legacy_path("tokens"),
        serde_json::to_vec(&token).unwrap(),
    )
    .unwrap();
    fs::write(store.legacy_path("client_info"),r#"{"client_id":"test-only-client","redirect_uris":["http://localhost:23037/oauth/callback"]}"#).unwrap();
    let before = fs::read(store.legacy_path("tokens")).unwrap();
    let loaded = store.load_local().unwrap().unwrap();
    assert_eq!(loaded.client_id, "test-only-client");
    assert!(loaded.token_response.is_some());
    assert_eq!(loaded.granted_scopes, vec!["read", "write"]);
    assert!(loaded.token_received_at.is_some());
    assert_eq!(fs::read(store.legacy_path("tokens")).unwrap(), before);
    assert!(!store.path().exists());
}
