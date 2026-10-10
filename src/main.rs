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
async fn execute() -> Result<()> {
    let (root, args) = root()?;
    let command = args.first().map(String::as_str).unwrap_or("menu");
    let log = Log::new(&root, "upwork-native");
    log.event("INFO", "Native executable started version=2.0.0.");
    let path = root.join("config/connections.json");
    let result = match command {
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
            println!("Native upwork-local MCP registered. Reopen Claude and reconnect.");
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
            println!("upwork-connector 2.0.0");
            Ok(())
        }
        _ => Err(Error::Safe(
            "Usage: upwork-connector [--root PATH] menu|status|import PATH|add|select NAME|direct|block|sign-in|bridge|register|desktop-config",
        )),
    };
    log.event(
        if result.is_ok() { "INFO" } else { "ERROR" },
        "Native executable stopped; private input suppressed.",
    );
    result
}
