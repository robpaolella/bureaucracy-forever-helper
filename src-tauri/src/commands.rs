//! The window's way into the key engine: Tauri commands that connect with a key, check
//! the stored key, remove it, and read or change the staging setting.
//!
//! Every command returns its result as data the front-end can tell apart (`kind`), with
//! the engine's own plain-words message where there is one. Nothing is swallowed, and
//! neither the key nor anything the site sent back is in a result beyond what the
//! engine already allows. The logic is in functions over any engine, so tests use a fake
//! store and a local site; the `#[tauri::command]` wrappers only hand them the app state.

use crate::key::{
    ConnectError, Connection, KeyEngine, Refusal, SecretStore, ShapeError, SiteError, SystemStore,
};
use serde::Serialize;
use std::sync::{Arc, RwLock};

/// The engine, held as app state. Checks only read it, so a slow check never holds up
/// removing a key; switching the site waits for checks to finish.
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
    engine.read().connect(pasted).into()
}

pub fn check_stored<S: SecretStore>(engine: &Engine<S>) -> KeyOutcome {
    engine.read().check_stored().into()
}

pub fn remove_key<S: SecretStore>(engine: &Engine<S>) -> ChangeOutcome {
    match engine.read().remove_key() {
        Ok(()) => ChangeOutcome::Done,
        Err(e) => ChangeOutcome::StoreFailed {
            message: e.to_string(),
        },
    }
}

pub fn staging<S: SecretStore>(engine: &Engine<S>) -> bool {
    engine.read().site() == crate::key::Site::Staging
}

pub fn set_staging<S: SecretStore>(engine: &Engine<S>, on: bool) -> ChangeOutcome {
    match engine.write().set_staging(on) {
        Ok(()) => ChangeOutcome::Done,
        Err(_) => ChangeOutcome::SettingsFailed {
            message: "The helper couldn’t save that setting. Try again.".to_string(),
        },
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
    key: String,
) -> Result<KeyOutcome, String> {
    let engine = engine.inner().clone();
    off_thread(move || connect(&engine, &key)).await
}

#[tauri::command]
pub async fn check_stored_key(engine: tauri::State<'_, Engine>) -> Result<KeyOutcome, String> {
    let engine = engine.inner().clone();
    off_thread(move || check_stored(&engine)).await
}

#[tauri::command]
pub async fn remove_stored_key(engine: tauri::State<'_, Engine>) -> Result<ChangeOutcome, String> {
    let engine = engine.inner().clone();
    off_thread(move || remove_key(&engine)).await
}

#[tauri::command]
pub fn get_staging(engine: tauri::State<'_, Engine>) -> bool {
    staging(&engine)
}

#[tauri::command]
pub async fn set_staging_site(
    engine: tauri::State<'_, Engine>,
    on: bool,
) -> Result<ChangeOutcome, String> {
    let engine = engine.inner().clone();
    off_thread(move || set_staging(&engine, on)).await
}

#[cfg(test)]
mod tests;
