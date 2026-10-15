//! The helper key engine: checks a pasted helper key against the guild site, keeps it in
//! the system's secure store, and remembers which site (production or staging) the helper
//! talks to (ADDON-SPEC 7.1, 7.2).
//!
//! Each site has its own key in the store, under its own name, and a check only ever sends
//! the current site's key to that site, so switching sites never carries a key over.
//!
//! The key itself is never written to the settings file, and never appears in a message:
//! `HelperKey` prints as `HelperKey(…)` and every error is built from fixed words and
//! status codes, never from the key or from what the site sent back.

pub(crate) mod settings;
mod site;
mod store;

pub use site::{SiteClient, UploadReply};
pub use store::{SecretStore, StoreAction, StoreError, StoreProblem, SystemStore};

use std::fmt;
use std::io;
use std::path::PathBuf;

/// The file's name in the helper's app data folder.
pub const SETTINGS_FILE: &str = "settings.json";

/// The guild site the helper talks to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Site {
    Production,
    Staging,
}

impl Site {
    /// The site's address (the site repo's AGENTS.md).
    pub fn address(self) -> &'static str {
        match self {
            Site::Production => "https://www.bureauguild.com",
            Site::Staging => "https://staging.bureauguild.com",
        }
    }

    /// The name its key is kept under in the secure store.
    fn store_name(self) -> &'static str {
        match self {
            Site::Production => "production",
            Site::Staging => "staging",
        }
    }
}

/// A key with the right shape: `bfh_` plus 32 random bytes in base64url (ADDON-SPEC 7.2).
#[derive(Clone, PartialEq, Eq)]
pub struct HelperKey(String);

impl HelperKey {
    /// Reads a pasted key, ignoring spaces and line breaks around it.
    pub fn parse(pasted: &str) -> Result<HelperKey, ShapeError> {
        let key = pasted.trim();
        if key.is_empty() {
            return Err(ShapeError::Empty);
        }
        let body = key.strip_prefix("bfh_").ok_or(ShapeError::Malformed)?;
        let base64url = |b: u8| b.is_ascii_alphanumeric() || b == b'-' || b == b'_';
        // 32 bytes are 43 base64url characters without padding.
        if body.len() == 43 && body.bytes().all(base64url) {
            Ok(HelperKey(key.to_string()))
        } else {
            Err(ShapeError::Malformed)
        }
    }

    fn secret(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for HelperKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("HelperKey(…)")
    }
}

/// Why a pasted key was refused before asking the site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeError {
    Empty,
    Malformed,
}

impl fmt::Display for ShapeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShapeError::Empty => f.write_str("Paste your helper key."),
            ShapeError::Malformed => f.write_str(
                "That doesn’t look like a helper key. Copy it again from the officer area.",
            ),
        }
    }
}

/// What the site says about an accepted key (`GET /api/helper/me`, ADDON-SPEC 7.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connection {
    /// The key owner's Discord name, shown as "Connected as <name>".
    pub officer: String,
    /// The site's environment, `production` or `staging` (ADDON-SPEC 2).
    pub env: String,
    pub format_versions: Vec<i64>,
    pub addon_version: String,
    pub addon_url: String,
}

/// Why the site refused the key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// `401`: unknown, revoked, or made by the other site.
    BadKey,
    /// `403`: its owner is no longer an officer.
    NotOfficer,
}

/// A check that didn't end in an accepted key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiteError {
    KeyRefused(Refusal),
    /// `503` `helper-off`: the site has the helper switched off, whatever the key.
    HelperOff,
    /// Staging answered with Vercel's login page, so it isn't open to the public yet.
    StagingNotOpen,
    /// No usable answer: no connection, a timeout, or any other status or body.
    /// `detail` says which, in words and status codes only.
    Unreachable {
        detail: String,
    },
}

impl fmt::Display for SiteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SiteError::KeyRefused(_) => {
                "This key has been revoked or no longer works. Create a new key in the officer area."
            }
            SiteError::HelperOff => {
                "The guild site hasn’t switched the helper on yet. Try again later."
            }
            SiteError::StagingNotOpen => "The staging site isn't open yet.",
            SiteError::Unreachable { .. } => {
                "Couldn’t reach the guild site. Check your internet connection and try again."
            }
        })
    }
}

/// Why connecting with a key, or checking the stored one, didn't give a connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectError {
    Shape(ShapeError),
    /// No key is stored for the current site.
    NoKey,
    Site(SiteError),
    Store(StoreError),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectError::Shape(e) => e.fmt(f),
            ConnectError::NoKey => f.write_str("Paste your helper key."),
            ConnectError::Site(e) => e.fmt(f),
            ConnectError::Store(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for ConnectError {}

/// The key engine: the current site, its stored key, and the client that checks it.
pub struct KeyEngine<S: SecretStore> {
    settings_path: PathBuf,
    site: Site,
    store: S,
    client: SiteClient,
}

impl<S: SecretStore> KeyEngine<S> {
    /// Opens the engine with the settings file at `settings_path`. A missing or unreadable
    /// file means production.
    pub fn open(settings_path: impl Into<PathBuf>, store: S, client: SiteClient) -> Self {
        let settings_path = settings_path.into();
        let site = if settings::read_staging(&settings_path) {
            Site::Staging
        } else {
            Site::Production
        };
        KeyEngine {
            settings_path,
            site,
            store,
            client,
        }
    }

    /// The site the helper talks to now.
    pub fn site(&self) -> Site {
        self.site
    }

    /// Switches between production and staging, remembered between runs. Each site keeps
    /// its own key; nothing is copied across.
    pub fn set_staging(&mut self, on: bool) -> io::Result<()> {
        settings::write_staging(&self.settings_path, on)?;
        self.site = if on { Site::Staging } else { Site::Production };
        Ok(())
    }

    /// Checks a pasted key with the current site and, once the site accepts it, stores it
    /// as that site's key, replacing any earlier one. A key the site didn't accept is not
    /// stored, and the stored key is left as it was.
    pub fn connect(&self, pasted: &str) -> Result<Connection, ConnectError> {
        let key = HelperKey::parse(pasted).map_err(ConnectError::Shape)?;
        let connection = self
            .client
            .check(self.site, &key)
            .map_err(ConnectError::Site)?;
        self.store
            .save(self.site, &key)
            .map_err(ConnectError::Store)?;
        Ok(connection)
    }

    /// Checks the current site's stored key. It stays stored whatever the answer.
    pub fn check_stored(&self) -> Result<Connection, ConnectError> {
        let key = self
            .store
            .read(self.site)
            .map_err(ConnectError::Store)?
            .ok_or(ConnectError::NoKey)?;
        self.client
            .check(self.site, &key)
            .map_err(ConnectError::Site)
    }

    /// Removes the current site's key; the other site's key is kept.
    pub fn remove_key(&self) -> Result<(), StoreError> {
        self.store.delete(self.site)
    }
}

#[cfg(test)]
mod tests;
