//! Keeps the add-on up to date. The site's `GET /api/helper/me` names the add-on's latest
//! version (ADDON-SPEC 7.1); the add-on's GitHub release for that version, read through
//! GitHub's public releases API, gives its zip and its "Before you update" note.
//!
//! An install downloads that zip, checks it, and swaps it in whole (`install`), keeping the
//! officer's inbox. Any failure leaves the old add-on and its inbox untouched, and nothing
//! here tries again by itself: the officer's "Try again" or the next release does. The same
//! call installs the add-on into a game version folder that has none.

mod install;

pub use install::installed_version;
use install::PlaceError;
use serde::Serialize;
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// The event that carries install progress to the window.
pub const PROGRESS_EVENT: &str = "addon-progress";

/// A release's address is this followed by its tag, `v<version>`.
const RELEASES: &str =
    "https://api.github.com/repos/robpaolella/bureaucracy-forever-addon/releases/tags/v";
const MAX_RELEASE: u64 = 1024 * 1024;
const MAX_ZIP: u64 = 20 * 1024 * 1024;

/// The release matching the site's version: its zip and its note, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub zip_url: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// The site didn't give the latest version (no key, refused, or unreachable).
    Site,
    /// No release matches the site's version, or it has no zip.
    NoRelease,
    /// GitHub couldn't be reached, or the download stopped.
    Offline,
    /// The download isn't the add-on at that version.
    BadDownload,
    /// The folder isn't a game version folder.
    NotGameFolder,
    /// The new version couldn't be put in place.
    InstallFailed,
}

impl Reason {
    fn message(self) -> &'static str {
        match self {
            Reason::Site => {
                "The helper couldn’t get the add-on’s latest version from the guild site."
            }
            Reason::NoRelease => {
                "The add-on’s latest version isn’t ready to download yet. Try again later."
            }
            Reason::Offline => {
                "The helper couldn’t reach GitHub. Check your internet connection and try again."
            }
            Reason::BadDownload => {
                "The download wasn’t the add-on it should be, so it wasn’t installed."
            }
            Reason::NotGameFolder => "That folder isn’t a World of Warcraft game folder.",
            Reason::InstallFailed => "The add-on couldn’t be put in place.",
        }
    }
}

/// How checking for an add-on update ended. `installed` is None when there is no add-on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AddonCheck {
    #[serde(rename_all = "camelCase")]
    Checked {
        latest: String,
        /// The release's "Before you update" section; None when it has none.
        note: Option<String>,
        installed: Option<String>,
        up_to_date: bool,
    },
    Failed {
        reason: Reason,
        message: &'static str,
        installed: Option<String>,
    },
}

/// How an install ended. On failure `kept` is the version still in place, None if none was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AddonInstall {
    Installed {
        version: String,
    },
    /// No check has found a release, so there was nothing to install.
    NothingFound,
    /// An install is already under way.
    Busy,
    NotInstalled {
        reason: Reason,
        message: &'static str,
        kept: Option<String>,
    },
}

/// Where an install is: downloading (bytes so far, and the size when known), then installing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum Progress {
    Downloading {
        version: String,
        downloaded: u64,
        total: Option<u64>,
    },
    Installing {
        version: String,
    },
}

/// The release's "Before you update" section, without its heading.
fn before_you_update(notes: &str) -> Option<String> {
    let mut lines = notes.lines().map(str::trim_end);
    lines.find(|line| line.trim() == "## Before you update")?;
    let section: Vec<&str> = lines.take_while(|line| !line.starts_with("## ")).collect();
    Some(section.join("\n").trim().to_string()).filter(|s| !s.is_empty())
}

/// Reads a release from GitHub's answer: its zip must be `BureaucracyForever-<version>.zip`.
pub fn release_from(answer: &Value, version: &str) -> Option<Release> {
    let name = format!("BureaucracyForever-{version}.zip");
    let zip_url = answer["assets"]
        .as_array()?
        .iter()
        .find(|asset| asset["name"].as_str() == Some(name.as_str()))?["browser_download_url"]
        .as_str()?
        .to_string();
    Some(Release {
        version: version.to_string(),
        zip_url,
        note: answer["body"].as_str().and_then(before_you_update),
    })
}

/// GitHub's public releases API and the release downloads; no login.
pub struct GitHub {
    agent: ureq::Agent,
    releases: String,
}

impl Default for GitHub {
    fn default() -> Self {
        GitHub::with_address(RELEASES, Duration::from_secs(60))
    }
}

impl GitHub {
    /// Reads releases at `releases` followed by `v<version>`, giving up after `timeout`.
    pub fn with_address(releases: &str, timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(timeout))
            .user_agent("bureaucracy-forever-helper")
            .build()
            .into();
        GitHub {
            agent,
            releases: releases.to_string(),
        }
    }

    pub fn release(&self, version: &str) -> Result<Release, Reason> {
        let version = version.trim().trim_start_matches('v');
        // The version goes into an address, so only a plain version is asked for.
        let plain = |c: char| c.is_ascii_alphanumeric() || c == '.' || c == '-';
        if version.is_empty() || !version.chars().all(plain) {
            return Err(Reason::NoRelease);
        }
        let response = self
            .agent
            .get(format!("{}{version}", self.releases))
            .header("Accept", "application/vnd.github+json")
            .call();
        let mut response = match response {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(404)) => return Err(Reason::NoRelease),
            Err(_) => return Err(Reason::Offline),
        };
        let text = response
            .body_mut()
            .with_config()
            .limit(MAX_RELEASE)
            .read_to_string()
            .map_err(|_| Reason::Offline)?;
        let answer: Value = serde_json::from_str(&text).map_err(|_| Reason::NoRelease)?;
        release_from(&answer, version).ok_or(Reason::NoRelease)
    }

    /// Downloads the zip, calling `progress` with each new whole percent (each piece when the
    /// size is unknown).
    pub fn download(
        &self,
        url: &str,
        mut progress: impl FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>, Reason> {
        let mut response = self.agent.get(url).call().map_err(|_| Reason::Offline)?;
        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok()?.parse().ok());
        let mut reader = response.body_mut().with_config().limit(MAX_ZIP).reader();
        let (mut bytes, mut piece, mut last) = (vec![], [0u8; 64 * 1024], None);
        loop {
            let read = reader.read(&mut piece).map_err(|_| Reason::Offline)?;
            if read == 0 {
                return Ok(bytes);
            }
            bytes.extend_from_slice(&piece[..read]);
            let percent = total.map(|t: u64| bytes.len() as u64 * 100 / t.max(1));
            if percent.is_none() || percent != last {
                last = percent;
                progress(bytes.len() as u64, total);
            }
        }
    }
}

/// The add-on update engine, held as app state.
pub struct AddonUpdates {
    github: GitHub,
    /// The release the last check found, for `install`.
    found: Mutex<Option<Release>>,
    busy: AtomicBool,
}

impl AddonUpdates {
    pub fn new(github: GitHub) -> Self {
        AddonUpdates {
            github,
            found: Mutex::default(),
            busy: AtomicBool::new(false),
        }
    }

    /// Compares the site's `latest` version (None when the site didn't give it) with the add-on
    /// in `version_folder`, and finds its release. Nothing is downloaded.
    pub fn check(&self, latest: Option<String>, version_folder: &Path) -> AddonCheck {
        let installed = installed_version(version_folder);
        let found = latest
            .ok_or(Reason::Site)
            .and_then(|latest| self.github.release(&latest));
        let outcome = match &found {
            Ok(release) => AddonCheck::Checked {
                latest: release.version.clone(),
                note: release.note.clone(),
                up_to_date: installed.as_deref() == Some(release.version.as_str()),
                installed,
            },
            Err(reason) => AddonCheck::Failed {
                reason: *reason,
                message: reason.message(),
                installed,
            },
        };
        *self.found.lock().unwrap_or_else(|e| e.into_inner()) = found.ok();
        outcome
    }

    /// Installs the release the last check found into `version_folder`, telling `progress`
    /// each step. Called again after a failure, it tries again; it never retries by itself.
    pub fn install(
        &self,
        version_folder: &Path,
        mut progress: impl FnMut(Progress),
    ) -> AddonInstall {
        let Some(release) = self.found.lock().unwrap_or_else(|e| e.into_inner()).clone() else {
            return AddonInstall::NothingFound;
        };
        if self.busy.swap(true, Ordering::SeqCst) {
            return AddonInstall::Busy;
        }
        let version = release.version.clone();
        let result = self
            .github
            .download(&release.zip_url, |downloaded, total| {
                progress(Progress::Downloading {
                    version: version.clone(),
                    downloaded,
                    total,
                })
            })
            .and_then(|bytes| {
                let zip = install::check(&bytes, &version).ok_or(Reason::BadDownload)?;
                progress(Progress::Installing {
                    version: version.clone(),
                });
                install::put_in_place(version_folder, zip).map_err(|e| match e {
                    PlaceError::NotGameFolder => Reason::NotGameFolder,
                    PlaceError::Failed => Reason::InstallFailed,
                })
            });
        self.busy.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => AddonInstall::Installed { version },
            Err(reason) => AddonInstall::NotInstalled {
                reason,
                message: reason.message(),
                kept: installed_version(version_folder),
            },
        }
    }
}

#[cfg(test)]
mod tests;
