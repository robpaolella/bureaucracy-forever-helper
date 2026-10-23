//! Tells whether the game is running, and since when. The game is found by where its
//! executable lives: a process whose executable file is inside the remembered game folder
//! is the game. Process names for the Forever client are unchecked and not used.

use std::path::{Path, PathBuf};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

/// Whether the game is running now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameStatus {
    /// A game process is running; it started at this time, Unix seconds (the latest if
    /// there are several).
    Running {
        started: i64,
    },
    NotRunning,
    /// It can't be told: no game folder is remembered, or the processes couldn't be read.
    Unknown,
}

/// One process as the system lists it: where its executable is, if that can be read, and
/// when it started (Unix seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub exe: Option<PathBuf>,
    pub started: i64,
}

/// Looks at the system's processes for one running from inside `folder`.
pub fn check(folder: &Path) -> GameStatus {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::Always),
    );
    let processes: Vec<Process> = system
        .processes()
        .values()
        .map(|p| Process {
            exe: p.exe().map(Path::to_path_buf),
            started: i64::try_from(p.start_time()).unwrap_or(i64::MAX),
        })
        .collect();
    classify(folder, &processes)
}

/// The answer for `folder` from a list of processes. Processes whose executable can't be
/// read (the system's own, other users') only make the answer unknown when none of the
/// others is the game, and an empty list is unknown too: the game runs as the officer, so
/// its path is readable, but a failed lookup must never hide a needed `/reload`.
pub fn classify(folder: &Path, processes: &[Process]) -> GameStatus {
    let folder = normal(folder);
    let started = processes
        .iter()
        .filter(|p| {
            p.exe
                .as_deref()
                .is_some_and(|e| normal(e).starts_with(&folder))
        })
        .map(|p| p.started)
        .max();
    match started {
        Some(started) => GameStatus::Running { started },
        None if processes.is_empty() || processes.iter().any(|p| p.exe.is_none()) => {
            GameStatus::Unknown
        }
        None => GameStatus::NotRunning,
    }
}

/// Paths compare without case on Windows and macOS, whose disks do not tell them apart.
fn normal(path: &Path) -> PathBuf {
    if cfg!(any(windows, target_os = "macos")) {
        PathBuf::from(path.to_string_lossy().to_lowercase())
    } else {
        path.to_path_buf()
    }
}

#[cfg(test)]
mod tests;
