//! Claude P2P Remote Mobile App - Tauri v2 Rust backend

use tauri::Manager;

/// Run the application
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
