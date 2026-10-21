pub mod addon_update;
pub mod chatlog;
pub mod commands;
pub mod game_folder;
pub mod inbox;
pub mod key;
pub mod notify;
pub mod queue;
pub mod raids;
pub mod reserves;
pub mod savedvars;
pub mod site_link;
pub mod throttle;
pub mod tray;
pub mod updater;
pub mod upload;

use tauri::{Emitter, Manager, WindowEvent};

use chatlog::follower::{self, Follower, Tail, CHAT_LOG_EVENT};
use commands::Engine;
use key::{KeyEngine, SiteClient, SystemStore, SETTINGS_FILE};
use queue::Queue;
use raids::{Poller, Timing as RaidsTiming, RAIDS_EVENT};
use reserves::{Inputs, Reserves, RESERVES_EVENT};
use savedvars::choice::{Choices, RAID_CHOICE_EVENT};
use savedvars::watcher::{Event, Timing, Watcher};
use std::sync::{Arc, Mutex};
use upload::{Runner, SYNC_STATUS_EVENT};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updater::Updates::default())
        .manage(Arc::new(addon_update::AddonUpdates::new(
            addon_update::GitHub::default(),
        )))
        .setup(|app| {
            // A missing folder only means the staging setting can't be saved later, which
            // `set_staging_site` reports; the window still opens.
            let data_dir = app.path().app_data_dir()?;
            let _ = std::fs::create_dir_all(&data_dir);
            let folders = Arc::new(game_folder::GameFolderStore::new(
                data_dir.join(SETTINGS_FILE),
            ));
            app.manage(Arc::clone(&folders));
            let engine = Engine::new(KeyEngine::open(
                data_dir.join(SETTINGS_FILE),
                SystemStore::default(),
                SiteClient::default(),
            ));
            // One waiting list for the whole app; the add-on reader and chat log follower
            // add to it through this same lock. A damaged file shows in the status.
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
            // The upcoming raids, kept fresh while the window is hidden. #61 shows them;
            // the inbox and chat log work read `Poller::view`.
            let handle = app.handle().clone();
            let raids = Poller::start(
                engine.clone(),
                SiteClient::default(),
                RaidsTiming::default(),
                move |view| {
                    let _ = handle.emit(RAIDS_EVENT, view);
                },
            );
            app.manage(raids.clone());
            // The reserves for those raids, written into the add-on's inbox when they change.
            // The game saving its variables (at `/reload`) tells it the officer reloaded.
            let (handle, remembered) = (app.handle().clone(), Arc::clone(&folders));
            let reserves = Reserves::start(
                engine.clone(),
                Inputs {
                    source: SiteClient::default(),
                    raids: move || raids.view(),
                    target: move || match remembered.remembered() {
                        game_folder::FolderOutcome::Found(install) => Some(inbox::path_in(
                            std::path::Path::new(&install.version_folder),
                        )),
                        _ => None,
                    },
                    now: || chrono::Utc::now().timestamp(),
                    timing: reserves::Timing::default(),
                },
                move |view| {
                    let _ = handle.emit(RESERVES_EVENT, view);
                },
            );
            app.manage(reserves.clone());
            // The chat log, followed for the add-on's award lines, with the raids and bosses
            // of the snapshots the reserves fetched.
            let (handle, remembered, known) =
                (app.handle().clone(), Arc::clone(&folders), reserves.clone());
            app.manage(Tail::start(
                Follower::open(data_dir.join(follower::FILE_NAME)),
                move || match remembered.remembered() {
                    game_folder::FolderOutcome::Found(install) => {
                        Some(install.version_folder.into())
                    }
                    _ => None,
                },
                move || known.known_raids(),
                Arc::clone(&queue),
                follower::POLL,
                move |event| {
                    let _ = handle.emit(CHAT_LOG_EVENT, event);
                },
            ));
            // The add-on's saved file, read when the game saves it. The status and error
            // display come with the sync window. How the raid was chosen goes to the window.
            let (handle, choices) = (app.handle().clone(), Arc::new(Choices::default()));
            app.manage(choices.clone());
            app.manage(Watcher::start(
                move || match folders.remembered() {
                    game_folder::FolderOutcome::Found(install) => {
                        Some(install.version_folder.into())
                    }
                    _ => None,
                },
                Arc::clone(&queue),
                Timing::default(),
                move |event| {
                    if let Event::Read(reading) = event {
                        reserves.game_saved(reading.saved_at);
                        if let Some(choice) = choices.record(&reading) {
                            let _ = handle.emit(RAID_CHOICE_EVENT, choice);
                        }
                    }
                },
            ));
            app.manage(engine);
            app.manage(queue);
            tray::build(app.handle())?;
            notify::open_window_on_click(app.handle())?;
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
            commands::get_raids,
            commands::get_reserves,
            commands::get_raid_choice,
            commands::check_for_update,
            commands::install_update,
            commands::restart_after_update,
            updater::open_update_notes,
            commands::check_addon_update,
            commands::install_addon,
            addon_update::open_addon_notes,
            site_link::open_loot_log,
            tray::set_tray_status,
            notify::raise_notification,
        ])
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                // The menu bar's brightness follows the system theme, so the icon is redrawn.
                WindowEvent::ThemeChanged(_) => tray::refresh(window.app_handle()),
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running the Bureaucracy Forever Helper");
}
