use std::fs;
use upwork_connector::{
    config::{self, Mode, Profile},
    import::import_file,
};

fn fixture() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::tempdir_in(".ci-work").unwrap()
}

#[test]
fn collision_allocation_keeps_holes_duplicates_order_and_active_route() {
    let root = fixture();
    let path = root.path().join("config/connections.json");
    config::update(&path, |value| {
        value.profiles = vec![
            Profile {
                name: "pool".into(),
                url: "http://127.0.0.1:9000".into(),
                label: Some("Original".into()),
            },
            Profile {
                name: "pool-3".into(),
                url: "http://127.0.0.1:9001".into(),
                label: None,
            },
        ];
        value.mode = Mode::Proxy;
        value.selected = Some("pool".into());
        Ok(())
    })
    .unwrap();
    let source = root.path().join("proxies.txt");
    fs::write(&source, "http://127.0.0.1:9100:u:p:pool\nhttp://127.0.0.1:9100:u:p:pool\nhttp://127.0.0.1:9101:u:p:pool\nhttp://127.0.0.1:9102:u:p:pool-2\nhttp://127.0.0.1:9103:u:p:pool\n").unwrap();
    assert_eq!(
        import_file(&path, source.to_str().unwrap()).unwrap(),
        (4, 1)
    );
    let value = config::read(&path).unwrap();
    let names: Vec<_> = value
        .profiles
        .iter()
        .map(|profile| profile.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["pool", "pool-3", "pool-2", "pool-4", "pool-2-2", "pool-5"]
    );
    assert!(value.mode == Mode::Proxy);
    assert_eq!(value.selected.as_deref(), Some("pool"));
    assert_eq!(value.profiles[0].url, "http://127.0.0.1:9000");
    assert_eq!(value.profiles[0].label.as_deref(), Some("Original"));
    let before = fs::read(&path).unwrap();
    assert_eq!(
        import_file(&path, source.to_str().unwrap()).unwrap(),
        (0, 5)
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    fs::write(&source, "http://127.0.0.1:9104:u:p:pool\n").unwrap();
    assert_eq!(
        import_file(&path, source.to_str().unwrap()).unwrap(),
        (1, 0)
    );
    assert_eq!(
        config::read(&path).unwrap().profiles.last().unwrap().name,
        "pool-6"
    );
}

#[test]
fn unicode_and_truncated_labels_keep_the_existing_slug_rule() {
    let root = fixture();
    let path = root.path().join("config/connections.json");
    let source = root.path().join("proxies.txt");
    let prefix = "a".repeat(54);
    let input = format!(
        "http://127.0.0.1:9200:u:p:پروکسی\nhttp://127.0.0.1:9201:u:p:代理\nhttp://127.0.0.1:9202:u:p:{prefix}-one\nhttp://127.0.0.1:9203:u:p:{prefix}-two\n"
    );
    fs::write(&source, input).unwrap();
    assert_eq!(
        import_file(&path, source.to_str().unwrap()).unwrap(),
        (4, 0)
    );
    let value = config::read(&path).unwrap();
    let names: Vec<_> = value
        .profiles
        .iter()
        .map(|profile| profile.name.clone())
        .collect();
    assert_eq!(
        names,
        vec![
            "proxy".to_owned(),
            "proxy-2".to_owned(),
            prefix.clone(),
            format!("{prefix}-2")
        ]
    );
    assert!(value.mode == Mode::Blocked);
    assert_eq!(value.profiles[0].label.as_deref(), Some("پروکسی"));
    let before = fs::read(&path).unwrap();
    fs::write(
        &source,
        "http://127.0.0.1:9300:u:p:pool\ninvalid-final-line\n",
    )
    .unwrap();
    assert!(import_file(&path, source.to_str().unwrap()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}

#[test]
fn thousands_of_colliding_labels_round_trip_and_reimport_without_renaming() {
    let root = fixture();
    let path = root.path().join("config/connections.json");
    let source = root.path().join("proxies.txt");
    let count = 4_096;
    let input: String = (0..count)
        .map(|index| format!("http://127.0.0.1:{}:u:p:pool\n", 10_000 + index))
        .collect();
    assert!(input.len() < 1024 * 1024);
    fs::write(&source, input).unwrap();
    assert_eq!(
        import_file(&path, source.to_str().unwrap()).unwrap(),
        (count, 0)
    );
    let value = config::read(&path).unwrap();
    assert_eq!(value.profiles.len(), count);
    for (index, profile) in value.profiles.iter().enumerate() {
        let expected = if index == 0 {
            "pool".to_owned()
        } else {
            format!("pool-{}", index + 1)
        };
        assert_eq!(profile.name, expected);
    }
    assert_eq!(
        import_file(&path, source.to_str().unwrap()).unwrap(),
        (0, count)
    );
}
