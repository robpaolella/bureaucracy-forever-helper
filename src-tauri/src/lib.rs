pub mod chatlog;
pub mod commands;
pub mod game_folder;
pub mod inbox;
pub mod key;
pub mod queue;
pub mod upload;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Emitter, Manager, WindowEvent,
};

use commands::Engine;
use key::{KeyEngine, SiteClient, SystemStore, SETTINGS_FILE};
use queue::Queue;
use std::sync::{Arc, Mutex};
use upload::{Runner, SYNC_STATUS_EVENT};

const QUIT_ID: &str = "quit";
const SHOW_ID: &str = "show";

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // A missing folder only means the staging setting can't be saved later, which
            // `set_staging_site` reports; the window still opens.
            let data_dir = app.path().app_data_dir()?;
            let _ = std::fs::create_dir_all(&data_dir);
            app.manage(std::sync::Arc::new(game_folder::GameFolderStore::new(
                data_dir.join(SETTINGS_FILE),
            )));
            let engine = Engine::new(KeyEngine::open(
                data_dir.join(SETTINGS_FILE),
                SystemStore::default(),
                SiteClient::default(),
            ));
            // One waiting list for the whole app; the add-on reader and chat log follower
            // will add to it through this same lock. A damaged file shows in the status.
            let queue = Arc::new(Mutex::new(Queue::open(data_dir.join(queue::FILE_NAME))));
            let handle = app.handle().clone();
            app.manage(Runner::start(
                engine.clone(),
                Arc::clone(&queue),
                SiteClient::default(),
                move |status| {
                    let _ = handle.emit(SYNC_STATUS_EVENT, status);
                },
            ));
            app.manage(engine);
            app.manage(queue);
            let show = MenuItem::with_id(app, SHOW_ID, "Show", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, QUIT_ID, "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Bureaucracy Forever Helper")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    QUIT_ID => {
                        app.state::<Runner>().stop();
                        app.exit(0);
                    }
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
        .invoke_handler(tauri::generate_handler![
            commands::connect_key,
            commands::check_stored_key,
            commands::remove_stored_key,
            commands::get_staging,
            commands::set_staging_site,
            commands::search_game_folder,
            commands::check_picked_game_folder,
            commands::remember_game_version,
            commands::get_remembered_game_folder,
            commands::get_sync_status,
        ])
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
