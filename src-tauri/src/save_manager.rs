use std::fs;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use std::time::UNIX_EPOCH;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::time::Duration;

pub fn get_bg3_save_dir() -> Result<PathBuf, String> {
    let dir = dirs::data_local_dir()
        .ok_or("Could not find Local AppData directory")?
        .join("Larian Studios/Baldur's Gate 3/PlayerProfiles/Public/Savegames/Story");
    Ok(dir)
}

pub fn get_backup_dir() -> Result<PathBuf, String> {
    let dir = dirs::document_dir()
        .ok_or("Could not find Documents directory")?
        .join("BG3_Backups");
    
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }
    
    Ok(dir)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SaveFolder {
    pub name: String,
    pub path: String,
    pub last_modified: u64,
}

#[tauri::command]
pub fn get_saves() -> Result<Vec<SaveFolder>, String> {
    let dir = get_bg3_save_dir()?;
    if !dir.exists() {
        return Ok(vec![]); 
    }
    scan_save_dir(&dir)
}

#[tauri::command]
pub fn get_backups() -> Result<Vec<SaveFolder>, String> {
    let dir = get_backup_dir()?;
    scan_save_dir(&dir)
}

fn scan_save_dir(dir: &PathBuf) -> Result<Vec<SaveFolder>, String> {
    let mut saves = vec![];
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;
    
    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() {
                let metadata = entry.metadata().map_err(|e| e.to_string())?;
                let mut last_modified = metadata.modified()
                    .unwrap_or(std::time::SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                // On Windows, in-place file modification doesn't always update the parent folder's modified time.
                // We must check the actual .lsv or .webp files inside to get the true latest save time.
                if let Ok(inner_entries) = fs::read_dir(&path) {
                    for inner_entry in inner_entries.flatten() {
                        if let Ok(inner_meta) = inner_entry.metadata() {
                            if let Ok(inner_mod) = inner_meta.modified() {
                                let inner_secs = inner_mod.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
                                if inner_secs > last_modified {
                                    last_modified = inner_secs;
                                }
                            }
                        }
                    }
                }
                    
                saves.push(SaveFolder {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: path.to_string_lossy().to_string(),
                    last_modified,
                });
            }
        }
    }
    
    saves.sort_by(|a, b| b.last_modified.cmp(&a.last_modified));
    Ok(saves)
}

#[tauri::command]
pub fn backup_save(save_name: String) -> Result<(), String> {
    let source_dir = get_bg3_save_dir()?.join(&save_name);
    if !source_dir.exists() {
        return Err("Save folder does not exist".to_string());
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_name = format!("{}____{}", save_name, timestamp);
    let backup_wrapper = get_backup_dir()?.join(&backup_name);

    fs::create_dir_all(&backup_wrapper).map_err(|e| e.to_string())?;

    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true;
    options.copy_inside = true;

    fs_extra::dir::copy(&source_dir, &backup_wrapper, &options).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn restore_backup(backup_name: String) -> Result<(), String> {
    let backup_wrapper = get_backup_dir()?.join(&backup_name);
    if !backup_wrapper.exists() {
        return Err("Backup folder does not exist".to_string());
    }

    // Determine if nested or direct
    let mut inner_save_dir = backup_wrapper.clone();
    let mut original_save_name = backup_name.clone();

    // Check if it's nested (has exactly one directory inside)
    let entries = fs::read_dir(&backup_wrapper).map_err(|e| e.to_string())?;
    for entry in entries {
        if let Ok(entry) = entry {
            if entry.path().is_dir() {
                // We found a directory inside, assume it's the actual save folder
                inner_save_dir = entry.path();
                original_save_name = entry.file_name().to_string_lossy().to_string();
                break;
            }
        }
    }

    let target_dir = get_bg3_save_dir()?.join(&original_save_name);
    
    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true;
    options.copy_inside = true; 

    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| e.to_string())?;
    }

    fs_extra::dir::copy(&inner_save_dir, get_bg3_save_dir()?, &options)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn delete_backup(backup_name: String) -> Result<(), String> {
    let backup_dir = get_backup_dir()?.join(&backup_name);
    if backup_dir.exists() {
        fs::remove_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct AppState {
    pub auto_backup_enabled: Arc<AtomicBool>,
}

#[tauri::command]
pub async fn toggle_auto_backup(
    state: tauri::State<'_, AppState>,
    app_handle: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let previously_enabled = state.auto_backup_enabled.swap(enabled, Ordering::SeqCst);
    
    // Only spawn the loop if it wasn't already running and we want it enabled.
    if enabled && !previously_enabled {
        let state_clone = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            auto_backup_loop(state_clone, app_handle).await;
        });
    }
    
    Ok(())
}

#[tauri::command]
pub fn get_auto_backup_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.auto_backup_enabled.load(Ordering::SeqCst))
}

async fn auto_backup_loop(state: AppState, _app: tauri::AppHandle) {
    let mut last_backed_up_time = 0;
    
    println!("Auto backup loop started.");
    
    while state.auto_backup_enabled.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_secs(2)).await;
        
        if !state.auto_backup_enabled.load(Ordering::SeqCst) {
            println!("Auto backup disabled, breaking loop.");
            break;
        }
        
        if let Ok(saves) = get_saves() {
            if let Some(latest) = saves.first() {
                if latest.last_modified > last_backed_up_time {
                    println!("Detected new/modified save: {} (Time: {} > {})", latest.name, latest.last_modified, last_backed_up_time);
                    
                    match backup_save(latest.name.clone()) {
                        Ok(_) => {
                            println!("Auto backup successful for {}", latest.name);
                            last_backed_up_time = latest.last_modified;
                        }
                        Err(e) => {
                            println!("Auto backup failed (locked by game?), will retry: {}", e);
                            // We do NOT update `last_backed_up_time` so it will retry securely in 2 seconds.
                        }
                    }
                }
            }
        }
    }
}
