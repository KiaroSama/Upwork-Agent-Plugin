use super::{
    child::{Result, invoke, require},
    fixture::temporary,
};
use serde_json::{Value, json};
use std::{fs, path::Path};

pub async fn run(binary: &Path, output: &Path) -> Result<Value> {
    let work = temporary(output, "cli-probe-")?;
    let work = work.path();
    let mut rows = Vec::new();
    let mut processes = 0;
    macro_rules! check {($name:expr,$condition:expr)=>{rows.push(json!({"case":$name,"passed":$condition}));};}
    macro_rules! call {
        ($root:expr,$args:expr,$data:expr) => {{
            processes += 1;
            invoke(binary, $root, $args, $data, 4.0).await?
        }};
    }
    macro_rules! rejected {
        ($result:expr) => {{
            let result = $result;
            super::oracle::controlled_rejection(result.0, result.3)
        }};
    }
    let invalid: Vec<Vec<&str>> = vec![
        vec!["direct", "--help"],
        vec!["direct", "extra"],
        vec!["block", "extra"],
        vec!["bridge", "extra"],
        vec!["sign-in", "extra"],
        vec!["menu", "extra"],
        vec!["status", "extra"],
        vec!["preview", "extra"],
        vec!["desktop-config", "extra"],
        vec!["register", "extra"],
        vec!["--version", "extra"],
        vec!["select"],
        vec!["select", "missing", "extra"],
        vec!["import"],
        vec!["import", "missing", "extra"],
        vec!["add", "extra"],
        vec!["local-proxy", "extra"],
        vec!["local-settings", "wrong"],
        vec!["local-settings", "set", "extra"],
        vec!["cleanup", "unexpected"],
        vec!["cleanup", "--apply", "extra"],
        vec!["unknown"],
        vec!["--root"],
        vec!["--help"],
    ];
    for (index, args) in invalid.iter().enumerate() {
        let root = work.join(format!("invalid-{index}"));
        fs::create_dir(&root)?;
        let result = call!(&root, args, b"");
        check!(format!("invalid-{index}-fails"), rejected!(result));
        check!(
            format!("invalid-{index}-no-files"),
            fs::read_dir(root)?.next().is_none()
        );
    }
    let root = work.join("state");
    fs::create_dir(&root)?;
    check!(
        "blocked-default",
        serde_json::from_slice::<Value>(&call!(&root, &["status"], b"").1)?["mode"] == "blocked"
    );
    let b = call!(&root, &["bridge"], b"");
    check!("blocked-clean-stdout", rejected!(&b) && b.1.is_empty());
    check!("explicit-direct", call!(&root, &["direct"], b"").0 == 0);
    let b = call!(&root, &["bridge"], b"");
    check!(
        "no-session-clean-stdout",
        b.0 == 3 && b.1.is_empty() && !b.3
    );
    check!("restore-blocked", call!(&root, &["block"], b"").0 == 0);
    for (scheme, ports) in [
        ("http", [80, 8080]),
        ("https", [443, 8443]),
        ("socks5", [1080, 1595]),
        ("socks5h", [1080, 1595]),
    ] {
        for host in ["127.0.0.1", "[::1]", "proxy.example.invalid"] {
            for port in ports {
                let label = format!("{scheme}-{host}-{port}");
                let url = format!("{scheme}://fixture-user:fixture-pass@{host}:{port}");
                check!(
                    format!("{label}-write"),
                    call!(
                        &root,
                        &["local-settings", "set"],
                        &serde_json::to_vec(&json!({"url":url}))?
                    )
                    .0 == 0
                );
                let public = serde_json::from_slice::<Value>(
                    &call!(&root, &["local-settings"], b"").1,
                )?["url"]
                    .as_str()
                    .ok_or("Missing URL")?
                    .to_owned();
                check!(
                    format!("{label}-no-credentials"),
                    !public.contains("fixture-user") && !public.contains("fixture-pass")
                );
                check!(
                    format!("{label}-port"),
                    public.contains(&format!(":{port}"))
                );
                check!(
                    format!("{label}-roundtrip"),
                    call!(
                        &root,
                        &["local-settings", "set"],
                        &serde_json::to_vec(&json!({"url":public}))?
                    )
                    .0 == 0
                );
            }
        }
    }
    check!(
        "settings-do-not-select",
        serde_json::from_slice::<Value>(&call!(&root, &["status"], b"").1)?["mode"] == "blocked"
    );
    let text = work.join("proxies.txt");
    let source = text.to_str().ok_or("NonUTF8 path")?;
    fs::write(
        &text,
        "\u{feff}# test\nhttp://proxy.example.invalid:80:u:p:First\nhttps://proxy.example.invalid:443:u:p:Second\n",
    )?;
    check!(
        "import-valid",
        call!(&root, &["import", source], b"").0 == 0
    );
    let path = root.join("config/connections.json");
    let before = fs::read(&path)?;
    check!(
        "duplicate-import",
        call!(&root, &["import", source], b"").0 == 0
    );
    check!("duplicate-preservation", fs::read(&path)? == before);
    fs::write(
        &text,
        "http://proxy.example.invalid:8080:u:p:Good\nINVALID\n",
    )?;
    check!(
        "atomic-bad-import",
        rejected!(call!(&root, &["import", source], b"")) && fs::read(&path)? == before
    );
    fs::write(&text, [b"#".as_slice(), &vec![b'x'; 1024 * 1024]].concat())?;
    check!(
        "oversized-import",
        rejected!(call!(&root, &["import", source], b"")) && fs::read(&path)? == before
    );
    check!(
        "directory-import",
        rejected!(call!(&root, &["import", work.to_str().unwrap()], b""))
    );
    let fifo = work.join("fifo");
    super::child::tool("mkfifo", &[fifo.as_os_str()], 2).await?;
    let result = call!(&root, &["import", fifo.to_str().unwrap()], b"");
    check!("fifo-no-writer-refused", rejected!(result));
    check!(
        "failed-select-preserves",
        rejected!(call!(&root, &["select", "missing"], b"")) && fs::read(&path)? == before
    );
    for (index, value) in [
        json!([]),
        json!(false),
        json!(42),
        json!("fixture"),
        Value::Null,
        json!({"mcpServers":[]}),
    ]
    .iter()
    .enumerate()
    {
        let r = work.join(format!("json-{index}"));
        fs::create_dir(&r)?;
        let path = r.join(".mcp.json");
        fs::write(&path, serde_json::to_vec(value)?)?;
        let original = fs::read(&path)?;
        let result = call!(&r, &["register"], b"");
        check!(format!("json-{index}-controlled"), rejected!(result));
        check!(
            format!("json-{index}-preserved"),
            fs::read(&path)? == original
        );
    }
    let r = work.join("foreign");
    let foreign = work.join("another-root");
    fs::create_dir(&r)?;
    fs::create_dir(&foreign)?;
    let path = r.join(".mcp.json");
    fs::write(
        &path,
        serde_json::to_vec(
            &json!({"mcpServers":{"Upwork-Agent-mcp":{"command":binary,"args":["--root",foreign,"bridge"]},"other":{"command":"preserved"}}}),
        )?,
    )?;
    let original = fs::read(&path)?;
    check!(
        "foreign-registration-refused",
        rejected!(call!(&r, &["register"], b"")) && fs::read(&path)? == original
    );
    let r = work.join("owned");
    fs::create_dir(&r)?;
    fs::write(
        r.join(".mcp.json"),
        serde_json::to_vec(&json!({"mcpServers":{"other":{"command":"preserved"}}}))?,
    )?;
    check!("registration-first", call!(&r, &["register"], b"").0 == 0);
    let first = fs::read(r.join(".mcp.json"))?;
    let settings = fs::read(r.join(".claude/settings.local.json"))?;
    check!(
        "registration-idempotent",
        call!(&r, &["register"], b"").0 == 0
            && fs::read(r.join(".mcp.json"))? == first
            && fs::read(r.join(".claude/settings.local.json"))? == settings
    );
    check!(
        "other-registration-preserved",
        serde_json::from_slice::<Value>(&first)?["mcpServers"]["other"]["command"] == "preserved"
    );
    let r = work.join("linked");
    let outside = work.join("outside");
    fs::create_dir(&r)?;
    fs::create_dir(&outside)?;
    std::os::unix::fs::symlink(&outside, r.join(".claude"))?;
    check!(
        "linked-registration-refused",
        rejected!(call!(&r, &["register"], b""))
            && fs::read_dir(&outside)?.next().is_none()
            && !r.join(".mcp.json").exists()
    );
    let mut paths = vec![work.to_path_buf()];
    while let Some(path) = paths.pop() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                paths.push(entry.path());
            } else if kind.is_file() && entry.path().extension().is_some_and(|v| v == "log") {
                let text = fs::read_to_string(entry.path())?;
                check!(
                    format!(
                        "credential-log-{}",
                        entry.path().strip_prefix(work)?.display()
                    ),
                    !text.contains("fixture-user") && !text.contains("fixture-pass")
                );
            }
        }
    }
    require(rows.len() == 272, "CLI assertion inventory drift")?;
    let passed = rows.iter().filter(|r| r["passed"] == true).count();
    Ok(
        json!({"checks":rows.len(),"passed":passed,"failed":rows.len()-passed,"child_processes":processes,"results":rows}),
    )
}
