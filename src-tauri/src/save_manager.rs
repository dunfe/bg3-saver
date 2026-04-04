use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, UNIX_EPOCH};

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

fn get_backup_root() -> Result<PathBuf, String> {
    dirs::document_dir()
        .ok_or_else(|| "Could not find Documents directory".to_string())
        .map(|d| d.join("BG3_Backups"))
}

/// Returns the backup directory path, creating it if it doesn't already exist.
/// Named `ensure_*` to make the side-effect clear at call sites.
pub fn ensure_backup_dir() -> Result<PathBuf, String> {
    let dir = get_backup_root()?;

    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    }

    Ok(dir)
}

/// Returns the backup directory path *without* any side effects.
pub fn get_backup_dir() -> Result<PathBuf, String> {
    get_backup_root()
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
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if path.is_dir() {
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(e) => {
                            eprintln!("Failed to read metadata for {:?}: {}", path, e);
                            continue;
                        }
                    };
                    
                    let mut last_modified = metadata
                        .modified()
                        .unwrap_or(std::time::UNIX_EPOCH) // Fix 6: fallback sorting
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
            Err(e) => {
                eprintln!("Failed to read directory entry: {}", e);
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
    let backup_name = format!("{}__TS__{}", save_name, timestamp); // Fix 7: resilient delimiter

    // Write into a temporary directory first, then rename — this makes the
    // backup atomic: a partial copy can never be mistaken for a valid backup.
    
    // Fix 2: Add random suffix to temp dir to avoid concurrent races
    let nanos = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    let temp_dir = backup_root.join(format!(".tmp_{}_{}", backup_name, nanos));
    let final_dir = backup_root.join(&backup_name);

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
    //   "<SaveName>__TS__<Timestamp>"
    // This is reliable and avoids fragile directory enumeration.
    let original_save_name = backup_name
        .split("__TS__") // Fix 7
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

    // Fix 1: Atomic Restoration
    let nanos = std::time::SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().subsec_nanos();
    let temp_target = save_root.join(format!(".tmp_restore_{}_{}", original_save_name, nanos));

    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true;
    options.copy_inside = true;

    fs::create_dir_all(&temp_target).map_err(|e| e.to_string())?;
    
    // Copy out from backup into temporary target
    fs_extra::dir::copy(&inner_save_dir, &temp_target, &options)
        .map_err(|e| {
            let _ = fs::remove_dir_all(&temp_target);
            format!("Copy failed: {}", e)
        })?;

    // Atomic promotion: delete old save and immediately swap in the temp save
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| format!("Failed to remove old save: {}", e))?;
    }
    
    fs::rename(&temp_target, &target_dir)
        .map_err(|e| format!("Could not finalize restore: {}", e))?;

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
// Event-driven auto-backup 
// ---------------------------------------------------------------------------

fn log_debug(msg: &str) {
    if let Ok(dir) = get_backup_dir() {
        if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(dir.join("auto_backup_debug.log")) {
            use std::io::Write;
            let _ = writeln!(f, "[{}] {}", chrono::Local::now().format("%H:%M:%S"), msg);
        }
    }
}

pub async fn auto_backup_watcher(_app: tauri::AppHandle) {
    let save_dir = match get_bg3_save_dir() {
        Ok(d) => d,
        Err(e) => {
            log_debug(&format!("Auto backup: could not resolve save directory: {}", e));
            return;
        }
    };

    if !save_dir.exists() {
        if let Err(e) = fs::create_dir_all(&save_dir) {
            log_debug(&format!("Auto backup: cannot create save directory: {}", e));
            return;
        }
    }

    log_debug(&format!("Auto backup permanent watcher started — watching {:?}", save_dir));

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

    let tx_clone = tx.clone();
    let mut debouncer = match new_debouncer(
        std::time::Duration::from_millis(2000),
        move |res: notify_debouncer_mini::DebounceEventResult| {
            if let Ok(events) = res {
                if !events.is_empty() {
                    let _ = tx_clone.send(events);
                }
            } else {
                let _ = tx_clone.send(vec![]);
            }
        },
    ) {
        Ok(d) => d,
        Err(e) => {
            log_debug(&format!("Auto backup: failed to create file watcher: {}", e));
            return;
        }
    };

    if let Err(e) = debouncer
        .watcher()
        .watch(&save_dir, RecursiveMode::Recursive)
    {
        log_debug(&format!("Auto backup: failed to watch save directory: {}", e));
        return;
    }

    let mut last_backup: Option<Instant> = None;

    loop {
        let maybe_events =
            tokio::time::timeout(Duration::from_millis(3000), rx.recv()).await;

        let events = match maybe_events {
            Err(_) => continue,
            Ok(None) => {
                log_debug("Auto backup: event channel closed unexpectedly.");
                break;
            }
            Ok(Some(evts)) => evts,
        };
        
        if events.is_empty() {
            log_debug("Auto backup: Debounce result contained an error.");
            continue;
        }

        log_debug(&format!("Received {} debounced events", events.len()));

        const COOLDOWN_SECS: u64 = 10;
        if let Some(last) = last_backup {
            if last.elapsed() < std::time::Duration::from_secs(COOLDOWN_SECS) {
                log_debug(&format!("Auto backup: cooldown active ({} s), skipping burst.", COOLDOWN_SECS));
                continue;
            }
        }

        let save_dir_len = save_dir.components().count();
        let affected: HashSet<String> = events
            .iter()
            .filter_map(|e| {
                let c_path = e.path.components().nth(save_dir_len).and_then(|c| c.as_os_str().to_str()).map(|s| s.to_string());
                log_debug(&format!("Event path: {:?} => resolved to: {:?}", e.path, c_path));
                c_path
            })
            .collect();

        if affected.is_empty() {
            log_debug("No valid save folders determined from events.");
            continue;
        }

        log_debug(&format!("Auto backup: {} event(s) affecting {} save folder(s): {:?}", events.len(), affected.len(), affected));

        for save_name in &affected {
            if save_name.starts_with(".tmp_") {
                continue;
            }

            match backup_save(save_name.clone()) {
                Ok(_) => {
                    log_debug(&format!("Auto backup: success for '{}'", save_name));
                }
                Err(e) => {
                    log_debug(&format!("Auto backup: backup failed for '{}': {}", save_name, e));
                }
            }
        }

        last_backup = Some(Instant::now());
    }

    log_debug("Auto backup watcher stopped.");
}
