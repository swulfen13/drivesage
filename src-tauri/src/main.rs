//! DriveSage desktop app: starts the local DriveSage server in-process and
//! shows the same web UI in a native window. The UI also offers a button to
//! open it in the normal browser.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use drivesage_server::{start, ServerOptions};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let server = tauri::async_runtime::block_on(start(ServerOptions { port: 0 }))?;
            let url = server.url();
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.parse()?))
                .title("DriveSage")
                .inner_size(1320.0, 860.0)
                .min_inner_size(980.0, 640.0)
                .build()?;
            app.manage(server); // keep the server alive as long as the app runs
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running DriveSage");
}
