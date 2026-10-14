pub mod inbox;
pub mod queue;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, WindowEvent,
};

const QUIT_ID: &str = "quit";
const SHOW_ID: &str = "show";

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let show = MenuItem::with_id(app, SHOW_ID, "Show", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Bureaucracy Forever Helper")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    QUIT_ID => app.exit(0),
                    SHOW_ID => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running the Bureaucracy Forever Helper");
}

#[cfg(test)]
mod tests {
    #[test]
    fn placeholder() {
        assert_eq!(super::QUIT_ID, "quit");
    }
}
