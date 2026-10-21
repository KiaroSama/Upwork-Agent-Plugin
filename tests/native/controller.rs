use super::{
    child::{Result, require, tool},
    fixture,
};
use serde_json::{Value, json};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};
use tokio::{
    process::Command,
    time::{Instant, timeout},
};
use upwork_connector::logging::Log;

pub const SUITES: [&str; 13] = [
    "probe-oracles",
    "cli",
    "protocol",
    "extended",
    "sign-in",
    "bridge",
    "credential-coordination",
    "initialization-ping",
    "state-reader",
    "import-scale",
    "route-revision",
    "http-media",
    "ordinary-http",
];
fn option(args: &[String], name: &str) -> Result<PathBuf> {
    let index = args
        .iter()
        .position(|v| v == name)
        .ok_or("Missing native argument")?;
    Ok(PathBuf::from(
        args.get(index + 1).ok_or("Missing native argument value")?,
    ))
}
pub async fn digest(path: &Path) -> Result<String> {
    let output = tool("sha256sum", &[OsStr::new("--"), path.as_os_str()], 5).await?;
    let text = std::str::from_utf8(&output)?;
    let digest = text.split_whitespace().next().ok_or("Missing digest")?;
    require(
        digest.len() == 64 && digest.bytes().all(|c| c.is_ascii_hexdigit()),
        "Invalid SHA256 output",
    )?;
    Ok(digest.into())
}
fn generation(path: &Path) -> Result<(u64, u64, u64, i64, i64)> {
    let m = fs::metadata(path)?;
    Ok((m.dev(), m.ino(), m.len(), m.mtime(), m.mtime_nsec()))
}
fn receipt(path: &Path, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
async fn worker(args: &[String]) -> Result<()> {
    let suite = args.get(2).ok_or("Missing native suite")?;
    require(SUITES.contains(&suite.as_str()), "Unknown native suite")?;
    let binary = option(args, "--binary")?.canonicalize()?;
    let output = option(args, "--output")?;
    let parent = output.parent().ok_or("Missing output parent")?;
    let log = Log::new(parent, "native-acceptance");
    log.event(
        "INFO",
        &format!("Suite started name={suite}; synthetic input only."),
    );
    let binary_digest = digest(&binary).await?;
    let mut value = match suite.as_str() {
        "probe-oracles" => {
            let controls = super::oracle::controls();
            lifecycle(&binary, parent).await?;
            controls
        }
        "cli" => super::cli::run(&binary, parent).await?,
        "protocol" => super::protocol::run(&binary, parent, "protocol", "sign-in").await?,
        "extended" => super::protocol::run(&binary, parent, "extended", "sign-in").await?,
        "sign-in" | "bridge" => super::protocol::run(&binary, parent, "terminal", suite).await?,
        "credential-coordination" => super::coordination::run(&binary, parent).await?,
        "initialization-ping" => super::protocol::run(&binary, parent, "ping", "sign-in").await?,
        "state-reader" => super::state::run(&binary, parent).await?,
        "import-scale" => super::state::import(&binary, parent).await?,
        "route-revision" => super::coordination::route(&binary, parent).await?,
        "http-media" => super::protocol::run(&binary, parent, "media", "sign-in").await?,
        "ordinary-http" => super::ordinary::run(&binary, parent).await?,
        _ => unreachable!(),
    };
    value["binary_sha256"] = json!(binary_digest);
    receipt(&output, &value)?;
    let passed = if suite == "probe-oracles" {
        value["passed"] == true
    } else {
        value["passed"] == value[if suite == "cli" { "checks" } else { "cases" }]
    };
    log.event(
        if passed { "INFO" } else { "ERROR" },
        &format!("Suite completed name={suite} passed={passed}; all owned children reaped."),
    );
    require(passed, "Native suite assertion failed")
}
async fn lifecycle(binary: &Path, parent: &Path) -> Result<()> {
    // Exercise the actual child owner against this controller's fixed-output fixture mode.
    let root = fixture::temporary(parent, "lifecycle-")?;
    for (case, first, invalid, exit) in [
        ("success", Some("result"), false, 0),
        ("server_error", Some("error"), false, 0),
        ("no_output", None, false, 1),
        ("wrong_id", None, true, 1),
        ("notification", Some("result"), false, 0),
        ("mixed", None, true, 1),
        ("malformed", None, true, 1),
        ("duplicate", Some("result"), true, 0),
        ("invalid_utf8", None, true, 1),
        ("unrelated_crash", None, false, -9),
    ] {
        let mut child = super::child::NativeChild::spawn(
            &std::env::current_exe()?,
            root.path(),
            None,
            &["fixture-child", case],
        )?;
        let (code, timed) = child.finish(1.0, true).await?;
        let mut observed = super::oracle::Observation::default();
        for line in &child.records {
            observed.line(line);
        }
        require(
            !timed
                && code == exit
                && observed.first == first
                && (observed.invalid || child.reader_invalid) == invalid,
            "Child lifecycle oracle mismatch",
        )?;
        child.close().await?;
    }
    let mut child = super::child::NativeChild::spawn(
        &std::env::current_exe()?,
        root.path(),
        None,
        &["fixture-child", "timeout"],
    )?;
    let result = child.finish(0.02, true).await?;
    require(
        result.1 && result.0 == -9,
        "Owned timeout did not kill and reap",
    )?;
    child.close().await?;
    let result = super::child::invoke(binary, root.path(), &["--version"], b"", 4.0).await?;
    require(
        result.0 == 0 && !result.3,
        "Lifecycle positive control failed",
    )
}
pub async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    // NativeChild always inserts --root; this mode is private to lifecycle self-controls.
    if args.get(3).map(String::as_str) == Some("fixture-child") {
        let case = args
            .get(4)
            .map(String::as_str)
            .ok_or("Missing child fixture case")?;
        if case == "timeout" {
            tokio::time::sleep(Duration::from_secs(10)).await;
            return Ok(());
        }
        if case == "unrelated_crash" {
            let pid = std::process::id().to_string();
            tool("kill", &[OsStr::new("-KILL"), OsStr::new(&pid)], 2).await?;
            return Err("Self-signal did not terminate fixture".into());
        }
        let result = "{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{}}\n";
        let (output,exit)=match case {
            "success"=>(result.to_owned(),0),
            "server_error"=>("{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32602,\"message\":\"fixture\"}}\n".into(),0),
            "no_output"=>(String::new(),1),"wrong_id"=>(result.replace("\"id\":1","\"id\":99"),1),
            "notification"=>(format!("{{\"jsonrpc\":\"2.0\",\"method\":\"notice\"}}\n{result}"),0),
            "mixed"=>("{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{},\"error\":{}}\n".into(),1),
            "malformed"=>("not-json\n".into(),1),"duplicate"=>(result.repeat(2),0),
            "invalid_utf8"=>{use std::io::Write;std::io::stdout().write_all(b"\xff\n")?;std::process::exit(1);},
            _=>return Err("Unknown lifecycle fixture".into()),
        };
        use std::io::Write;
        std::io::stdout().write_all(output.as_bytes())?;
        std::process::exit(exit);
    }
    if args.get(1).map(String::as_str) == Some("--worker") {
        return worker(&args).await;
    }
    require(
        std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"),
        "Native acceptance runs only in GitHub-hosted CI",
    )?;
    let archive = option(&args, "--archive")?.canonicalize()?;
    let output = option(&args, "--output")?;
    let output = if output.is_absolute() {
        output
    } else {
        fixture::root_path().join(output)
    };
    let parent = output
        .parent()
        .ok_or("Missing output parent")?
        .canonicalize()?;
    require(
        parent.starts_with(fixture::root_path().join(".ci-work").canonicalize()?),
        "Output must be project CI-owned",
    )?;
    fs::create_dir(&output)?;
    let log = Log::new(&output, "native-acceptance");
    log.event(
        "INFO",
        "Controller started; one immutable packaged executable and thirteen sequential suites.",
    );
    let expected = fs::read_to_string(PathBuf::from(format!("{}.sha256", archive.display())))?;
    let archive_digest = digest(&archive).await?;
    require(
        expected
            .trim_start_matches('\u{feff}')
            .split_whitespace()
            .next()
            == Some(archive_digest.as_str()),
        "Archive SHA256 mismatch",
    )?;
    let listing = tool("tar", &[OsStr::new("-tvzf"), archive.as_os_str()], 5).await?;
    let text = std::str::from_utf8(&listing)?;
    require(
        text.lines()
            .all(|line| !line.starts_with('l') && !line.starts_with('h')),
        "Archive contains links",
    )?;
    let names = tool("tar", &[OsStr::new("-tzf"), archive.as_os_str()], 5).await?;
    let names = std::str::from_utf8(&names)?;
    let bins: Vec<_> = names
        .lines()
        .filter(|v| v.ends_with("/bin/upwork-connector"))
        .collect();
    require(bins.len() == 1, "Archive executable inventory mismatch")?;
    let bytes = tool(
        "tar",
        &[
            OsStr::new("-xOzf"),
            archive.as_os_str(),
            OsStr::new("--"),
            OsStr::new(bins[0]),
        ],
        5,
    )
    .await?;
    require(
        bytes.len() < 40 * 1024 * 1024,
        "Packaged executable too large",
    )?;
    let binary = output.join("upwork-connector");
    fs::write(&binary, bytes)?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o500))?;
    let binary_digest = digest(&binary).await?;
    let original = generation(&binary)?;
    let mut results = Vec::new();
    for suite in SUITES {
        require(
            generation(&binary)? == original,
            "Owned executable changed between suites",
        )?;
        let capture = fs::File::create(output.join(format!("{suite}-controller.log")))?;
        let mut command = Command::new(std::env::current_exe()?);
        command
            .arg("--worker")
            .arg(suite)
            .arg("--binary")
            .arg(&binary)
            .arg("--output")
            .arg(output.join(format!("{suite}.json")))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(capture.try_clone()?)
            .kill_on_drop(true)
            .process_group(0);
        for key in ["TEMP", "TMP", "TMPDIR"] {
            command.env(key, &output);
        }
        let started = Instant::now();
        let mut child = command.spawn()?;
        let pid = child.id().ok_or("Missing suite pid")?;
        let stdout = child.stdout.take().ok_or("Missing suite output")?;
        let progress = tokio::spawn(async move {
            use std::io::Write;
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut lines = BufReader::new(stdout).lines();
            let mut capture = capture;
            while let Some(line) = lines.next_line().await? {
                writeln!(capture, "{line}")?;
                println!("suite={suite} {line}");
            }
            Ok::<_, std::io::Error>(())
        });
        let status = match timeout(Duration::from_secs(180), child.wait()).await {
            Ok(status) => status?,
            Err(_) => {
                let group = format!("-{pid}");
                tool(
                    "kill",
                    &[OsStr::new("-KILL"), OsStr::new("--"), OsStr::new(&group)],
                    3,
                )
                .await?;
                timeout(Duration::from_secs(5), child.wait()).await??;
                progress.abort();
                let _ = progress.await;
                log.event(
                    "ERROR",
                    &format!(
                        "Suite deadline exceeded name={suite}; owned process group terminated."
                    ),
                );
                return Err("Native suite deadline exceeded".into());
            }
        };
        if !status.success() {
            let group = format!("-{pid}");
            let _ = tool(
                "kill",
                &[OsStr::new("-KILL"), OsStr::new("--"), OsStr::new(&group)],
                3,
            )
            .await;
        }
        let mut progress = progress;
        match timeout(Duration::from_secs(1), &mut progress).await {
            Ok(result) => result??,
            Err(_) => {
                progress.abort();
                let _ = progress.await;
                return Err("Suite progress reader did not drain".into());
            }
        }
        results.push(json!({"suite":suite,"exit":status.code(),"seconds":started.elapsed().as_secs_f64(),"reaped":true}));
        receipt(
            &output.join("summary.json"),
            &json!({"binary_sha256":binary_digest,"suites":results}),
        )?;
        println!("{suite}: exit={}", status.code().unwrap_or(-1));
        require(status.success(), "Native suite failed")?;
    }
    require(
        generation(&binary)? == original,
        "Owned executable changed during final suite",
    )?;
    fs::remove_file(binary)?;
    log.event(
        "INFO",
        "All thirteen suites passed; extracted executable removed and owned workers reaped.",
    );
    Ok(())
}
