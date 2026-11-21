//! `drivesage-web`: starts DriveSage as a local web app and opens the browser.

use drivesage_server::{start, ServerOptions};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args: Vec<String> = std::env::args().collect();
    let port = args
        .iter()
        .position(|a| a == "--port")
        .and_then(|i| args.get(i + 1))
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let no_open = args.iter().any(|a| a == "--no-open");

    let server = start(ServerOptions { port }).await?;
    let url = server.url();
    println!("\n  DriveSage läuft:  {url}\n  (Strg+C zum Beenden)\n");
    if !no_open {
        let _ = open::that_detached(&url);
    }
    tokio::signal::ctrl_c().await?;
    Ok(())
}
