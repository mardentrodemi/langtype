use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// User data directory. Older folders are renamed once, in place.
pub fn user_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let fresh = home.join(".local/share/langtype");
    if fresh.exists() {
        return fresh;
    }
    let share = home.join(".local/share");
    for legacy_name in ["ruentype", "typecoach"] {
        let legacy = share.join(legacy_name);
        if !legacy.exists() {
            continue;
        }
        let _ = fs::create_dir_all(&share);
        if fs::rename(&legacy, &fresh).is_err() {
            return legacy;
        }
        return fresh;
    }
    fresh
}

pub fn write_json(path: &Path, value: &impl Serialize) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(value) {
        let _ = fs::write(path, format!("{text}\n"));
    }
}
