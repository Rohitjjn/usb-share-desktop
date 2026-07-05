#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::sync::{Arc, Mutex};
use app_lib::server::{AppState, ServerState};
use app_lib::commands::ServerHandle;

fn main() {
    let state = AppState {
        state: Arc::new(Mutex::new(ServerState {
            root_path: std::path::PathBuf::from("."),
            credentials: None,
        })),
    };

    let handle = ServerHandle {
        abort_tx: Mutex::new(None),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .manage(handle)
        .invoke_handler(tauri::generate_handler![
            app_lib::commands::start_local_server,
            app_lib::commands::stop_local_server,
            app_lib::commands::get_local_ip,
            app_lib::commands::discover_phone_cmd
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
