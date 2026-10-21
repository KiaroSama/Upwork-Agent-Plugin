use super::{
    child::{Result, invoke, require, tool},
    fixture::temporary,
};
use serde_json::{Value, json};
use std::os::unix::fs::symlink;
use std::{fs, path::Path, time::Instant};
const LIMIT: usize = 16 * 1024 * 1024;
fn seed(root: &Path) -> Result<()> {
    fs::create_dir_all(root.join("config"))?;
    fs::create_dir_all(root.join(".state/rust"))?;
    fs::write(
        root.join("config/connections.json"),
        serde_json::to_vec(
            &json!({"version":1,"mode":"proxy","selected":"fixture","profiles":[{"name":"fixture","url":"http://127.0.0.1:1"}]}),
        )?,
    )?;
    Ok(())
}
async fn observe(binary: &Path, root: &Path, command: &str) -> Result<Value> {
    let (exit, stdout, stderr, timed) = invoke(binary, root, &[command], b"", 1.5).await?;
    let stderr = String::from_utf8(stderr)?;
    let stdout = String::from_utf8(stdout)?;
    let rejection = if stderr.contains("Internal state must be a regular file within 16 MiB;") {
        "state"
    } else if stderr.contains("Invalid JSON configuration or protocol message.") {
        "json"
    } else if stderr.contains("OAuth failed.") {
        "auth_storage"
    } else if stderr.contains("Connection state must be valid UTF-8;") {
        "utf8"
    } else if stderr.contains("Internal state metadata unavailable.") {
        "metadata"
    } else {
        "other"
    };
    let marker = if stderr.contains("Upwork sign-in required.") {
        "signin"
    } else if stdout.contains("Native Upwork-Agent-mcp MCP registered.") {
        "registered"
    } else if stdout.contains("\"profiles\"") {
        "status"
    } else if stdout.contains("\"url\"") {
        "settings"
    } else {
        "other"
    };
    Ok(
        json!({"timeout":timed,"exit_code":exit,"reaped":true,"rejection":rejection,"marker":marker}),
    )
}
pub async fn run(binary: &Path, output: &Path) -> Result<Value> {
    let mut rows = Vec::new();
    for (command, expected, marker) in [
        ("status", 0, "status"),
        ("local-settings", 0, "settings"),
        ("bridge", 3, "signin"),
        ("register", 0, "registered"),
    ] {
        let root = temporary(output, "state-control-")?;
        seed(root.path())?;
        let mut row = observe(binary, root.path(), command).await?;
        row["passed"] = json!(
            row["timeout"] == false && row["exit_code"] == expected && row["marker"] == marker
        );
        row["case"] = json!(format!("valid-{command}"));
        rows.push(row);
    }
    for kind in [
        "fifo",
        "directory",
        "oversized",
        "invalid_utf8",
        "ignored_utf8",
        "dangling",
    ] {
        for (relative, command) in [
            ("config/connections.json", "status"),
            ("config/local-proxy.json", "local-settings"),
            (".state/rust/credentials.json", "bridge"),
        ] {
            if kind == "ignored_utf8" && command != "status" {
                continue;
            }
            let root = temporary(output, "state-boundary-")?;
            seed(root.path())?;
            let path = root.path().join(relative);
            if path.exists() {
                fs::remove_file(&path)?;
            }
            match kind {
            "fifo"=>{tool("mkfifo",&[path.as_os_str()],2).await?;},
            "directory"=>fs::create_dir(&path)?,
            "oversized"=>fs::File::create(&path)?.set_len((LIMIT+1)as u64)?,
            "dangling"=>{symlink(root.path().join("absent-target"),&path)?;if command=="bridge"{
                let legacy=root.path().join(".state/auth/mcp-remote-v1");fs::create_dir_all(&legacy)?;
                use md5::{Digest,Md5};let name=format!("{:x}_tokens.json",Md5::digest(super::fixture::ENDPOINT.as_bytes()));fs::write(legacy.join(name),"{}")?;
            }},
            "ignored_utf8"=>fs::write(&path,b"{\"version\":1,\"mode\":\"direct\",\"selected\":null,\"profiles\":[],\"ignored\":\"\xff\"}")?,
            _=>fs::write(&path,b"\xff")?,
        }
            let mut row = observe(binary, root.path(), command).await?;
            let expected = if command == "bridge" {
                "auth_storage"
            } else if kind == "dangling" {
                "metadata"
            } else if command == "status" && matches!(kind, "invalid_utf8" | "ignored_utf8") {
                "utf8"
            } else if kind == "invalid_utf8" {
                "json"
            } else {
                "state"
            };
            row["passed"] = json!(
                row["timeout"] == false && row["exit_code"] == 1 && row["rejection"] == expected
            );
            row["case"] = json!(format!("{kind}-{command}"));
            rows.push(row);
        }
    }
    let root = temporary(output, "registration-boundary-")?;
    seed(root.path())?;
    let path = root.path().join(".mcp.json");
    let bytes = [b"{\"padding\":\"".as_slice(), &vec![b'x'; LIMIT], b"\"}"].concat();
    fs::write(&path, &bytes)?;
    let mut row = observe(binary, root.path(), "register").await?;
    let preserved = fs::read(path)? == bytes;
    row["original_preserved"] = json!(preserved);
    row["case"] = json!("oversized-valid-registration");
    row["passed"] = json!(
        preserved
            && row["timeout"] == false
            && row["exit_code"] == 1
            && row["rejection"] == "state"
    );
    rows.push(row);
    require(rows.len() == 21, "State case inventory drift")?;
    Ok(
        json!({"cases":rows.len(),"passed":rows.iter().filter(|v|v["passed"]==true).count(),"results":rows}),
    )
}
pub async fn import(binary: &Path, output: &Path) -> Result<Value> {
    let root = temporary(output, "import-scale-")?;
    let root = root.path();
    fs::create_dir(root.join("config"))?;
    let anchor =
        json!({"name":"anchor","url":"http://127.0.0.1:65534","label":"Preserve active route"});
    let path = root.join("config/connections.json");
    fs::write(
        &path,
        serde_json::to_vec(
            &json!({"version":1,"mode":"proxy","selected":"anchor","profiles":[anchor]}),
        )?,
    )?;
    let payload = (0..8192)
        .map(|i| format!("http://127.0.0.1:{}:u:p:pool\n", 1000 + i))
        .collect::<String>();
    require(payload.len() <= 1024 * 1024, "Import fixture too large")?;
    let source = root.join("synthetic-proxies.txt");
    fs::write(&source, &payload)?;
    let started = Instant::now();
    let (exit, stdout, timed, cpu_seconds) =
        super::child::import_measured(binary, root, &source).await?;
    let elapsed = started.elapsed().as_secs_f64();
    let state: Value = serde_json::from_slice(&fs::read(&path)?)?;
    let profiles = state["profiles"].as_array().ok_or("Missing profiles")?;
    let mut passed = exit == 0
        && !timed
        && String::from_utf8(stdout)?.trim()
            == "Imported 8192 proxy profiles; skipped 0 duplicates. Active route unchanged.";
    passed &= state["mode"] == "proxy"
        && state["selected"] == "anchor"
        && profiles.len() == 8193
        && profiles[0] == anchor;
    passed &= profiles.iter().skip(1).enumerate().all(|(i, p)| {
        p["name"]
            == if i == 0 {
                "pool".to_owned()
            } else {
                format!("pool-{}", i + 1)
            }
            && p["label"] == "pool"
    });
    passed &= fs::read(&source)? == payload.as_bytes();
    let original = fs::read(&path)?;
    let (repeat, stdout, timed, _) = super::child::import_measured(binary, root, &source).await?;
    let unchanged = repeat == 0
        && !timed
        && String::from_utf8(stdout)?.trim()
            == "Imported 0 proxy profiles; skipped 8192 duplicates. Active route unchanged."
        && fs::read(path)? == original;
    passed &= unchanged;
    Ok(
        json!({"cases":1,"passed":usize::from(passed),"results":[{"case":"colliding-labels-8192","passed":passed,"seconds":elapsed,"cpu_seconds":cpu_seconds,"profiles_requested":8192,"input_bytes":payload.len(),"expected_baseline_name_probes":8192*8193/2,"expected_fixed_name_probes":8192,"duplicate_reimport_unchanged":unchanged,"reaped":true}]}),
    )
}
