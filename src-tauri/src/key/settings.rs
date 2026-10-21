//! The helper's settings file: `{ "formatVersion": 1, "staging": true }`. It holds no
//! secrets. Other entries in it are kept as they are when the staging setting is written.

use serde_json::{json, Map, Value};
use std::fs;
use std::io;
use std::path::Path;
use std::sync::Mutex;

const FORMAT_VERSION: i64 = 1;

/// Writes read the file and put it back, so two at once could lose one; they take turns.
static WRITING: Mutex<()> = Mutex::new(());

/// Whether the staging setting is on. A missing or unreadable file means off, so the
/// helper falls back to production.
pub(crate) fn read_staging(path: &Path) -> bool {
    read(path)
        .and_then(|settings| settings.get("staging").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// Writes the staging setting. A crash mid-write leaves either the old file or the new one.
pub(crate) fn write_staging(path: &Path, on: bool) -> io::Result<()> {
    write_entry(path, "staging", json!(on))
}

/// The remembered game folder. A missing or unreadable file, or no entry, means none.
pub(crate) fn read_game_folder(path: &Path) -> Option<String> {
    read(path)?
        .get("gameFolder")
        .and_then(Value::as_str)
        .map(str::to_string)
}

pub(crate) fn write_game_folder(path: &Path, folder: &str) -> io::Result<()> {
    write_entry(path, "gameFolder", json!(folder))
}

/// Whether the officer (or first setup) has decided on starting at login: `None` until then.
pub(crate) fn read_start_at_login(path: &Path) -> Option<bool> {
    read(path)?.get("startAtLogin").and_then(Value::as_bool)
}

pub(crate) fn write_start_at_login(path: &Path, on: bool) -> io::Result<()> {
    write_entry(path, "startAtLogin", json!(on))
}

/// Sets one entry, keeping the others, through a temporary file renamed over the old one.
fn write_entry(path: &Path, name: &str, value: Value) -> io::Result<()> {
    let _turn = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut settings = read(path).unwrap_or_default();
    settings.insert("formatVersion".into(), json!(FORMAT_VERSION));
    settings.insert(name.into(), value);
    let bytes = serde_json::to_vec_pretty(&settings).expect("JSON values always serialize");
    let mut temp_name = path.file_name().unwrap_or_default().to_os_string();
    temp_name.push(".tmp");
    let temp = path.with_file_name(temp_name);
    let written = fs::write(&temp, bytes).and_then(|()| fs::rename(&temp, path));
    if written.is_err() {
        let _ = fs::remove_file(&temp);
    }
    written
}

/// The settings, if the file holds a version this helper knows.
fn read(path: &Path) -> Option<Map<String, Value>> {
    let bytes = fs::read(path).ok()?;
    match serde_json::from_slice(&bytes).ok()? {
        Value::Object(settings)
            if settings.get("formatVersion") == Some(&json!(FORMAT_VERSION)) =>
        {
            Some(settings)
        }
        _ => None,
    }
}
