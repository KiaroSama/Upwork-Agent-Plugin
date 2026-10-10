use crate::{
    config::{self, Mode, Profile},
    error::{Error, Result},
    import, register,
};
use std::{
    io::{self, IsTerminal, Write},
    path::Path,
};

fn answer(prompt: &str, default: &str, secret: bool) -> Result<String> {
    print!("\n{prompt} [{}] {{back=0, quit=exit}}: ", default);
    io::stdout().flush()?;
    let mut value = String::new();
    if secret && io::stdin().is_terminal() {
        value = rpassword::read_password()?;
    } else {
        if io::stdin().read_line(&mut value)? == 0 {
            return Ok("exit".into());
        }
    }
    let value = value.trim();
    Ok(if value.is_empty() {
        default.to_owned()
    } else {
        value.to_owned()
    })
}
fn accent(text: &str) {
    if io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none() {
        println!("\x1b[1;38;2;155;225;255m{text}\x1b[0m");
    } else {
        println!("{text}");
    }
}
pub fn preview(root: &Path) -> Result<()> {
    let state = config::read(&root.join("config/connections.json"))?;
    accent("                           Upwork Agent Plugin");
    accent(&"=".repeat(80));
    let mode = match state.mode {
        Mode::Blocked => "blocked",
        Mode::Direct => "direct",
        Mode::Proxy => "proxy",
    };
    let selected = state
        .profiles
        .iter()
        .find(|p| Some(&p.name) == state.selected.as_ref())
        .map(|p| p.label.as_ref().unwrap_or(&p.name).as_str())
        .unwrap_or("none");
    println!(
        "  Connection: {mode} | Proxy: {selected} | Saved proxies: {}",
        state.profiles.len()
    );
    accent("\nUpwork Main menu:");
    for item in [
        "1. Select proxy [1]",
        "2. Import proxy list from file",
        "3. Add one proxy",
        "4. Direct connection (no proxy)",
        "5. Block connections",
        "6. Sign in to Upwork",
        "7. Register MCP in Claude",
    ] {
        println!("  {item}");
    }
    Ok(())
}
pub async fn run(root: &Path) -> Result<()> {
    let path = root.join("config/connections.json");
    loop {
        preview(root)?;
        let choice = answer("Selection", "1", false)?;
        if choice.eq_ignore_ascii_case("exit") {
            return Ok(());
        }
        let result=async {match choice.as_str(){
            "1"=>{
                let c=config::read(&path)?;
                if c.profiles.is_empty(){accent("No saved proxies. Import a list with option 2.");return Ok(());}
                for (i,p) in c.profiles.iter().enumerate(){println!("  {}. {}",i+1,p.label.as_ref().unwrap_or(&p.name));}
                let value=answer("Proxy number","0",false)?;
                if value=="0"{return Ok(());}
if value.eq_ignore_ascii_case("exit"){return Err(Error::Safe("MENU_EXIT"));}
                let p=value.parse::<usize>().ok().and_then(|n|n.checked_sub(1)).and_then(|n|c.profiles.get(n)).ok_or(Error::Safe("Enter a listed proxy number."))?;
                config::update(&path,|c|{c.mode=Mode::Proxy;c.selected=Some(p.name.clone());Ok(())})?;
            },
            "2"=>{let value=answer("Proxy list file path",&root.join("Proxy list.txt").to_string_lossy(),false)?;if value=="0"{return Ok(());}
if value.eq_ignore_ascii_case("exit"){return Err(Error::Safe("MENU_EXIT"));}let (added,skipped)=import::import_file(&path,&value)?;println!("Imported {added} proxy profiles; skipped {skipped} duplicates. Active route unchanged.");},
            "3"=>{let name=answer("Profile name (letters/digits/_/-)","",false)?;if name=="0"{return Ok(());}
if name.eq_ignore_ascii_case("exit"){return Err(Error::Safe("MENU_EXIT"));}let url=answer("Proxy URL (http/https/socks5)","",true)?;if url=="0"{return Ok(());}
if url.eq_ignore_ascii_case("exit"){return Err(Error::Safe("MENU_EXIT"));}config::update(&path,|c|{c.profiles.push(Profile{name,url,label:None});Ok(())})?;},
            "4"=>{let value=answer("Use direct connection without proxy?","Y",false)?;if value.eq_ignore_ascii_case("exit"){return Err(Error::Safe("MENU_EXIT"));}
if matches!(value.to_lowercase().as_str(),"y"|"yes"){config::update(&path,|c|{c.mode=Mode::Direct;c.selected=None;Ok(())})?;}},
            "5"=>{config::update(&path,|c|{c.mode=Mode::Blocked;c.selected=None;Ok(())})?;},
            "6"=>crate::bridge::run(root,crate::ENDPOINT,true).await?,
            "7"=>{register::register(root)?;println!("Project MCP registered. Reopen Claude and reconnect upwork-local.");},
            _=>return Err(Error::Safe("Enter a menu number from 1 to 7, or exit.")),
        }Ok::<(),Error>(())}.await;
        if let Err(Error::Safe("MENU_EXIT")) = result {
            return Ok(());
        }
        if let Err(error) = result {
            eprintln!("{error}");
        } else {
            println!("Operation complete. Reconnect MCP after a route change.");
        }
    }
}
