use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

// ---------------------------------------------------------------------------
// Directory helpers
// ---------------------------------------------------------------------------

pub fn get_bg3_save_dir() -> Result<PathBuf, String> {
    let dir = dirs::data_local_dir()
        .ok_or("Could not find Local AppData directory")?
        .join("Larian Studios/Baldur's Gate 3/PlayerProfiles/Public/Savegames/Story");
    Ok(dir)
}

/// Returns the backup directory path, creating it if it doesn't already exist.
/// Named `ensure_*` to make the side-effect clear at call sites.
pub fn ensure_backup_dir() -> Result<PathBuf, String> {
    let dir = dirs::document_dir()
        .ok_or("Could not find Documents directory")?
        .join("BG3_Backups");

    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    Ok(dir)
}

/// Returns the backup directory path *without* any side effects.
pub fn get_backup_dir() -> Result<PathBuf, String> {
    let dir = dirs::document_dir()
        .ok_or("Could not find Documents directory")?
        .join("BG3_Backups");
    Ok(dir)
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Serialize, Deserialize, Clone)]
pub struct SaveFolder {
    pub name: String,
    pub path: String,
    pub last_modified: u64,
}

// ---------------------------------------------------------------------------
// Tauri commands — read operations
// ---------------------------------------------------------------------------

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
    if !dir.exists() {
        return Ok(vec![]);
    }
    scan_save_dir(&dir)
}

/// Accepts `&Path` (not `&PathBuf`) so it works with any path-like value.
fn scan_save_dir(dir: &Path) -> Result<Vec<SaveFolder>, String> {
    let mut saves = vec![];
    let entries = fs::read_dir(dir).map_err(|e| e.to_string())?;

    for entry in entries {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.is_dir() {
                let metadata = entry.metadata().map_err(|e| e.to_string())?;
                let mut last_modified = metadata
                    .modified()
                    .unwrap_or(std::time::SystemTime::now())
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();

                // On Windows, in-place file modification doesn't always update
                // the parent folder's modified time.  Check inner files instead.
                if let Ok(inner_entries) = fs::read_dir(&path) {
                    for inner_entry in inner_entries.flatten() {
                        if let Ok(inner_meta) = inner_entry.metadata() {
                            if let Ok(inner_mod) = inner_meta.modified() {
                                let inner_secs = inner_mod
                                    .duration_since(UNIX_EPOCH)
                                    .unwrap_or_default()
                                    .as_secs();
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

// ---------------------------------------------------------------------------
// Tauri commands — write operations
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn backup_save(save_name: String) -> Result<(), String> {
    let source_dir = get_bg3_save_dir()?.join(&save_name);
    if !source_dir.exists() {
        return Err("Save folder does not exist".to_string());
    }

    let backup_root = ensure_backup_dir()?;
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_name = format!("{}____{}", save_name, timestamp);

    // Write into a temporary directory first, then rename — this makes the
    // backup atomic: a partial copy can never be mistaken for a valid backup.
    let temp_dir = backup_root.join(format!(".tmp_{}", backup_name));
    let final_dir = backup_root.join(&backup_name);

    // Clean up any leftover temp dir from a previous crashed attempt.
    if temp_dir.exists() {
        fs::remove_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true;
    options.copy_inside = true;

    fs_extra::dir::copy(&source_dir, &temp_dir, &options)
        .map_err(|e| format!("Copy failed: {}", e))?;

    // Atomic promotion: rename temp → final destination.
    fs::rename(&temp_dir, &final_dir)
        .map_err(|e| format!("Could not finalise backup (rename failed): {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn restore_backup(backup_name: String) -> Result<(), String> {
    let backup_root = get_backup_dir()?;
    let backup_wrapper = backup_root.join(&backup_name);

    if !backup_wrapper.exists() {
        return Err(format!("Backup '{}' does not exist", backup_name));
    }

    // Derive the original save name from our naming convention:
    //   "<SaveName>____<Timestamp>"
    // This is reliable and avoids fragile directory enumeration.
    let original_save_name = backup_name
        .split("____")
        .next()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            format!(
                "Could not determine original save name from backup name: '{}'",
                backup_name
            )
        })?
        .to_string();

    // The actual save data sits one level below the wrapper folder.
    let inner_save_dir = backup_wrapper.join(&original_save_name);
    if !inner_save_dir.exists() {
        return Err(format!(
            "Backup appears to be corrupt — expected inner folder '{}' not found",
            inner_save_dir.display()
        ));
    }

    let save_root = get_bg3_save_dir()?;
    let target_dir = save_root.join(&original_save_name);

    // Remove the existing save before restoring.
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| e.to_string())?;
    }

    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true;
    options.copy_inside = true;

    fs_extra::dir::copy(&inner_save_dir, &save_root, &options)
        .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn delete_backup(backup_name: String) -> Result<(), String> {
    let backup_dir = get_backup_dir()?.join(&backup_name);
    if !backup_dir.exists() {
        return Err(format!("Backup '{}' not found", backup_name));
    }
    fs::remove_dir_all(&backup_dir).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Auto-backup state
// ---------------------------------------------------------------------------

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

    // Spawn the watcher loop only when transitioning from disabled → enabled.
    if enabled && !previously_enabled {
        let state_clone = state.inner().clone();
        tauri::async_runtime::spawn(async move {
            auto_backup_watcher(state_clone, app_handle).await;
        });
    }

    Ok(())
}

#[tauri::command]
pub fn get_auto_backup_status(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    Ok(state.auto_backup_enabled.load(Ordering::SeqCst))
}

// ---------------------------------------------------------------------------
// Event-driven auto-backup (replaces polling loop)
// ---------------------------------------------------------------------------

async fn auto_backup_watcher(state: AppState, _app: tauri::AppHandle) {
    let save_dir = match get_bg3_save_dir() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Auto backup: could not resolve save directory: {}", e);
            return;
        }
    };

    println!("Auto backup watcher started — watching {:?}", save_dir);

    // Channel that the debouncer sends batched events through.
    let (tx, mut rx) = tokio::sync::mpsc::channel(32);

    // Build a debouncer with a 500 ms quiet period so we don't fire mid-write.
    let mut debouncer = match new_debouncer(
        std::time::Duration::from_millis(500),
        move |res: notify_debouncer_mini::DebounceEventResult| {
            if let Ok(events) = res {
                // Only forward if there are actual events.
                if !events.is_empty() {
                    let _ = tx.blocking_send(events);
                }
            }
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("Auto backup: failed to create file watcher: {}", e);
            return;
        }
    };

    if let Err(e) = debouncer
        .watcher()
        .watch(&save_dir, RecursiveMode::Recursive)
    {
        eprintln!("Auto backup: failed to watch save directory: {}", e);
        return;
    }

    while state.auto_backup_enabled.load(Ordering::SeqCst) {
        // Wait up to 250 ms for an event, then re-check the enabled flag.
        let maybe_events =
            tokio::time::timeout(Duration::from_millis(250), rx.recv()).await;

        if !state.auto_backup_enabled.load(Ordering::SeqCst) {
            break;
        }

        if maybe_events.is_err() {
            // Timeout — no events, loop again to recheck the flag.
            continue;
        }

        // We received at least one debounced event.  Back up the most-recently
        // modified save.
        if let Ok(saves) = get_saves() {
            if let Some(latest) = saves.first() {
                println!(
                    "Auto backup: change detected, backing up '{}'",
                    latest.name
                );
                match backup_save(latest.name.clone()) {
                    Ok(_) => println!("Auto backup: success for '{}'", latest.name),
                    Err(e) => eprintln!(
                        "Auto backup: backup failed (game may still be writing): {}",
                        e
                    ),
                }
            }
        }
    }

    println!("Auto backup watcher stopped.");
    // `debouncer` is dropped here, which stops the underlying OS watcher.
}
