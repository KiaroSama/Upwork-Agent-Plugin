use std::{
    fs,
    path::Path,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};
use upwork_connector::{config, import};

const LIMIT: usize = 1024 * 1024;

fn temp() -> tempfile::TempDir {
    fs::create_dir_all(".ci-work").unwrap();
    tempfile::Builder::new()
        .prefix("import boundaries ")
        .tempdir_in(".ci-work")
        .unwrap()
}

fn saved_config(root: &Path) -> (std::path::PathBuf, Vec<u8>) {
    let path = root.join("config/connections.json");
    config::update(&path, |config| {
        config.mode = config::Mode::Direct;
        Ok(())
    })
    .unwrap();
    let before = fs::read(&path).unwrap();
    (path, before)
}

fn bounded_process(command: &mut Command) -> Output {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let start = Instant::now();
    while child.try_wait().unwrap().is_none() {
        if start.elapsed() > Duration::from_secs(4) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("Import must reject the input instead of blocking");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn exact_size_input_is_accepted_and_oversize_preserves_state_and_source() {
    let dir = temp();
    let (config, _) = saved_config(dir.path());
    let input = dir.path().join("size boundary.txt");
    let mut bytes = b"socks5://example.invalid:1080\n#".to_vec();
    bytes.resize(LIMIT, b'x');
    fs::write(&input, &bytes).unwrap();
    assert_eq!(
        import::import_file(&config, input.to_str().unwrap()).unwrap(),
        (1, 0)
    );
    let before = fs::read(&config).unwrap();
    bytes.push(b'x');
    fs::write(&input, &bytes).unwrap();
    assert_eq!(
        import::import_file(&config, input.to_str().unwrap())
            .unwrap_err()
            .to_string(),
        "Proxy file exceeds 1 MiB."
    );
    assert_eq!(fs::read(&config).unwrap(), before);
    assert_eq!(fs::read(&input).unwrap(), bytes);
}

#[test]
fn missing_directory_empty_and_invalid_utf8_sources_preserve_configuration() {
    let dir = temp();
    let (config, before) = saved_config(dir.path());
    let input = dir.path().join("input.txt");
    assert!(import::import_file(&config, input.to_str().unwrap()).is_err());
    assert!(import::import_file(&config, dir.path().to_str().unwrap()).is_err());
    for bytes in [vec![], vec![0xff], b"# comments only\n".to_vec()] {
        fs::write(&input, &bytes).unwrap();
        assert!(import::import_file(&config, input.to_str().unwrap()).is_err());
        assert_eq!(fs::read(&config).unwrap(), before);
        assert_eq!(fs::read(&input).unwrap(), bytes);
    }
}

#[test]
fn oversized_sparse_file_uses_the_controlled_size_error() {
    let dir = temp();
    let (config, before) = saved_config(dir.path());
    let input = dir.path().join("large.txt");
    fs::File::create(&input)
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    let out = bounded_process(
        Command::new(env!("CARGO_BIN_EXE_upwork-connector"))
            .arg("--root")
            .arg(dir.path())
            .arg("import")
            .arg(&input),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("Proxy file exceeds 1 MiB."));
    assert_eq!(fs::read(config).unwrap(), before);
    assert_eq!(fs::metadata(input).unwrap().len(), 64 * 1024 * 1024);
}

#[cfg(target_os = "linux")]
#[test]
fn oversized_input_is_size_rejected_under_a_bounded_address_space() {
    let dir = temp();
    let (config, before) = saved_config(dir.path());
    let input = dir.path().join("large.txt");
    fs::File::create(&input)
        .unwrap()
        .set_len(64 * 1024 * 1024)
        .unwrap();
    // Arguments are separate argv entries, never interpolated into the shell command.
    let out = bounded_process(
        Command::new("sh")
            .arg("-c")
            .arg("ulimit -c 0; ulimit -v 65536; exec \"$@\"")
            .arg("import-memory-fixture")
            .arg(env!("CARGO_BIN_EXE_upwork-connector"))
            .arg("--root")
            .arg(dir.path())
            .arg("import")
            .arg(input),
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "Expected a controlled size rejection"
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("Proxy file exceeds 1 MiB."));
    assert_eq!(fs::read(config).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn no_writer_fifo_is_rejected_before_a_blocking_open() {
    let dir = temp();
    let (config, before) = saved_config(dir.path());
    let input = dir.path().join("unopened.fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&input)
            .status()
            .unwrap()
            .success()
    );
    let out = bounded_process(
        Command::new(env!("CARGO_BIN_EXE_upwork-connector"))
            .arg("--root")
            .arg(dir.path())
            .arg("import")
            .arg(input),
    );
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("regular UTF-8 file"));
    assert_eq!(fs::read(config).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn symlink_to_regular_input_is_supported_without_modifying_the_target() {
    let dir = temp();
    let (config, _) = saved_config(dir.path());
    let target = dir.path().join("actual.txt");
    let bytes = b"socks5://example.invalid:1080\n";
    fs::write(&target, bytes).unwrap();
    let link = dir.path().join("linked.txt");
    std::os::unix::fs::symlink(fs::canonicalize(&target).unwrap(), &link).unwrap();
    assert_eq!(
        import::import_file(&config, link.to_str().unwrap()).unwrap(),
        (1, 0)
    );
    assert_eq!(fs::read(target).unwrap(), bytes);
}
