use std::fs;
use upwork_connector::{
    config::{self, Mode},
    route::Route,
};

#[test]
fn changed_bytes_are_validated_and_revocation_is_shared_and_irreversible() {
    fs::create_dir_all(".ci-work").unwrap();
    let root = tempfile::tempdir_in(".ci-work").unwrap();
    let path = root.path().join("connections.json");
    config::update(&path, |v| {
        v.mode = Mode::Direct;
        Ok(())
    })
    .unwrap();
    let original = fs::read(&path).unwrap();
    let route = Route::new(path.clone()).unwrap();
    let clone = route.clone();
    route.check().unwrap();
    fs::write(&path, [original.as_slice(), b" \n"].concat()).unwrap();
    clone.check().unwrap();
    route.check().unwrap();
    fs::write(&path, b"{\"version\":1,\"mode\":\"direct\",\"profiles\":[{\"name\":\"bad name\",\"url\":\"http://invalid:1\"}]}").unwrap();
    assert!(
        clone.check().is_err(),
        "Inactive invalid profiles must still revoke"
    );
    fs::write(&path, &original).unwrap();
    assert!(route.check().is_err());
    assert!(clone.check().is_err());
    Route::new(path).unwrap().check().unwrap();
}

#[test]
fn replacement_deletion_invalid_utf8_and_completed_cycles_never_reuse_old_validity() {
    for case in ["deleted", "directory", "utf8", "trailing", "cycle"] {
        fs::create_dir_all(".ci-work").unwrap();
        let root = tempfile::tempdir_in(".ci-work").unwrap();
        let path = root.path().join("connections.json");
        config::update(&path, |v| {
            v.mode = Mode::Direct;
            Ok(())
        })
        .unwrap();
        let route = Route::new(path.clone()).unwrap();
        route.check().unwrap();
        match case {
            "deleted" => fs::remove_file(&path).unwrap(),
            "directory" => {
                fs::remove_file(&path).unwrap();
                fs::create_dir(&path).unwrap();
            }
            "utf8" => fs::write(&path, [0xff]).unwrap(),
            "trailing" => {
                let mut bytes = fs::read(&path).unwrap();
                bytes.push(b'x');
                fs::write(&path, bytes).unwrap();
            }
            _ => {
                config::update(&path, |v| {
                    v.mode = Mode::Blocked;
                    Ok(())
                })
                .unwrap();
                config::update(&path, |v| {
                    v.mode = Mode::Direct;
                    Ok(())
                })
                .unwrap();
            }
        }
        assert!(route.check().is_err(), "{case}");
        assert!(route.clone().check().is_err(), "{case}");
    }
}
