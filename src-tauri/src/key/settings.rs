//! The helper's settings file: `{ "formatVersion": 1, "staging": true }`. It holds no
//! secrets. Other entries in it are kept as they are when the staging setting is written.

use serde_json::{json, Map, Value};
use std::fs;
use std::io;
use std::path::Path;

const FORMAT_VERSION: i64 = 1;

/// Whether the staging setting is on. A missing or unreadable file means off, so the
/// helper falls back to production.
pub(super) fn read_staging(path: &Path) -> bool {
    read(path)
        .and_then(|settings| settings.get("staging").and_then(Value::as_bool))
        .unwrap_or(false)
}

/// Writes the staging setting through a temporary file renamed over the old one, so a
/// crash leaves either the old file or the new one.
pub(super) fn write_staging(path: &Path, on: bool) -> io::Result<()> {
    let mut settings = read(path).unwrap_or_default();
    settings.insert("formatVersion".into(), json!(FORMAT_VERSION));
    settings.insert("staging".into(), json!(on));
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
