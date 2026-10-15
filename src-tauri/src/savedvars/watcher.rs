//! Watches each account's SavedVariables file under the game folder and reads it once the
//! game has finished writing it.
//!
//! A poll every few seconds looks at each account's file (`WTF/Account/<account>/
//! SavedVariables/BureaucracyForever.lua`, under the game version folder from `game_folder`,
//! looked up again on every poll so a newly chosen folder is followed). The game writes the
//! file in one go at logout or `/reload`, but it can take a moment, so a changed file is
//! read only once its size and time have stayed the same for the settle time, and a file
//! that changes again during the read is read again later. A file that is missing is
//! skipped; an unreadable one is reported once, until it changes.
//!
//! Two threads, so a busy queue never makes the reader miss a change: the poller finds,
//! reads and parses files and records each reading as soon as it is made; a second thread
//! takes the readings in turn and hands their operations to the queue, waiting for its lock
//! (held while an upload is in progress, up to the 20-second network time-out).

use super::{hand_off, read_file, HandOff, Reading, FILE_NAME};
use crate::queue::Queue;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

/// How often to look, and how long a file must stay unchanged before it is read.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    pub poll: Duration,
    pub settle: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            poll: Duration::from_secs(2),
            settle: Duration::from_secs(3),
        }
    }
}

/// What the watcher tells the app.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A file was read; its operations are on their way to the queue.
    Read(Reading),
    /// A reading's operations reached the queue.
    HandedOn { account: String, result: HandOff },
    /// A file couldn't be read (plain words); nothing from it was used.
    Failed { account: String, message: String },
}

type Latest = Arc<Mutex<BTreeMap<String, Reading>>>;

/// The running watcher, held as app state.
pub struct Watcher {
    stop: mpsc::Sender<()>,
    latest: Latest,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Watcher {
    /// Starts watching under whatever `version_folder` returns (the remembered game
    /// version folder, if any). `on_event` is called from the watcher's threads.
    pub fn start(
        version_folder: impl Fn() -> Option<PathBuf> + Send + 'static,
        queue: Arc<Mutex<Queue>>,
        timing: Timing,
        on_event: impl Fn(Event) + Send + Sync + 'static,
    ) -> Watcher {
        let on_event = Arc::new(on_event);
        let latest: Latest = Arc::default();
        let (stop, stop_rx) = mpsc::channel();
        let (jobs, jobs_rx) = mpsc::channel::<Reading>();
        let hand = {
            let on_event = Arc::clone(&on_event);
            std::thread::spawn(move || {
                for reading in jobs_rx {
                    let result = hand_off(&queue, &reading);
                    on_event(Event::HandedOn {
                        account: reading.account,
                        result,
                    });
                }
            })
        };
        let poll = {
            let latest = Arc::clone(&latest);
            std::thread::spawn(move || {
                let mut trackers: HashMap<PathBuf, Tracker> = HashMap::new();
                loop {
                    if let Some(folder) = version_folder() {
                        look(&folder, &mut trackers, timing, &latest, &jobs, &*on_event);
                    } else {
                        trackers.clear();
                    }
                    match stop_rx.recv_timeout(timing.poll) {
                        Err(RecvTimeoutError::Timeout) => {}
                        _ => return,
                    }
                }
            })
        };
        Watcher {
            stop,
            latest,
            threads: Mutex::new(vec![poll, hand]),
        }
    }

    /// The newest successful reading of each account, by account. For the raid selection
    /// (#60), which also needs `saved_at` to ignore a reading over 12 hours old.
    pub fn latest(&self) -> Vec<Reading> {
        let latest = self.latest.lock().unwrap_or_else(|e| e.into_inner());
        latest.values().cloned().collect()
    }

    /// Stops watching. Readings already made still reach the queue.
    pub fn stop(&self) {
        let _ = self.stop.send(());
    }

    /// Waits for the threads to end after `stop`.
    pub fn join(&self) {
        let threads = std::mem::take(&mut *self.threads.lock().unwrap_or_else(|e| e.into_inner()));
        for thread in threads {
            let _ = thread.join();
        }
    }
}

/// Size and time of a file: what "has stopped changing" compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Stamp {
    modified: Option<SystemTime>,
    len: u64,
}

pub(super) fn stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::metadata(path).ok().filter(|m| m.is_file())?;
    Some(Stamp {
        modified: meta.modified().ok(),
        len: meta.len(),
    })
}

/// What one file has been doing across polls.
#[derive(Default)]
pub(super) struct Tracker {
    seen: Option<Stamp>,
    since: Option<Instant>,
    handled: Option<Stamp>,
}

impl Tracker {
    /// Whether the file, as it is now (`None`: not there), should be read now: it differs
    /// from the last reading and has stayed as it is for `settle`.
    pub(super) fn ready(&mut self, now: Instant, current: Option<Stamp>, settle: Duration) -> bool {
        let Some(current) = current else {
            *self = Tracker::default();
            return false;
        };
        if self.seen != Some(current) {
            self.seen = Some(current);
            self.since = Some(now);
            return false;
        }
        self.handled != Some(current)
            && self
                .since
                .is_some_and(|since| now.duration_since(since) >= settle)
    }
}

/// One poll: every account's file under `version_folder`.
fn look(
    version_folder: &Path,
    trackers: &mut HashMap<PathBuf, Tracker>,
    timing: Timing,
    latest: &Latest,
    jobs: &mpsc::Sender<Reading>,
    on_event: &(dyn Fn(Event) + Send + Sync),
) {
    let files = account_files(version_folder);
    trackers.retain(|path, _| files.iter().any(|(_, p)| p == path));
    for (account, path) in files {
        let tracker = trackers.entry(path.clone()).or_default();
        let before = stamp(&path);
        if !tracker.ready(Instant::now(), before, timing.settle) {
            continue;
        }
        let result = read_file(&path, &account);
        // If it changed while being read, leave it unhandled: the next polls see the new
        // stamp, wait for it to settle and read again.
        if stamp(&path) != before {
            continue;
        }
        tracker.handled = before;
        match result {
            Ok(reading) => {
                latest
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(account, reading.clone());
                on_event(Event::Read(reading.clone()));
                let _ = jobs.send(reading);
            }
            Err(e) => on_event(Event::Failed {
                account,
                message: e.to_string(),
            }),
        }
    }
}

/// `(account, file)` for each account folder that has the add-on's saved file.
fn account_files(version_folder: &Path) -> Vec<(String, PathBuf)> {
    let Ok(accounts) = fs::read_dir(version_folder.join("WTF").join("Account")) else {
        return Vec::new();
    };
    let mut files: Vec<(String, PathBuf)> = accounts
        .flatten()
        .map(|a| {
            let path = a.path().join("SavedVariables").join(FILE_NAME);
            (a.file_name().to_string_lossy().into_owned(), path)
        })
        .filter(|(_, path)| path.is_file())
        .collect();
    files.sort();
    files
}
