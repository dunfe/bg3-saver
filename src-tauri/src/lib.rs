pub mod save_manager;

use save_manager::{backup_save, delete_backup, get_backups, get_save_preview, get_saves, restore_backup};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                save_manager::auto_backup_watcher(app_handle).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_saves,
            get_backups,
            backup_save,
            restore_backup,
            delete_backup,
            get_save_preview,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
