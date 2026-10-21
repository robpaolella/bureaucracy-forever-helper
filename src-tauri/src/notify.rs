//! The one way to raise a system notification. Only for what the officer must act on (the
//! `/reload` prompt, a key that stopped working, awards that can't send, an update ready),
//! never routine news. Wording and the notices themselves are in the approved design
//! (`notification`): the system shows the app name, then the bold headline, then one line
//! of detail. Feature issues call `raise` from Rust or `raise_notification` from the window.

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// One notice: a bold headline and one line of detail.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Notice {
    pub headline: String,
    pub detail: String,
}

impl Notice {
    /// A notice needs both lines; an empty one would show as a blank notification.
    pub fn new(headline: &str, detail: &str) -> Result<Notice, String> {
        let (headline, detail) = (headline.trim(), detail.trim());
        if headline.is_empty() || detail.is_empty() {
            return Err("A notification needs a headline and a detail line.".into());
        }
        Ok(Notice {
            headline: headline.into(),
            detail: detail.into(),
        })
    }
}

/// Clicking a notice opens the window, even when it was hidden. Called once at start-up.
pub fn open_window_on_click(app: &AppHandle) -> tauri::Result<()> {
    let handle = app.clone();
    app.notification()
        .on_action(move |action| {
            if action.action_id() == "tap" {
                show_window(&handle);
            }
        })
        .map_err(|e| tauri::Error::Anyhow(e.into()))
}

/// The event that tells the window it was shown (`true`) or hidden (`false`). The webview's
/// own visibility events differ between systems, so the app says it itself.
pub const WINDOW_VISIBLE_EVENT: &str = "window-visible";

pub fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
        let _ = window.emit(WINDOW_VISIBLE_EVENT, true);
    }
}

/// Shows `notice` as a system notification. The system may refuse (notifications turned off
/// for the app); that is returned for the caller to log, never a reason to stop.
pub fn raise(app: &AppHandle, notice: &Notice) -> Result<(), String> {
    app.notification()
        .builder()
        .title(&notice.headline)
        .body(&notice.detail)
        .show()
        .map_err(|e| e.to_string())
}

/// For features that run in the window. Returns an error only when the notice is empty or the
/// system refused it.
#[tauri::command]
pub fn raise_notification(app: AppHandle, notice: Notice) -> Result<(), String> {
    let notice = Notice::new(&notice.headline, &notice.detail)?;
    raise(&app, &notice)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_notice_keeps_both_lines_trimmed() {
        let n = Notice::new(
            " Type /reload in game ",
            "Reserves for Molten Core are ready.",
        )
        .unwrap();
        assert_eq!(n.headline, "Type /reload in game");
        assert_eq!(n.detail, "Reserves for Molten Core are ready.");
    }

    #[test]
    fn an_empty_line_is_refused() {
        assert!(Notice::new("", "detail").is_err());
        assert!(Notice::new("headline", "  ").is_err());
    }
}
