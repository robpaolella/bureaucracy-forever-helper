//! Starting the helper when the computer starts. The login item is the system's own (a
//! login item on macOS, the Run registry key on Windows), made by the autostart plugin with
//! `LOGIN_ARG`, so a start at login can tell itself from one the officer opened.
//!
//! The settings file remembers that a choice was made (`startAtLogin`), so the default "on"
//! is set up once, when the key and game folder steps are first both done, and an officer's
//! "off" stays off. What the switch shows is always the system's own state.

use crate::key::settings;
use serde::Serialize;
use std::path::Path;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

/// Passed to the helper by the login item.
pub const LOGIN_ARG: &str = "--autostart";

pub const REFUSED: &str = "Your computer didn’t allow that change, so the helper won’t start by itself. Check your computer’s startup settings and try again.";

/// Whether the helper was started by the login item, so its window stays hidden.
pub fn started_at_login(args: impl IntoIterator<Item = String>) -> bool {
    args.into_iter().any(|arg| arg == LOGIN_ARG)
}

/// The system's login item for the helper.
pub trait LoginItem {
    fn is_enabled(&self) -> Result<bool, String>;
    fn set(&self, on: bool) -> Result<(), String>;
}

impl LoginItem for AppHandle {
    fn is_enabled(&self) -> Result<bool, String> {
        self.autolaunch().is_enabled().map_err(|e| e.to_string())
    }

    fn set(&self, on: bool) -> Result<(), String> {
        let manager = self.autolaunch();
        if on {
            manager.enable()
        } else {
            manager.disable()
        }
        .map_err(|e| e.to_string())
    }
}

/// How a change ended; `on` is what the system has now, which the switch shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LoginOutcome {
    Done {
        on: bool,
    },
    /// The system refused the change.
    Refused {
        message: &'static str,
        on: bool,
    },
}

/// Whether the login item is on. A system that can't say counts as off.
pub fn current(item: &impl LoginItem) -> bool {
    item.is_enabled().unwrap_or(false)
}

/// Turns the login item on or off and remembers the choice.
pub fn change(item: &impl LoginItem, settings_path: &Path, on: bool) -> LoginOutcome {
    if item.set(on).is_err() {
        // The choice was made even if the system refused it, so setup doesn't try again.
        let _ = settings::write_start_at_login(settings_path, false);
        return LoginOutcome::Refused {
            message: REFUSED,
            on: current(item),
        };
    }
    // A choice that can't be remembered only means setup may offer the default once more.
    let _ = settings::write_start_at_login(settings_path, on);
    LoginOutcome::Done { on: current(item) }
}

/// Called whenever the key and game folder steps are both done. The first time, it turns the
/// login item on; after that it changes nothing.
pub fn first_setup(item: &impl LoginItem, settings_path: &Path) -> LoginOutcome {
    if settings::read_start_at_login(settings_path).is_some() {
        return LoginOutcome::Done { on: current(item) };
    }
    change(item, settings_path, true)
}

#[tauri::command]
pub fn get_start_at_login(app: AppHandle) -> bool {
    current(&app)
}

#[tauri::command]
pub fn set_start_at_login(
    app: AppHandle,
    path: tauri::State<'_, SettingsPath>,
    on: bool,
) -> LoginOutcome {
    change(&app, &path.0, on)
}

#[tauri::command]
pub fn finish_login_setup(app: AppHandle, path: tauri::State<'_, SettingsPath>) -> LoginOutcome {
    first_setup(&app, &path.0)
}

/// Where the settings file is, as app state.
pub struct SettingsPath(pub std::path::PathBuf);

#[cfg(test)]
mod tests;
