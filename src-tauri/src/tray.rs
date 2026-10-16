//! The tray (menu bar) icon, its menu and its hover text. The window owns the status and
//! passes it here through `set_tray_status` whenever it changes; this module only shows it.
//! Words and order follow the approved design (`tray` state).
//!
//! The Mac menu bar can be light or dark, so the icon comes in a version for each, chosen by
//! the system theme; the status dot's colour follows the status tone.

use serde::Deserialize;
use std::sync::Mutex;
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Theme,
};

pub const TRAY_ID: &str = "main";
pub const SHOW_ID: &str = "show";
pub const QUIT_ID: &str = "quit";
const APP_NAME: &str = "Bureaucracy Forever Helper";

/// How the status reads, as the window's pill does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tone {
    Ok,
    Warn,
    Stop,
    /// "Not set up": the design's "needs you".
    Closed,
}

/// The colour of the dot on the icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dot {
    Green,
    Amber,
    Red,
}

impl Tone {
    pub fn dot(self) -> Dot {
        match self {
            Tone::Ok => Dot::Green,
            Tone::Warn | Tone::Closed => Dot::Amber,
            Tone::Stop => Dot::Red,
        }
    }
}

/// What the window tells the tray (`TrayStatus` in src/bridge.ts).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayStatus {
    pub tone: Tone,
    /// The status word, such as "Sending".
    pub word: String,
    pub sent_tonight: u32,
    pub staging: bool,
    /// The finished second line (#60 decides it); none leaves the line out.
    pub second_line: Option<String>,
}

impl Default for TrayStatus {
    fn default() -> Self {
        TrayStatus {
            tone: Tone::Closed,
            word: "Not set up".into(),
            sent_tonight: 0,
            staging: false,
            second_line: None,
        }
    }
}

fn sent(n: u32) -> Option<String> {
    match n {
        0 => None,
        1 => Some("1 award sent".into()),
        n => Some(format!("{n} awards sent")),
    }
}

impl TrayStatus {
    /// The greyed first menu line: "Sending · 14 awards sent", or "Staging site · Ready"
    /// while staging is on.
    pub fn status_line(&self) -> String {
        if self.staging {
            return format!("Staging site · {}", self.word);
        }
        match sent(self.sent_tonight) {
            Some(count) => format!("{} · {count}", self.word),
            None => self.word.clone(),
        }
    }

    /// The hover text: "Bureaucracy Forever Helper: Sending, 14 awards sent".
    pub fn tooltip(&self) -> String {
        let detail = if self.staging {
            Some("Staging site".to_string())
        } else {
            sent(self.sent_tonight)
        };
        match detail {
            Some(d) => format!("{APP_NAME}: {}, {d}", self.word),
            None => format!("{APP_NAME}: {}", self.word),
        }
    }

    /// The greyed second line, left out when there is nothing to say.
    pub fn second_line(&self) -> Option<&str> {
        self.second_line
            .as_deref()
            .map(str::trim)
            .filter(|l| !l.is_empty())
    }
}

/// The icon for a dot colour on a dark or light menu bar.
fn icon(dot: Dot, light_bar: bool) -> Image<'static> {
    let bytes: &[u8] = match (dot, light_bar) {
        (Dot::Green, false) => include_bytes!("../icons/tray/ok-dark.png"),
        (Dot::Green, true) => include_bytes!("../icons/tray/ok-light.png"),
        (Dot::Amber, false) => include_bytes!("../icons/tray/warn-dark.png"),
        (Dot::Amber, true) => include_bytes!("../icons/tray/warn-light.png"),
        (Dot::Red, false) => include_bytes!("../icons/tray/stop-dark.png"),
        (Dot::Red, true) => include_bytes!("../icons/tray/stop-light.png"),
    };
    Image::from_bytes(bytes).expect("the tray icons are valid PNGs")
}

/// The latest status, kept so the icon can be redrawn when the system theme changes.
#[derive(Default)]
pub struct Current(Mutex<TrayStatus>);

fn menu(app: &AppHandle, status: &TrayStatus) -> tauri::Result<Menu<tauri::Wry>> {
    let line = |text: String| MenuItem::new(app, text, false, None::<&str>);
    let menu = Menu::new(app)?;
    menu.append(&line(status.status_line())?)?;
    if let Some(second) = status.second_line() {
        menu.append(&line(second.to_string())?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        SHOW_ID,
        format!("Open {APP_NAME}"),
        true,
        None::<&str>,
    )?)?;
    // "Check for updates" is added here, with its action, by #7.
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        QUIT_ID,
        "Quit",
        true,
        None::<&str>,
    )?)?;
    Ok(menu)
}

fn light_bar(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|w| w.theme().ok())
        .is_some_and(|t| t == Theme::Light)
}

/// Draws `status` on the tray: icon, hover text and menu.
fn apply(app: &AppHandle, status: &TrayStatus) -> tauri::Result<()> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Ok(());
    };
    tray.set_icon(Some(icon(status.tone.dot(), light_bar(app))))?;
    tray.set_tooltip(Some(status.tooltip()))?;
    tray.set_menu(Some(menu(app, status)?))
}

/// Makes the tray with the "not set up" status; the window replaces it once it knows more.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let status = TrayStatus::default();
    app.manage(Current::default());
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(status.tone.dot(), light_bar(app)))
        .tooltip(status.tooltip())
        .menu(&menu(app, &status)?)
        .on_menu_event(|app, event| match event.id.as_ref() {
            QUIT_ID => {
                app.state::<crate::upload::Runner>().stop();
                app.state::<crate::raids::Poller>().stop();
                app.state::<crate::savedvars::watcher::Watcher>().stop();
                app.exit(0);
            }
            SHOW_ID => crate::notify::show_window(app),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Redraws the tray with the last status, for when the menu bar's theme changes.
pub fn refresh(app: &AppHandle) {
    let status = app
        .state::<Current>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let _ = apply(app, &status);
}

/// The window's status, shown on the tray as soon as it changes.
#[tauri::command]
pub fn set_tray_status(app: AppHandle, status: TrayStatus) -> Result<(), String> {
    *app.state::<Current>()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = status.clone();
    apply(&app, &status).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(word: &str, sent: u32) -> TrayStatus {
        TrayStatus {
            tone: Tone::Ok,
            word: word.into(),
            sent_tonight: sent,
            staging: false,
            second_line: None,
        }
    }

    #[test]
    fn tone_picks_the_dot() {
        assert_eq!(Tone::Ok.dot(), Dot::Green);
        assert_eq!(Tone::Warn.dot(), Dot::Amber);
        assert_eq!(Tone::Closed.dot(), Dot::Amber);
        assert_eq!(Tone::Stop.dot(), Dot::Red);
    }

    #[test]
    fn menu_line_and_hover_text_follow_the_design() {
        let s = status("Sending", 14);
        assert_eq!(s.status_line(), "Sending · 14 awards sent");
        assert_eq!(
            s.tooltip(),
            "Bureaucracy Forever Helper: Sending, 14 awards sent"
        );
    }

    #[test]
    fn one_award_is_singular_and_none_is_left_out() {
        assert_eq!(status("Sending", 1).status_line(), "Sending · 1 award sent");
        assert_eq!(status("Ready", 0).status_line(), "Ready");
        assert_eq!(
            status("Ready", 0).tooltip(),
            "Bureaucracy Forever Helper: Ready"
        );
    }

    #[test]
    fn staging_replaces_the_count() {
        let s = TrayStatus {
            staging: true,
            ..status("Ready", 14)
        };
        assert_eq!(s.status_line(), "Staging site · Ready");
        assert_eq!(
            s.tooltip(),
            "Bureaucracy Forever Helper: Ready, Staging site"
        );
    }

    #[test]
    fn the_second_line_is_shown_as_given_or_left_out() {
        for line in [
            "Awards going to Molten Core",
            "Raid not loaded in game",
            "Raid not chosen yet",
            "Not a guild run",
        ] {
            let s = TrayStatus {
                second_line: Some(line.into()),
                ..status("Sending", 1)
            };
            assert_eq!(s.second_line(), Some(line));
        }
        assert_eq!(status("Sending", 1).second_line(), None);
        let blank = TrayStatus {
            second_line: Some("  ".into()),
            ..status("Sending", 1)
        };
        assert_eq!(blank.second_line(), None);
    }

    #[test]
    fn the_window_message_reads_as_a_status() {
        let s: TrayStatus = serde_json::from_str(
            r#"{"tone":"warn","word":"Offline","sentTonight":4,"staging":false,"secondLine":null}"#,
        )
        .unwrap();
        assert_eq!(s.tone, Tone::Warn);
        assert_eq!(s.status_line(), "Offline · 4 awards sent");
    }

    #[test]
    fn every_icon_decodes() {
        for dot in [Dot::Green, Dot::Amber, Dot::Red] {
            for light in [false, true] {
                assert_eq!(icon(dot, light).width(), 64);
            }
        }
    }
}
