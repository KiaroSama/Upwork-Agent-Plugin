use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use upwork_connector::{
    config::{self, Connections, Mode, Profile},
    route::Route,
    storage,
};

fn fixture(mode: Mode) -> (tempfile::TempDir, PathBuf) {
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("config/connections.json");
    config::update(&path, |config| {
        config.mode = mode;
        config.selected = Some("a".into());
        config.profiles = vec![
            Profile {
                name: "a".into(),
                url: "socks5://localhost:1595".into(),
                label: Some("A".into()),
            },
            Profile {
                name: "b".into(),
                url: "http://localhost:1596".into(),
                label: Some("B".into()),
            },
        ];
        Ok(())
    })
    .unwrap();
    (root, path)
}

fn revision(path: &Path) -> Option<String> {
    serde_json::from_slice::<Value>(&fs::read(path).unwrap())
        .unwrap()
        .get("_route_revision")
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn cycle(path: &Path, mode: Mode) {
    config::update(path, |config| {
        config.mode = Mode::Blocked;
        Ok(())
    })
    .unwrap();
    config::update(path, |config| {
        config.mode = mode;
        Ok(())
    })
    .unwrap();
}

#[test]
fn baseline_cycle_direct_is_revoked_without_observing_blocked_state() {
    let (_root, path) = fixture(Mode::Direct);
    let old = Route::new(path.clone()).unwrap();
    let clone = old.clone();
    cycle(&path, Mode::Direct);
    assert!(
        old.check().is_err(),
        "An unobserved completed route cycle must revoke the old route"
    );
    assert!(clone.check().is_err());
    assert!(Route::new(path).unwrap().check().is_ok());
}

#[test]
fn baseline_cycle_proxy_selection_and_credentials_cannot_resurrect() {
    for credentials in [false, true] {
        let (_root, path) = fixture(Mode::Proxy);
        let old = Route::new(path.clone()).unwrap();
        if credentials {
            let original = config::read(&path).unwrap().profiles[0].url.clone();
            config::update(&path, |c| {
                c.profiles[0].url = "socks5://synthetic:fixture@localhost:1595".into();
                Ok(())
            })
            .unwrap();
            config::update(&path, |c| {
                c.profiles[0].url = original;
                Ok(())
            })
            .unwrap();
        } else {
            config::update(&path, |c| {
                c.selected = Some("b".into());
                Ok(())
            })
            .unwrap();
            config::update(&path, |c| {
                c.selected = Some("a".into());
                Ok(())
            })
            .unwrap();
        }
        assert!(
            old.check().is_err(),
            "A completed proxy cycle must not be invisible"
        );
        assert!(Route::new(path).unwrap().check().is_ok());
    }
}

#[tokio::test]
async fn baseline_cycle_guard_refuses_to_poll_a_new_operation() {
    let (_root, path) = fixture(Mode::Direct);
    let old = Route::new(path.clone()).unwrap();
    cycle(&path, Mode::Direct);
    let polled = AtomicUsize::new(0);
    let result = old
        .guard(async {
            polled.fetch_add(1, Ordering::SeqCst);
        })
        .await;
    assert!(
        result.is_err(),
        "Revoked routes must fail before operation polling"
    );
    assert_eq!(polled.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn a_cycle_between_guard_polls_cancels_an_in_flight_operation() {
    let (_root, path) = fixture(Mode::Direct);
    let old = Route::new(path.clone()).unwrap();
    let polled = AtomicUsize::new(0);
    let mut guarded = Box::pin(old.guard(async {
        polled.fetch_add(1, Ordering::SeqCst);
        std::future::pending::<()>().await;
    }));
    assert!(futures::poll!(&mut guarded).is_pending());
    // No await occurs between the two writes, so the current-thread watcher
    // cannot observe the intermediate state. No sleep is a readiness oracle.
    cycle(&path, Mode::Direct);
    let result = tokio::time::timeout(Duration::from_secs(1), guarded)
        .await
        .expect("A persisted cycle must cancel at a watcher tick");
    assert!(result.is_err());
    assert_eq!(polled.load(Ordering::SeqCst), 1);
}

#[test]
fn inactive_profiles_labels_order_and_noop_updates_keep_existing_routes() {
    for mode in [Mode::Direct, Mode::Proxy] {
        let (_root, path) = fixture(mode);
        let old = Route::new(path.clone()).unwrap();
        let before = revision(&path);
        config::update(&path, |c| {
            c.profiles[0].label = Some("Renamed A".into());
            c.profiles[1].url = "http://localhost:2596".into();
            c.profiles.push(Profile {
                name: "unused".into(),
                url: "http://localhost:3596".into(),
                label: None,
            });
            c.profiles.reverse();
            Ok(())
        })
        .unwrap();
        config::update(&path, |_| Ok(())).unwrap();
        assert_eq!(revision(&path), before);
        assert!(old.check().is_ok());
    }
}

#[test]
fn canonical_equivalent_selected_proxy_spellings_do_not_disconnect() {
    let (_root, path) = fixture(Mode::Proxy);
    let old = Route::new(path.clone()).unwrap();
    let before = revision(&path);
    config::update(&path, |c| {
        c.profiles[0].url = "socks5h://localhost:1595".into();
        Ok(())
    })
    .unwrap();
    assert_eq!(revision(&path), before);
    assert!(old.check().is_ok());
    config::update(&path, |c| {
        c.profiles[0].url = "HTTP://LOCALHOST:80/".into();
        Ok(())
    })
    .unwrap();
    let old = Route::new(path.clone()).unwrap();
    let before = revision(&path);
    config::update(&path, |c| {
        c.profiles[0].url = "http://localhost:80".into();
        Ok(())
    })
    .unwrap();
    assert_eq!(revision(&path), before);
    assert!(old.check().is_ok());
}

#[test]
fn semantic_transitions_receive_distinct_durable_revisions() {
    let (_root, path) = fixture(Mode::Direct);
    let mut seen = std::collections::HashSet::new();
    for mode in [
        Mode::Direct,
        Mode::Blocked,
        Mode::Proxy,
        Mode::Direct,
        Mode::Proxy,
    ] {
        config::update(&path, |c| {
            c.mode = mode;
            Ok(())
        })
        .unwrap();
        let revision = revision(&path).expect("Active transition records its revision");
        assert!(uuid::Uuid::parse_str(&revision).is_ok());
        assert!(
            seen.insert(revision),
            "Each real transition needs a fresh token"
        );
    }
}

#[test]
fn rejected_changes_preserve_exact_bytes_and_revision() {
    let (_root, path) = fixture(Mode::Direct);
    let before = fs::read(&path).unwrap();
    assert!(
        config::update(&path, |c| {
            c.mode = Mode::Blocked;
            Err(upwork_connector::error::Error::Safe(
                "Injected rejected change.",
            ))
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(
        config::update(&path, |c| {
            c.mode = Mode::Proxy;
            c.selected = Some("missing".into());
            Ok(())
        })
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(Route::new(path).unwrap().check().is_ok());
}

#[test]
fn legacy_documents_are_read_without_rewrite_and_upgrade_only_on_transition() {
    let (root, path) = fixture(Mode::Direct);
    let legacy = Connections {
        mode: Mode::Direct,
        ..Connections::default()
    };
    storage::atomic_json(&path, &legacy).unwrap();
    let bytes = fs::read(&path).unwrap();
    let old = Route::new(path.clone()).unwrap();
    assert!(config::read(&path).is_ok());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    config::update(&path, |_| Ok(())).unwrap();
    assert!(revision(&path).is_none());
    assert!(old.check().is_ok());
    cycle(&path, Mode::Direct);
    assert!(old.check().is_err());
    assert!(revision(&path).is_some());
    let absent = root.path().join("absent.json");
    assert!(matches!(config::read(&absent).unwrap().mode, Mode::Blocked));
    assert!(!absent.exists());
}

#[test]
fn malformed_revision_is_fail_closed_and_not_rewritten() {
    let (_root, path) = fixture(Mode::Direct);
    let old = Route::new(path.clone()).unwrap();
    for revision in [json!("not-a-uuid"), json!(false), json!({}), json!(17)] {
        let raw = json!({"version":1,"mode":"direct","selected":null,"profiles":[],"_route_revision":revision});
        fs::write(&path, serde_json::to_vec(&raw).unwrap()).unwrap();
        let before = fs::read(&path).unwrap();
        assert!(config::read(&path).is_err());
        assert!(config::update(&path, |_| Ok(())).is_err());
        assert!(old.check().is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn observed_revocation_remains_latched_even_if_the_exact_old_document_is_restored() {
    let (_root, path) = fixture(Mode::Direct);
    let old = Route::new(path.clone()).unwrap();
    let bytes = fs::read(&path).unwrap();
    config::update(&path, |c| {
        c.mode = Mode::Blocked;
        Ok(())
    })
    .unwrap();
    assert!(old.check().is_err());
    fs::write(&path, bytes).unwrap();
    assert!(old.check().is_err());
    assert!(Route::new(path).unwrap().check().is_ok());
}

#[test]
fn local_settings_and_import_writes_preserve_or_change_only_the_effective_route() {
    let (root, path) = fixture(Mode::Direct);
    let old = Route::new(path.clone()).unwrap();
    let before = revision(&path);
    let source = root.path().join("proxy-input.txt");
    fs::write(&source, "http://localhost:4001\n").unwrap();
    upwork_connector::import::import_file(&path, source.to_str().unwrap()).unwrap();
    upwork_connector::settings::save(
        root.path(),
        upwork_connector::settings::LocalProxy {
            url: "http://localhost:4002".into(),
        },
    )
    .unwrap();
    assert_eq!(revision(&path), before);
    assert!(old.check().is_ok());
    upwork_connector::settings::select(root.path()).unwrap();
    config::update(&path, |c| {
        c.mode = Mode::Direct;
        c.selected = None;
        Ok(())
    })
    .unwrap();
    assert!(old.check().is_err());
}
