pub mod save_manager;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use save_manager::{
    get_saves, get_backups, backup_save, restore_backup, delete_backup,
    toggle_auto_backup, get_auto_backup_status, AppState,
};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app_state = AppState {
        auto_backup_enabled: Arc::new(AtomicBool::new(false)),
    };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_saves,
            get_backups,
            backup_save,
            restore_backup,
            delete_backup,
            toggle_auto_backup,
            get_auto_backup_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
