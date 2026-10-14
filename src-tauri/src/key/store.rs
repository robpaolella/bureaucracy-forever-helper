//! The secure store: Keychain on a Mac, Credential Manager on Windows. Each site's key is
//! its own entry. When the store fails, the key is kept nowhere else.

use super::{HelperKey, Site};
use std::fmt;

/// Where keys are kept. The engine takes any store, so tests can use a fake one.
pub trait SecretStore {
    /// Saves `key` as `site`'s key, replacing any earlier one.
    fn save(&self, site: Site, key: &HelperKey) -> Result<(), StoreError>;
    /// `site`'s key, if one is stored. A stored value that isn't a key counts as none.
    fn read(&self, site: Site) -> Result<Option<HelperKey>, StoreError>;
    /// Removes `site`'s key. Removing a key that isn't there is not an error.
    fn delete(&self, site: Site) -> Result<(), StoreError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreAction {
    Save,
    Read,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreProblem {
    /// This system has no secure store the helper can use.
    Unavailable,
    /// The store refused access (on a Mac, access wasn't allowed when asked).
    Refused,
    /// Any other failure.
    Failed,
}

/// A store failure, in plain words. It never holds the key or the system's own message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreError {
    pub action: StoreAction,
    pub problem: StoreProblem,
}

/// The store's name as officers know it.
const STORE_NAME: &str = if cfg!(target_os = "macos") {
    "Keychain"
} else if cfg!(target_os = "windows") {
    "Credential Manager"
} else {
    "the system’s secure store"
};

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let what = match self.action {
            StoreAction::Save => format!("save your key in {STORE_NAME}"),
            StoreAction::Read => format!("read your key from {STORE_NAME}"),
            StoreAction::Delete => format!("remove your key from {STORE_NAME}"),
        };
        let next = match self.problem {
            StoreProblem::Unavailable => "It isn’t available on this computer.",
            StoreProblem::Refused if cfg!(target_os = "macos") => {
                "Allow access when your Mac asks, then try again."
            }
            StoreProblem::Refused => "Allow access, then try again.",
            StoreProblem::Failed => "Try again.",
        };
        write!(f, "The helper couldn’t {what}. {next}")
    }
}

impl std::error::Error for StoreError {}

/// The system's own secure store.
pub struct SystemStore {
    service: String,
}

impl Default for SystemStore {
    fn default() -> Self {
        SystemStore::with_service("com.robpaolella.bureaucracy-forever-helper")
    }
}

impl SystemStore {
    /// A store whose entries are filed under `service`, one per site.
    pub(super) fn with_service(service: &str) -> Self {
        SystemStore {
            service: service.to_string(),
        }
    }

    fn entry(&self, site: Site, action: StoreAction) -> Result<keyring_core::Entry, StoreError> {
        let error = |problem| StoreError { action, problem };
        let store = native_store().ok_or(error(StoreProblem::Unavailable))?;
        store
            .build(&self.service, site.store_name(), None)
            .map_err(|e| error(problem(&e)))
    }
}

impl SecretStore for SystemStore {
    fn save(&self, site: Site, key: &HelperKey) -> Result<(), StoreError> {
        let action = StoreAction::Save;
        self.entry(site, action)?
            .set_password(key.secret())
            .map_err(|e| StoreError {
                action,
                problem: problem(&e),
            })
    }

    fn read(&self, site: Site) -> Result<Option<HelperKey>, StoreError> {
        let action = StoreAction::Read;
        match self.entry(site, action)?.get_password() {
            Ok(stored) => Ok(HelperKey::parse(&stored).ok()),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(StoreError {
                action,
                problem: problem(&e),
            }),
        }
    }

    fn delete(&self, site: Site) -> Result<(), StoreError> {
        let action = StoreAction::Delete;
        match self.entry(site, action)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(StoreError {
                action,
                problem: problem(&e),
            }),
        }
    }
}

fn problem(e: &keyring_core::Error) -> StoreProblem {
    match e {
        keyring_core::Error::NoStorageAccess(_) => StoreProblem::Refused,
        keyring_core::Error::NoDefaultStore | keyring_core::Error::NotSupportedByStore(_) => {
            StoreProblem::Unavailable
        }
        _ => StoreProblem::Failed,
    }
}

#[cfg(target_os = "macos")]
fn native_store() -> Option<std::sync::Arc<keyring_core::CredentialStore>> {
    let store = apple_native_keyring_store::keychain::Store::new().ok()?;
    Some(store)
}

#[cfg(target_os = "windows")]
fn native_store() -> Option<std::sync::Arc<keyring_core::CredentialStore>> {
    let store = windows_native_keyring_store::Store::new().ok()?;
    Some(store)
}

/// The helper ships only for Mac and Windows; elsewhere there is no store.
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_store() -> Option<std::sync::Arc<keyring_core::CredentialStore>> {
    None
}
