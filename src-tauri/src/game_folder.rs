//! Finds the officer's World of Warcraft folder and the add-on inside it.
//!
//! The game keeps each version of itself (retail, classic, a beta) in its own subfolder of
//! the main game folder. There is no list of those names: a game folder is one with at
//! least one subfolder holding an `Interface` folder, and the version the officer plays is
//! whichever of those subfolders holds `Interface/AddOns/BureaucracyForever`.
//!
//! Nothing is read or written in the game folder beyond looking at folder names. The
//! functions here take the places to look, so tests build a fake layout in a temp folder.
//! What is remembered is the version folder (such as `.../World of Warcraft/_retail_`), so
//! two installed versions never need choosing between twice.

use crate::key::settings;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// The add-on's folder name, from its `BureaucracyForever.toc`.
pub const ADDON_FOLDER: &str = "BureaucracyForever";
const GAME_FOLDER: &str = "World of Warcraft";

pub const NOT_FOUND: &str = "Couldn’t find World of Warcraft. Choose the folder it’s installed in.";
pub const NOT_A_GAME_FOLDER: &str =
    "That folder isn’t a World of Warcraft folder. Choose the folder named World of Warcraft.";
pub const NO_ADDON: &str =
    "World of Warcraft is here, but the Bureaucracy Forever add-on isn’t installed.";
pub const GONE: &str = "The game folder you chose before isn’t there any more. Choose it again.";

/// One game version with the add-on installed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Install {
    /// The main World of Warcraft folder.
    pub game_folder: String,
    /// The subfolder for the version played, such as `_retail_`.
    pub version: String,
    /// The game folder and version together: the folder that is remembered.
    pub version_folder: String,
    pub addon_folder: String,
}

/// What looking for the game folder found: one result per state of the folder step.
/// ("Searching" is the time a search takes; the front-end shows it while it waits.)
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FolderOutcome {
    /// One game version has the add-on.
    Found(Install),
    /// Several do, and the officer picks.
    Several {
        installs: Vec<Install>,
    },
    NotFound {
        message: &'static str,
    },
    NotGame {
        message: &'static str,
    },
    /// A game folder, but the add-on isn't in it.
    NoAddon {
        message: &'static str,
        path: String,
    },
    /// The remembered folder is no longer there.
    Gone {
        message: &'static str,
        path: String,
    },
    /// Nothing is remembered yet.
    NothingRemembered,
    /// The folder couldn't be saved in the settings file.
    SettingsFailed {
        message: &'static str,
    },
}

fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Other,
}

/// Where the usual places are on this machine.
#[derive(Debug, Clone)]
pub struct Environment {
    pub os: Os,
    pub home: Option<PathBuf>,
    /// Windows' `Program Files` folders.
    pub program_files: Vec<PathBuf>,
    /// The root of every Windows drive that exists, such as `D:\`.
    pub drives: Vec<PathBuf>,
}

impl Environment {
    pub fn current() -> Self {
        let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
        if cfg!(target_os = "macos") {
            Environment {
                os: Os::Mac,
                home: var("HOME"),
                program_files: vec![],
                drives: vec![],
            }
        } else if cfg!(target_os = "windows") {
            let mut program_files: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)"]
                .iter()
                .filter_map(|name| var(name))
                .collect();
            if program_files.is_empty() {
                program_files.push(PathBuf::from(r"C:\Program Files"));
            }
            let drives = ('A'..='Z')
                .map(|letter| PathBuf::from(format!(r"{letter}:\")))
                .filter(|root| root.exists())
                .collect();
            Environment {
                os: Os::Windows,
                home: None,
                program_files,
                drives,
            }
        } else {
            Environment {
                os: Os::Other,
                home: var("HOME"),
                program_files: vec![],
                drives: vec![],
            }
        }
    }

    /// The folders the game is usually installed in.
    pub fn usual_places(&self) -> Vec<PathBuf> {
        match self.os {
            Os::Mac => {
                let mut places = vec![Path::new("/Applications").join(GAME_FOLDER)];
                places.extend(
                    self.home
                        .iter()
                        .map(|h| h.join("Applications").join(GAME_FOLDER)),
                );
                places
            }
            Os::Windows => {
                let mut places: Vec<PathBuf> = self
                    .program_files
                    .iter()
                    .map(|p| p.join(GAME_FOLDER))
                    .collect();
                for drive in &self.drives {
                    places.push(drive.join(GAME_FOLDER));
                    places.push(drive.join("Games").join(GAME_FOLDER));
                }
                places
            }
            Os::Other => vec![],
        }
    }
}

/// The subfolders of `folder` that hold an `Interface` folder, in name order.
fn game_versions(folder: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(folder) else {
        return vec![];
    };
    let mut versions: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.join("Interface").is_dir())
        .collect();
    versions.sort();
    versions
}

pub fn is_game_folder(folder: &Path) -> bool {
    !game_versions(folder).is_empty()
}

fn install(version_folder: &Path) -> Option<Install> {
    let addon = version_folder
        .join("Interface")
        .join("AddOns")
        .join(ADDON_FOLDER);
    let game = version_folder.parent()?;
    let version = version_folder.file_name()?.to_string_lossy().into_owned();
    addon.is_dir().then(|| Install {
        game_folder: text(game),
        version,
        version_folder: text(version_folder),
        addon_folder: text(&addon),
    })
}

fn outcome(mut installs: Vec<Install>) -> Option<FolderOutcome> {
    match installs.len() {
        0 => None,
        1 => installs.pop().map(FolderOutcome::Found),
        _ => Some(FolderOutcome::Several { installs }),
    }
}

/// A folder the officer picked: it must be a game folder, and some version in it must have
/// the add-on.
pub fn check(folder: &Path) -> FolderOutcome {
    let versions = game_versions(folder);
    if versions.is_empty() {
        return FolderOutcome::NotGame {
            message: NOT_A_GAME_FOLDER,
        };
    }
    let installs = versions.iter().filter_map(|v| install(v)).collect();
    outcome(installs).unwrap_or_else(|| FolderOutcome::NoAddon {
        message: NO_ADDON,
        path: text(folder),
    })
}

/// Looks in each place for a game folder; every version with the add-on counts, across all
/// the places. If none has it but a game folder was found, that is reported instead.
pub fn search(places: &[PathBuf]) -> FolderOutcome {
    let mut installs: Vec<Install> = vec![];
    let mut game_without_addon = None;
    for place in places {
        match check(place) {
            FolderOutcome::Found(one) => installs.push(one),
            FolderOutcome::Several { installs: more } => installs.extend(more),
            FolderOutcome::NoAddon { path, .. } => {
                game_without_addon.get_or_insert(path);
            }
            _ => {}
        }
    }
    installs.dedup_by(|a, b| a.version_folder == b.version_folder);
    outcome(installs).unwrap_or(match game_without_addon {
        Some(path) => FolderOutcome::NoAddon {
            message: NO_ADDON,
            path,
        },
        None => FolderOutcome::NotFound { message: NOT_FOUND },
    })
}

/// The remembered version folder, checked again: it may be gone, or have lost the add-on.
pub fn recheck(version_folder: &Path) -> FolderOutcome {
    if !version_folder.is_dir() {
        return FolderOutcome::Gone {
            message: GONE,
            path: text(version_folder),
        };
    }
    match install(version_folder) {
        Some(found) => FolderOutcome::Found(found),
        None => FolderOutcome::NoAddon {
            message: NO_ADDON,
            path: version_folder.parent().map_or_else(String::new, text),
        },
    }
}

/// The settings file's remembered game folder.
pub struct GameFolderStore {
    settings_path: PathBuf,
}

impl GameFolderStore {
    pub fn new(settings_path: impl Into<PathBuf>) -> Self {
        GameFolderStore {
            settings_path: settings_path.into(),
        }
    }

    /// The remembered folder, checked again. Never replaced by a search.
    pub fn remembered(&self) -> FolderOutcome {
        match settings::read_game_folder(&self.settings_path) {
            Some(folder) => recheck(Path::new(&folder)),
            None => FolderOutcome::NothingRemembered,
        }
    }

    /// Remembers a version folder, but only one that has the add-on; anything else is
    /// returned as it checks and nothing is saved.
    pub fn remember(&self, version_folder: &Path) -> FolderOutcome {
        let result = recheck(version_folder);
        if let FolderOutcome::Found(found) = &result {
            if settings::write_game_folder(&self.settings_path, &found.version_folder).is_err() {
                return FolderOutcome::SettingsFailed {
                    message: "The helper couldn’t save that folder. Try again.",
                };
            }
        }
        result
    }
}

#[cfg(test)]
mod tests;
