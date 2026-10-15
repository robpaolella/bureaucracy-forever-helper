//! The helper's self-update engine, on Tauri's updater: check the published GitHub release
//! for a newer version, install a found one only when the window asks (the officer
//! confirmed), and relaunch into it only after it installed.
//!
//! The updater only ever sees the latest published release, never a draft. Every update
//! file must carry a signature from the helper's update key that names its version; one
//! that doesn't is refused before anything is installed. Every result is data the window
//! can tell apart (`kind`, and `reason` for a failure), with a plain-words message; any
//! failure leaves the installed version as it was.
//!
//! The download runs on the async runtime and takes no lock the uploader uses, so awards
//! keep sending while it runs.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri_plugin_updater::{Error, Update, Updater, UpdaterBuilder};

/// The event that carries download progress to the window.
pub const PROGRESS_EVENT: &str = "update-progress";

/// Where the officer reads what's new: the release the updater reads its update from.
pub const NOTES_URL: &str =
    "https://github.com/robpaolella/bureaucracy-forever-helper/releases/latest";

/// How long to wait to connect, and between pieces of an answer, before giving up.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);

/// The helper's own settings for an updater: a stalled check or download fails rather
/// than hanging. The commands and the tests both build theirs with it.
pub fn configure(builder: UpdaterBuilder) -> UpdaterBuilder {
    builder.timeout(READ_TIMEOUT).configure_client(|client| {
        client
            .connect_timeout(CONNECT_TIMEOUT)
            .read_timeout(READ_TIMEOUT)
    })
}

/// How checking for an update ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CheckOutcome {
    #[serde(rename_all = "camelCase")]
    Available {
        version: String,
        notes_url: String,
    },
    UpToDate,
    Failed {
        reason: Reason,
        message: String,
    },
}

/// How installing the found update ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum InstallOutcome {
    /// Installed; it runs from the next start, which `restart` gives.
    Installed {
        version: String,
    },
    /// No update was found by an earlier check, so there was nothing to install.
    NothingFound,
    /// An install is already under way.
    Busy,
    Failed {
        reason: Reason,
        message: String,
    },
}

/// How asking to restart ended. In the app a restart doesn't return, so the window only
/// ever hears `notInstalled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RestartOutcome {
    Restarting,
    /// No update has been installed, so there is nothing new to restart into.
    NotInstalled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// GitHub couldn't be reached.
    Offline,
    /// The release has no update for this computer, or its file isn't there.
    Missing,
    /// The update file's signature doesn't match the helper's key or its version.
    BadSignature,
    /// The download stopped part way.
    Interrupted,
    /// Anything else that went wrong while checking.
    CheckFailed,
    /// The downloaded update couldn't be put in place.
    InstallFailed,
}

impl Reason {
    fn message(self) -> &'static str {
        match self {
            Reason::Offline => "The helper couldn’t reach GitHub. Check your internet connection and try again.",
            Reason::Missing => "The update file couldn’t be found. Try again later.",
            Reason::BadSignature => "The update failed its safety check, so it wasn’t installed. Your current version still works.",
            Reason::Interrupted => "The download stopped part way. Try again.",
            Reason::CheckFailed => "The helper couldn’t check for updates. Try again later.",
            Reason::InstallFailed => "The update couldn’t be installed. Your current version still works.",
        }
    }
}

/// Which step an error came from: the same network error means a different thing in each.
#[derive(Clone, Copy)]
enum Step {
    Check,
    Download,
    Install,
}

fn reason(error: &Error, step: Step) -> Reason {
    match (error, step) {
        (
            Error::Minisign(_)
            | Error::Base64(_)
            | Error::SignatureUtf8(_)
            | Error::SignedVersionMismatch { .. }
            | Error::MissingSignedVersion,
            _,
        ) => Reason::BadSignature,
        (Error::Reqwest(e), Step::Download) if e.is_body() || e.is_decode() => Reason::Interrupted,
        (Error::Reqwest(e), Step::Check) if e.is_decode() => Reason::Missing,
        (Error::Reqwest(_), _) => Reason::Offline,
        (
            Error::ReleaseNotFound
            | Error::TargetNotFound(_)
            | Error::TargetsNotFound(_)
            | Error::Serialization(_)
            | Error::Semver(_),
            Step::Check,
        ) => Reason::Missing,
        // The download answered with an error status, such as 404.
        (Error::Network(_), Step::Download) => Reason::Missing,
        (_, Step::Check) => Reason::CheckFailed,
        (_, Step::Download) => Reason::Interrupted,
        (_, Step::Install) => Reason::InstallFailed,
    }
}

fn failed(error: &Error, step: Step) -> (Reason, String) {
    let reason = reason(error, step);
    (reason, reason.message().to_string())
}

/// Download progress: bytes so far, and the whole size when the server gave it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

/// The update engine, held as app state.
#[derive(Default)]
pub struct Updates {
    /// The update the last check found, for `install`.
    found: Mutex<Option<Update>>,
    /// The version installed and waiting for a restart.
    installed: Mutex<Option<String>>,
    busy: AtomicBool,
}

impl Updates {
    /// Asks the update endpoint for a newer version. Nothing is downloaded or installed.
    pub async fn check(&self, updater: Result<Updater, Error>) -> CheckOutcome {
        let checked = match updater {
            Ok(updater) => updater.check().await,
            Err(e) => Err(e),
        };
        let found = match checked {
            Ok(found) => found,
            Err(e) => {
                let (reason, message) = failed(&e, Step::Check);
                return CheckOutcome::Failed { reason, message };
            }
        };
        let outcome = match &found {
            Some(update) => CheckOutcome::Available {
                version: update.version.clone(),
                notes_url: NOTES_URL.to_string(),
            },
            None => CheckOutcome::UpToDate,
        };
        *self.found.lock().unwrap_or_else(|e| e.into_inner()) = found;
        outcome
    }

    /// Downloads the update the last check found, checks its signature and hands it to
    /// `install`, which the app gives as the updater's own install. `progress` hears each
    /// new whole percent (each piece when the size is unknown). A file that fails its
    /// signature never reaches `install`.
    pub async fn install(
        &self,
        mut progress: impl FnMut(Progress),
        install: impl FnOnce(&Update, &[u8]) -> Result<(), Error>,
    ) -> InstallOutcome {
        let Some(update) = self.found.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return InstallOutcome::NothingFound;
        };
        if self.busy.swap(true, Ordering::SeqCst) {
            return InstallOutcome::Busy;
        }
        let mut downloaded = 0u64;
        let mut last_percent = None;
        let on_chunk = |size: usize, total: Option<u64>| {
            downloaded += size as u64;
            let percent = total.map(|t| downloaded * 100 / t.max(1));
            if percent.is_none() || percent != last_percent {
                last_percent = percent;
                progress(Progress { downloaded, total });
            }
        };
        // `download` checks the signature before it returns the bytes.
        let outcome = match update.download(on_chunk, || {}).await {
            Err(e) => Err(failed(&e, Step::Download)),
            Ok(bytes) => install(&update, &bytes).map_err(|e| failed(&e, Step::Install)),
        };
        self.busy.store(false, Ordering::SeqCst);
        match outcome {
            Ok(()) => {
                *self.installed.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some(update.version.clone());
                InstallOutcome::Installed {
                    version: update.version,
                }
            }
            Err((reason, message)) => InstallOutcome::Failed { reason, message },
        }
    }

    /// Calls `relaunch` only once an update has installed.
    pub fn restart(&self, relaunch: impl FnOnce()) -> RestartOutcome {
        if self
            .installed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_none()
        {
            return RestartOutcome::NotInstalled;
        }
        relaunch();
        RestartOutcome::Restarting
    }
}

#[cfg(test)]
mod tests;
