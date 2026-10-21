#[cfg(target_os = "linux")]
mod child;
#[cfg(target_os = "linux")]
mod cli;
#[cfg(target_os = "linux")]
mod controller;
#[cfg(target_os = "linux")]
mod coordination;
#[cfg(target_os = "linux")]
mod fixture;
#[cfg(target_os = "linux")]
mod initialization;
#[cfg(target_os = "linux")]
mod oracle;
#[cfg(target_os = "linux")]
mod ordinary;
#[cfg(target_os = "linux")]
mod protocol;
#[cfg(target_os = "linux")]
mod state;

#[cfg(target_os = "linux")]
#[tokio::main(flavor = "current_thread")]
async fn main() {
    if controller::main().await.is_err() {
        eprintln!("Native acceptance failed; inspect the sanitized active-suite receipt.");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Packaged native certificate and observed-stop fixtures require Linux.");
    std::process::exit(1);
}
