//! The window's way into the key engine: Tauri commands that connect with a key, check
//! the stored key, remove it, and read or change the staging setting.
//!
//! The game folder commands search for the folder, check a picked one, and remember or
//! read the remembered one.
//!
//! Every command returns its result as data the front-end can tell apart (`kind`), with
//! the engine's own plain-words message where there is one. Nothing is swallowed, and
//! neither the key nor anything the site sent back is in a result beyond what the
//! engine already allows. The logic is in functions over any engine, so tests use a fake
//! store and a local site; the `#[tauri::command]` wrappers only hand them the app state.

use crate::game_folder::{self, Environment, FolderOutcome, GameFolderStore};
use crate::key::{
    ConnectError, Connection, HelperKey, KeyEngine, Refusal, SecretStore, ShapeError, Site,
    SiteError, StoreError, SystemStore,
};
use crate::raids::{Poller, RaidsView};
use crate::upload::{Runner, SyncStatus};
use serde::Serialize;
use std::sync::{Arc, RwLock};

/// The engine, held as app state. Anything that changes the stored key or the site (connect,
/// remove, switch) takes the write lock, so they happen one at a time in the order they were
/// asked: a slow connect can't save its key after a removal that came later has returned.
/// Checking the stored key only reads, so checks run side by side.
pub struct Engine<S: SecretStore = SystemStore>(Arc<RwLock<KeyEngine<S>>>);

impl<S: SecretStore> Clone for Engine<S> {
    fn clone(&self) -> Self {
        Engine(Arc::clone(&self.0))
    }
}

impl<S: SecretStore> Engine<S> {
    pub fn new(engine: KeyEngine<S>) -> Self {
        Engine(Arc::new(RwLock::new(engine)))
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, KeyEngine<S>> {
        self.0.read().unwrap_or_else(|e| e.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, KeyEngine<S>> {
        self.0.write().unwrap_or_else(|e| e.into_inner())
    }

    /// The current site and its stored key, read together so the key always belongs to
    /// the site it goes to.
    pub fn upload_target(&self) -> (Site, Result<Option<HelperKey>, StoreError>) {
        let engine = self.read();
        (engine.site(), engine.stored_key())
    }
}

/// How connecting with a key, or checking the stored one, ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeyOutcome {
    /// The site accepted the key.
    #[serde(rename_all = "camelCase")]
    Accepted {
        /// The key owner's Discord name, for "Connected as <name>".
        officer: String,
        env: String,
        format_versions: Vec<i64>,
        addon_version: String,
        addon_url: String,
    },
    /// The site refused the key: `badKey` (unknown or revoked) or `notOfficer`.
    Refused {
        reason: &'static str,
        message: String,
    },
    /// The site has the helper switched off.
    SiteOff { message: String },
    /// Staging isn't open to the public yet.
    StagingNotOpen { message: String },
    /// No usable answer from the site. The stored key is kept.
    Unreachable { message: String },
    /// The secure store failed.
    StoreFailed { message: String },
    /// The pasted key has the wrong shape (`empty` or `malformed`); the site wasn't asked.
    Malformed {
        reason: &'static str,
        message: String,
    },
    /// Checking the stored key, but none is stored for the current site.
    NoKey,
}

/// How removing a key or changing the staging setting ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChangeOutcome {
    Done,
    StoreFailed {
        message: String,
    },
    /// The settings file couldn't be written, so the site is unchanged.
    SettingsFailed {
        message: String,
    },
}

impl From<Result<Connection, ConnectError>> for KeyOutcome {
    fn from(result: Result<Connection, ConnectError>) -> Self {
        let connection = match result {
            Ok(c) => c,
            Err(e) => return KeyOutcome::from(e),
        };
        KeyOutcome::Accepted {
            officer: connection.officer,
            env: connection.env,
            format_versions: connection.format_versions,
            addon_version: connection.addon_version,
            addon_url: connection.addon_url,
        }
    }
}

impl From<ConnectError> for KeyOutcome {
    fn from(error: ConnectError) -> Self {
        let message = error.to_string();
        match error {
            ConnectError::NoKey => KeyOutcome::NoKey,
            ConnectError::Shape(shape) => KeyOutcome::Malformed {
                reason: match shape {
                    ShapeError::Empty => "empty",
                    ShapeError::Malformed => "malformed",
                },
                message,
            },
            ConnectError::Store(_) => KeyOutcome::StoreFailed { message },
            ConnectError::Site(SiteError::KeyRefused(why)) => KeyOutcome::Refused {
                reason: match why {
                    Refusal::BadKey => "badKey",
                    Refusal::NotOfficer => "notOfficer",
                },
                message,
            },
            ConnectError::Site(SiteError::HelperOff) => KeyOutcome::SiteOff { message },
            ConnectError::Site(SiteError::StagingNotOpen) => KeyOutcome::StagingNotOpen { message },
            ConnectError::Site(SiteError::Unreachable { .. }) => {
                KeyOutcome::Unreachable { message }
            }
        }
    }
}

pub fn connect<S: SecretStore>(engine: &Engine<S>, pasted: &str) -> KeyOutcome {
    engine.write().connect(pasted).into()
}

pub fn check_stored<S: SecretStore>(engine: &Engine<S>) -> KeyOutcome {
    engine.read().check_stored().into()
}

pub fn remove_key<S: SecretStore>(engine: &Engine<S>) -> ChangeOutcome {
    match engine.write().remove_key() {
        Ok(()) => ChangeOutcome::Done,
        Err(e) => ChangeOutcome::StoreFailed {
            message: e.to_string(),
        },
    }
}

pub fn staging<S: SecretStore>(engine: &Engine<S>) -> bool {
    engine.read().site() == Site::Staging
}

pub fn set_staging<S: SecretStore>(engine: &Engine<S>, on: bool) -> ChangeOutcome {
    match engine.write().set_staging(on) {
        Ok(()) => ChangeOutcome::Done,
        Err(_) => ChangeOutcome::SettingsFailed {
            message: "The helper couldn’t save that setting. Try again.".to_string(),
        },
    }
}

/// Looks in the usual places for the game folder.
pub fn find_game_folder(environment: &Environment) -> FolderOutcome {
    game_folder::search(&environment.usual_places())
}

/// Checks a folder the officer picked.
pub fn check_game_folder(folder: &str) -> FolderOutcome {
    game_folder::check(std::path::Path::new(folder))
}

pub fn remember_game_folder(store: &GameFolderStore, version_folder: &str) -> FolderOutcome {
    store.remember(std::path::Path::new(version_folder))
}

pub fn remembered_game_folder(store: &GameFolderStore) -> FolderOutcome {
    store.remembered()
}

/// Once the key or site has changed, the uploader and the raids list start again with the
/// new ones. A failed change leaves them as they were, so both carry on, keeping any wait
/// the site asked for.
fn restart_if(runner: &Runner, raids: &Poller, changed: bool) {
    if changed {
        runner.restart();
        raids.restart();
    }
}

/// Connecting and checking wait on the network, so they run off the thread that draws
/// the window. A failure of the thread itself rejects the call.
async fn off_thread<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|_| "The helper hit an unexpected problem. Try again.".to_string())
}

#[tauri::command]
pub async fn connect_key(
    engine: tauri::State<'_, Engine>,
    runner: tauri::State<'_, Runner>,
    raids: tauri::State<'_, Poller>,
    key: String,
) -> Result<KeyOutcome, String> {
    let (engine, runner, raids) = (
        engine.inner().clone(),
        runner.inner().clone(),
        raids.inner().clone(),
    );
    off_thread(move || {
        let outcome = connect(&engine, &key);
        restart_if(
            &runner,
            &raids,
            matches!(outcome, KeyOutcome::Accepted { .. }),
        );
        outcome
    })
    .await
}

#[tauri::command]
pub async fn check_stored_key(engine: tauri::State<'_, Engine>) -> Result<KeyOutcome, String> {
    let engine = engine.inner().clone();
    off_thread(move || check_stored(&engine)).await
}

#[tauri::command]
pub async fn remove_stored_key(
    engine: tauri::State<'_, Engine>,
    runner: tauri::State<'_, Runner>,
    raids: tauri::State<'_, Poller>,
) -> Result<ChangeOutcome, String> {
    let (engine, runner, raids) = (
        engine.inner().clone(),
        runner.inner().clone(),
        raids.inner().clone(),
    );
    off_thread(move || {
        let outcome = remove_key(&engine);
        restart_if(&runner, &raids, outcome == ChangeOutcome::Done);
        outcome
    })
    .await
}

#[tauri::command]
pub fn get_staging(engine: tauri::State<'_, Engine>) -> bool {
    staging(&engine)
}

#[tauri::command]
pub async fn set_staging_site(
    engine: tauri::State<'_, Engine>,
    runner: tauri::State<'_, Runner>,
    raids: tauri::State<'_, Poller>,
    on: bool,
) -> Result<ChangeOutcome, String> {
    let (engine, runner, raids) = (
        engine.inner().clone(),
        runner.inner().clone(),
        raids.inner().clone(),
    );
    off_thread(move || {
        let outcome = set_staging(&engine, on);
        restart_if(&runner, &raids, outcome == ChangeOutcome::Done);
        outcome
    })
    .await
}

/// The upload's sync status now; each later change comes as a `sync-status` event.
#[tauri::command]
pub fn get_sync_status(runner: tauri::State<'_, Runner>) -> SyncStatus {
    runner.status()
}

/// The upcoming raids and how the list stands now; each later change comes as a `raids` event.
#[tauri::command]
pub fn get_raids(raids: tauri::State<'_, Poller>) -> RaidsView {
    raids.view()
}

#[tauri::command]
pub async fn search_game_folder() -> Result<FolderOutcome, String> {
    off_thread(|| find_game_folder(&Environment::current())).await
}

#[tauri::command]
pub async fn check_picked_game_folder(folder: String) -> Result<FolderOutcome, String> {
    off_thread(move || check_game_folder(&folder)).await
}

#[tauri::command]
pub async fn remember_game_version(
    store: tauri::State<'_, Arc<GameFolderStore>>,
    folder: String,
) -> Result<FolderOutcome, String> {
    let store = Arc::clone(store.inner());
    off_thread(move || remember_game_folder(&store, &folder)).await
}

#[tauri::command]
pub async fn get_remembered_game_folder(
    store: tauri::State<'_, Arc<GameFolderStore>>,
) -> Result<FolderOutcome, String> {
    let store = Arc::clone(store.inner());
    off_thread(move || remembered_game_folder(&store)).await
}

#[cfg(test)]
mod tests;
