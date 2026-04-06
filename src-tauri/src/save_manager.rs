use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use std::sync::{Mutex, OnceLock};
use std::time::{Instant, UNIX_EPOCH};

use notify::RecursiveMode;
use notify_debouncer_mini::new_debouncer;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

static RECENT_RESTORES: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();

fn mark_restored(save_name: &str) {
    if let Ok(mut map) = RECENT_RESTORES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        map.insert(save_name.to_string(), Instant::now());
    }
}

fn recently_restored(save_name: &str) -> bool {
    if let Ok(mut map) = RECENT_RESTORES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
    {
        if let Some(&time) = map.get(save_name) {
            if time.elapsed() < std::time::Duration::from_secs(30) {
                return true;
            } else {
                map.remove(save_name);
            }
        }
    }
    false
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

// ---------------------------------------------------------------------------
// Notes helpers
// ---------------------------------------------------------------------------

fn read_notes(backup_dir: &Path) -> Option<String> {
    let notes_path = backup_dir.join("notes.json");
    if notes_path.exists() {
        fs::read_to_string(&notes_path).ok().and_then(|content| {
            serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|v| v.get("notes").and_then(|n| n.as_str()).map(String::from))
        })
    } else {
        None
    }
}

fn write_notes(backup_dir: &Path, notes: &str) -> Result<(), String> {
    let notes_path = backup_dir.join("notes.json");
    let content = serde_json::json!({ "notes": notes });
    fs::write(
        &notes_path,
        serde_json::to_string_pretty(&content).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
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
                        notes: read_notes(&path),
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
pub fn backup_save(save_name: String) -> Result<bool, String> {
    let source_dir = get_bg3_save_dir()?.join(&save_name);
    if !source_dir.exists() {
        return Err("Save folder does not exist".to_string());
    }

    let backup_root = ensure_backup_dir()?;

    // Check for duplicate against the most recent backup
    if let Ok(backups) = get_backups() {
        if let Some(latest) = backups
            .iter()
            .find(|b| b.name.starts_with(&format!("{}__TS__", save_name)))
        {
            let source_path = source_dir.to_string_lossy().to_string();
            match (
                get_save_preview(source_path),
                get_save_preview(latest.path.clone()),
            ) {
                (Ok(source_preview), Ok(backup_preview)) => {
                    log_debug(&format!(
                        "Comparing previews for '{}': source {} bytes, backup {} bytes",
                        save_name,
                        source_preview.len(),
                        backup_preview.len()
                    ));
                    if source_preview == backup_preview {
                        // The active save matches the latest backup state.
                        log_debug(&format!(
                            "Skipping backup for '{}': preview image matches latest backup '{}'",
                            save_name, latest.name
                        ));
                        return Ok(false);
                    } else {
                        log_debug(&format!("Previews differ for '{}'", save_name));
                    }
                }
                (Err(e1), _) => log_debug(&format!(
                    "Failed to read source preview for '{}': {}",
                    save_name, e1
                )),
                (_, Err(e2)) => log_debug(&format!(
                    "Failed to read backup preview for '{}': {}",
                    latest.name, e2
                )),
            }
        } else {
            log_debug(&format!("No previous backup found for '{}'", save_name));
        }
    } else {
        log_debug("Failed to get backups list");
    }

    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
    let backup_name = format!("{}__TS__{}", save_name, timestamp); // Fix 7: resilient delimiter

    // Write into a temporary directory first, then rename — this makes the
    // backup atomic: a partial copy can never be mistaken for a valid backup.

    // Fix 2: Add random suffix to temp dir to avoid concurrent races
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
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

    Ok(true)
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
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    let temp_target = save_root.join(format!(".tmp_restore_{}_{}", original_save_name, nanos));

    let mut options = fs_extra::dir::CopyOptions::new();
    options.overwrite = true;
    options.content_only = true;

    fs::create_dir_all(&temp_target).map_err(|e| e.to_string())?;

    // Copy out from backup into temporary target
    fs_extra::dir::copy(&inner_save_dir, &temp_target, &options).map_err(|e| {
        let _ = fs::remove_dir_all(&temp_target);
        format!("Copy failed: {}", e)
    })?;

    // Atomic promotion: delete old save and immediately swap in the temp save
    if target_dir.exists() {
        fs::remove_dir_all(&target_dir).map_err(|e| format!("Failed to remove old save: {}", e))?;
    }

    fs::rename(&temp_target, &target_dir)
        .map_err(|e| format!("Could not finalize restore: {}", e))?;

    mark_restored(&original_save_name);

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

#[tauri::command]
pub fn update_backup_notes(backup_name: String, notes: String) -> Result<(), String> {
    let backup_dir = get_backup_dir()?.join(&backup_name);
    if !backup_dir.exists() {
        return Err(format!("Backup '{}' not found", backup_name));
    }
    write_notes(&backup_dir, &notes)
}

#[tauri::command]
pub fn get_save_preview(path: String) -> Result<Vec<u8>, String> {
    let folder_path = Path::new(&path);
    if !folder_path.exists() || !folder_path.is_dir() {
        return Err("Save path is invalid".to_string());
    }

    // Check directly in the folder
    if let Ok(entries) = fs::read_dir(folder_path) {
        for entry_res in entries {
            if let Ok(entry) = entry_res {
                if entry
                    .path()
                    .extension()
                    .and_then(|s| s.to_str())
                    .map(|s| s.eq_ignore_ascii_case("webp"))
                    .unwrap_or(false)
                {
                    return fs::read(entry.path()).map_err(|e| e.to_string());
                }
            }
        }
    }

    // If it's a backup, it has an outer wrapper, so the save is one folder deeper.
    if let Ok(entries) = fs::read_dir(folder_path) {
        for entry_res in entries {
            if let Ok(entry) = entry_res {
                if entry.path().is_dir() {
                    if let Ok(inner_entries) = fs::read_dir(entry.path()) {
                        for inner_entry_res in inner_entries {
                            if let Ok(inner_entry) = inner_entry_res {
                                if inner_entry
                                    .path()
                                    .extension()
                                    .and_then(|s| s.to_str())
                                    .map(|s| s.eq_ignore_ascii_case("webp"))
                                    .unwrap_or(false)
                                {
                                    return fs::read(inner_entry.path()).map_err(|e| e.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Err("No webp preview found".to_string())
}

// ---------------------------------------------------------------------------
// Event-driven auto-backup
// ---------------------------------------------------------------------------

fn log_debug(msg: &str) {
    if let Ok(dir) = get_backup_dir() {
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("auto_backup_debug.log"))
        {
            use std::io::Write;
            let _ = writeln!(f, "[{}] {}", chrono::Local::now().format("%H:%M:%S"), msg);
        }
    }
}

pub async fn auto_backup_watcher(_app: tauri::AppHandle) {
    let save_dir = match get_bg3_save_dir() {
        Ok(d) => d,
        Err(e) => {
            log_debug(&format!(
                "Auto backup: could not resolve save directory: {}",
                e
            ));
            return;
        }
    };

    if !save_dir.exists() {
        if let Err(e) = fs::create_dir_all(&save_dir) {
            log_debug(&format!("Auto backup: cannot create save directory: {}", e));
            return;
        }
    }

    log_debug(&format!(
        "Auto backup permanent watcher started — watching {:?}",
        save_dir
    ));

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
            log_debug(&format!(
                "Auto backup: failed to create file watcher: {}",
                e
            ));
            return;
        }
    };

    if let Err(e) = debouncer
        .watcher()
        .watch(&save_dir, RecursiveMode::Recursive)
    {
        log_debug(&format!(
            "Auto backup: failed to watch save directory: {}",
            e
        ));
        return;
    }

    let mut last_backup: Option<Instant> = None;

    loop {
        let maybe_events = tokio::time::timeout(Duration::from_millis(3000), rx.recv()).await;

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
                log_debug(&format!(
                    "Auto backup: cooldown active ({} s), skipping burst.",
                    COOLDOWN_SECS
                ));
                continue;
            }
        }

        let save_dir_len = save_dir.components().count();
        let affected: HashSet<String> = events
            .iter()
            .filter_map(|e| {
                let c_path = e
                    .path
                    .components()
                    .nth(save_dir_len)
                    .and_then(|c| c.as_os_str().to_str())
                    .map(|s| s.to_string());
                log_debug(&format!(
                    "Event path: {:?} => resolved to: {:?}",
                    e.path, c_path
                ));
                c_path
            })
            .collect();

        if affected.is_empty() {
            log_debug("No valid save folders determined from events.");
            continue;
        }

        log_debug(&format!(
            "Auto backup: {} event(s) affecting {} save folder(s): {:?}",
            events.len(),
            affected.len(),
            affected
        ));

        let mut any_backed_up = false;
        for save_name in &affected {
            if save_name.starts_with(".tmp_") {
                continue;
            }

            if recently_restored(save_name) {
                log_debug(&format!(
                    "Auto backup: skipping backup for '{}' because it was just restored.",
                    save_name
                ));
                continue;
            }

            match backup_save(save_name.clone()) {
                Ok(backed_up) => {
                    if backed_up {
                        log_debug(&format!(
                            "Auto backup: successfully backed up '{}'",
                            save_name
                        ));
                        any_backed_up = true;
                    } else {
                        log_debug(&format!(
                            "Auto backup: skipped duplicate backup for '{}'",
                            save_name
                        ));
                    }
                }
                Err(e) => {
                    log_debug(&format!(
                        "Auto backup: backup failed for '{}': {}",
                        save_name, e
                    ));
                }
            }
        }

        if any_backed_up {
            last_backup = Some(Instant::now());
        }
    }

    log_debug("Auto backup watcher stopped.");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_and_read_notes() {
        let temp_dir = std::env::temp_dir().join("bg3_notes_test");
        fs::create_dir_all(&temp_dir).unwrap();

        write_notes(&temp_dir, "My backup notes").unwrap();
        let notes = read_notes(&temp_dir);
        assert_eq!(notes, Some("My backup notes".to_string()));

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_read_notes_returns_none_when_missing() {
        let temp_dir = std::env::temp_dir().join("bg3_notes_test_empty");
        fs::create_dir_all(&temp_dir).unwrap();

        let notes = read_notes(&temp_dir);
        assert_eq!(notes, None);

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn test_write_notes_overwrites() {
        let temp_dir = std::env::temp_dir().join("bg3_notes_test_overwrite");
        fs::create_dir_all(&temp_dir).unwrap();

        write_notes(&temp_dir, "First notes").unwrap();
        write_notes(&temp_dir, "Updated notes").unwrap();
        let notes = read_notes(&temp_dir);
        assert_eq!(notes, Some("Updated notes".to_string()));

        fs::remove_dir_all(&temp_dir).unwrap();
    }
}

