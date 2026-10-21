use std::{
    io::{self, Read},
    path::PathBuf,
};
use upwork_connector::{
    config::{self, Mode, Profile},
    error::{Error, Result},
    logging::Log,
};

fn root() -> Result<(PathBuf, Vec<String>)> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    let root = if args.first().is_some_and(|s| s == "--root") {
        if args.len() < 2 {
            return Err(Error::Safe("--root requires a project folder."));
        }
        let root = PathBuf::from(args.remove(1));
        args.remove(0);
        root
    } else {
        let exe = std::env::current_exe()?;
        let parent = exe
            .parent()
            .ok_or(Error::Safe("Cannot resolve executable location."))?;
        if parent.file_name().is_some_and(|n| n == "bin") {
            parent
                .parent()
                .ok_or(Error::Safe("Cannot resolve project root."))?
                .into()
        } else {
            std::env::current_dir()?
        }
    };
    Ok((std::fs::canonicalize(root)?, args))
}
#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Never install upstream tracing subscribers: SDK debug events can contain OAuth secrets.
    std::panic::set_hook(Box::new(|_| {
        eprintln!("Native connector stopped after an internal error; private details suppressed.")
    }));
    let code = match execute().await {
        Ok(()) => 0,
        Err(error) => {
            eprintln!("{error}");
            match error {
                Error::Safe(s) if s.starts_with("Route changed") => 4,
                Error::Safe(s) if s.starts_with("Upwork sign-in required") => 3,
                _ => 1,
            }
        }
    };
    std::process::exit(code);
}
fn validated_command(args: &[String]) -> Result<&str> {
    let command = args.first().map(String::as_str).unwrap_or("menu");
    let valid = match command {
        "menu" => args.len() <= 1,
        "status" | "add" | "local-proxy" | "direct" | "block" | "sign-in" | "bridge"
        | "preview" | "register" | "desktop-config" | "--version" => args.len() == 1,
        "import" | "select" => args.len() == 2 && !args[1].is_empty(),
        "local-settings" => args.len() == 1 || (args.len() == 2 && args[1] == "set"),
        "cleanup" => args.len() == 1 || (args.len() == 2 && args[1] == "--apply"),
        _ => false,
    };
    if !valid {
        return Err(Error::Safe(
            "Invalid command arguments. Use --root PATH before the command; extra arguments are not accepted.",
        ));
    }
    Ok(command)
}

async fn execute() -> Result<()> {
    let (root, args) = root()?;
    // Validate the complete invocation before creating state, logging or networking.
    let command = validated_command(&args)?;
    upwork_connector::register::prepare_cache(&root)?;
    let log = Log::new(&root, "upwork-native");
    log.event("INFO", "Native executable started version=2.1.13.");
    let result = dispatch(root, &args, command, &log).await;
    if let Err(Error::Auth(error)) = &result {
        log.event(
            "ERROR",
            &format!(
                "OAuth failure class={}",
                upwork_connector::diagnostics::auth(error)
            ),
        );
    }
    log.event(
        if result.is_ok() { "INFO" } else { "ERROR" },
        "Native executable stopped; private input suppressed.",
    );
    result
}

async fn dispatch(root: PathBuf, args: &[String], command: &str, log: &Log) -> Result<()> {
    let cleanup_command = command == "cleanup";
    let _maintenance = if cleanup_command {
        Some(upwork_connector::cleanup::lease(&root, true)?)
    } else {
        if let Ok(_exclusive) = upwork_connector::cleanup::lease(&root, true) {
            match upwork_connector::cleanup::sweep(&root, true, std::time::SystemTime::now()) {
                Ok(report) if report.removed > 0 => {
                    log.event("INFO", "Stale connector temporary files removed.")
                }
                Err(_) => log.event("WARNING", "Temporary cleanup skipped safely."),
                _ => {}
            }
        }
        let mut runtime = None;
        for _ in 0..20 {
            if let Ok(file) = upwork_connector::cleanup::lease(&root, false) {
                runtime = Some(file);
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        Some(runtime.ok_or(Error::Safe("Maintenance is busy. Retry after it finishes."))?)
    };
    let path = root.join("config/connections.json");
    match command {
        "cleanup" => {
            let apply = match args.get(1).map(String::as_str) {
                None if args.len() == 1 => false,
                Some("--apply") if args.len() == 2 => true,
                _ => {
                    return Err(Error::Safe(
                        "Use cleanup or cleanup --apply; only stale connector-owned temporary files are eligible.",
                    ));
                }
            };
            let report =
                upwork_connector::cleanup::sweep(&root, apply, std::time::SystemTime::now())?;
            println!("{}", serde_json::to_string(&report)?);
            Ok(())
        }
        "bridge" => upwork_connector::bridge::run(&root, upwork_connector::ENDPOINT, false).await,
        "sign-in" => upwork_connector::bridge::run(&root, upwork_connector::ENDPOINT, true).await,
        "menu" => upwork_connector::menu::run(&root).await,
        "preview" => upwork_connector::menu::preview(&root),
        "status" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&config::read(&path)?.status()?)?
            );
            Ok(())
        }
        "import" => {
            let input = args.get(1).ok_or(Error::Safe("Enter a proxy file path."))?;
            let (imported, skipped) = upwork_connector::import::import_file(&path, input)?;
            println!(
                "Imported {imported} proxy profiles; skipped {skipped} duplicates. Active route unchanged."
            );
            Ok(())
        }
        "add" => {
            let mut input = String::new();
            io::stdin().take(8193).read_to_string(&mut input)?;
            if input.len() > 8192 {
                return Err(Error::Safe("Proxy input too large."));
            }
            let profile: Profile = serde_json::from_str(input.trim_start_matches('\u{feff}'))?;
            config::update(&path, |c| {
                c.profiles.push(profile);
                Ok(())
            })?;
            println!("Proxy saved.");
            Ok(())
        }
        "local-settings" => {
            match args.get(1).map(String::as_str) {
                None => println!(
                    "{}",
                    serde_json::json!({"url":upwork_connector::settings::read(&root)?.public_url()?})
                ),
                Some("set") => {
                    let mut input = String::new();
                    io::stdin().take(8193).read_to_string(&mut input)?;
                    if input.len() > 8192 {
                        return Err(Error::Safe("Local proxy input too large."));
                    }
                    let value = serde_json::from_str(input.trim_start_matches('\u{feff}'))?;
                    upwork_connector::settings::save(&root, value)?;
                    println!(
                        "Local proxy settings saved. Select local-proxy and reconnect MCP to apply."
                    );
                }
                _ => {
                    return Err(Error::Safe(
                        "Use local-settings or local-settings set with JSON on stdin.",
                    ));
                }
            }
            Ok(())
        }
        "local-proxy" => {
            upwork_connector::settings::select(&root)?;
            println!("Local proxy selected. Reconnect MCP to apply.");
            Ok(())
        }
        "select" | "direct" | "block" => {
            config::update(&path, |c| {
                c.mode = match command {
                    "select" => Mode::Proxy,
                    "direct" => Mode::Direct,
                    _ => Mode::Blocked,
                };
                c.selected = if command == "select" {
                    Some(
                        args.get(1)
                            .ok_or(Error::Safe("Enter a saved profile name."))?
                            .clone(),
                    )
                } else {
                    None
                };
                Ok(())
            })?;
            println!("Route saved. Active connector stops; reconnect MCP to apply.");
            Ok(())
        }
        "register" => {
            upwork_connector::register::register(&root)?;
            println!("Native Upwork-Agent-mcp MCP registered. Reopen Claude and reconnect.");
            Ok(())
        }
        "desktop-config" => {
            println!(
                "{}",
                serde_json::to_string_pretty(&upwork_connector::register::desktop(&root)?)?
            );
            Ok(())
        }
        "--version" => {
            println!("upwork-connector 2.1.13");
            Ok(())
        }
        _ => Err(Error::Safe(
            "Usage: upwork-connector [--root PATH] menu|status|import PATH|add|select NAME|local-settings [set]|local-proxy|direct|block|sign-in|bridge|register|desktop-config",
        )),
    }
}
