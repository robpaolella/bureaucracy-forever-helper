//! Keeps the add-on's inbox current: for every raid on the upcoming list it fetches the
//! snapshot and the catch-up (ADDON-SPEC 7.1), writes them with the inbox writer, and tells
//! the window when the officer has to type `/reload` for the game to read them.
//!
//! Each pass waits for the raids list to be good, fetches every raid's two documents one
//! request at a time, and only then compares the result with what was last written, ignoring
//! the `written` time. A result that is the same writes nothing and says nothing. Any failed
//! fetch ends the pass with the old file untouched, so the game never gets half a list.
//!
//! The site allows a key 30 requests a minute (7.6). The raids list costs one a minute, the
//! uploader a few, and this timer never sends two requests closer than `spacing`: at most 20 a
//! minute whatever the number of raids. A `429` is waited out as the site asks.
//!
//! The first file written (none there) is "in the game"; a later change is a reload prompt,
//! and so is a file on disk from an earlier run that differs. The prompt stays until the
//! game saves its variables after the write, which it does at `/reload`.

use crate::commands::Engine;
use crate::inbox::{self, OnDisk, RaidData};
use crate::key::{HelperKey, SecretStore, Site, SiteClient, SiteError};
use crate::raids::{Raid, RaidsFailure, RaidsState, RaidsView};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The event that carries each new reserves view to the window.
pub const RESERVES_EVENT: &str = "reserves";

/// Which document of a raid to fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Part {
    Snapshot,
    Catchup,
}

/// Where a raid's documents come from: the site client, or a fake one in tests.
pub trait ReservesSource {
    fn fetch(
        &self,
        site: Site,
        key: &HelperKey,
        raid_id: &str,
        part: Part,
    ) -> Result<Value, RaidsFailure>;
}

impl ReservesSource for SiteClient {
    fn fetch(
        &self,
        site: Site,
        key: &HelperKey,
        raid_id: &str,
        part: Part,
    ) -> Result<Value, RaidsFailure> {
        let part = match part {
            Part::Snapshot => "snapshot",
            Part::Catchup => "catchup",
        };
        self.raid_document(site, key, raid_id, part)
    }
}

/// How the reserves stand in the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReservesState {
    /// Nothing written since the app, the key or the site changed, or no raids to prepare.
    Waiting,
    /// These raids' reserves are in the inbox and nothing has changed since.
    InGame { raids: Vec<String> },
    /// The inbox changed and the game hasn't read it yet: `changed` are the raids that did.
    Reload {
        raids: Vec<String>,
        changed: Vec<String>,
    },
}

/// The reserves and what happened to them, as the commands and event carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReservesView {
    pub state: ReservesState,
    /// Goes up with every write the officer should hear about (the first, or a change), so
    /// the window raises one notice per count however often it is told.
    pub changes: u64,
    /// Whether the latest counted write was the first.
    pub first: bool,
    /// Why the latest write was refused, in plain words; the old file is as it was.
    pub problem: Option<String>,
}

impl ReservesView {
    fn waiting() -> Self {
        ReservesView {
            state: ReservesState::Waiting,
            changes: 0,
            first: false,
            problem: None,
        }
    }
}

/// How long to wait between requests and passes.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Between passes: reserves are polled no more often than every 30 s.
    pub tick: Duration,
    /// The least gap between two requests.
    pub spacing: Duration,
    /// While the raids list isn't known yet.
    pub pending: Duration,
    /// After the site said the helper is off, or the secure store failed.
    pub slow: Duration,
    /// After a refused key: until the key or site changes.
    pub stopped: Duration,
    /// The longest wait taken on the site's `Retry-After`.
    pub max_retry_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            tick: Duration::from_secs(30),
            spacing: Duration::from_secs(3),
            pending: Duration::from_secs(2),
            slow: Duration::from_secs(300),
            stopped: Duration::from_secs(24 * 3600),
            max_retry_after: Duration::from_secs(3600),
        }
    }
}

impl Timing {
    /// The most requests a minute this timer can send, however many raids there are.
    pub fn most_per_minute(&self) -> u64 {
        60_000 / (self.spacing.as_millis().max(1) as u64)
    }

    /// How long to wait after a failed fetch: the site's `Retry-After` if it gave one, but
    /// never less than the tick or more than `max_retry_after`.
    fn after_failure(&self, failure: &RaidsFailure) -> Duration {
        match &failure.error {
            SiteError::KeyRefused(_) => self.stopped,
            SiteError::HelperOff | SiteError::StagingNotOpen => self.slow,
            SiteError::Unreachable { .. } => failure
                .retry_after
                .map_or(self.tick, |s| {
                    Duration::from_secs(s).min(self.max_retry_after)
                })
                .max(self.tick),
        }
    }
}

#[derive(Default)]
struct Control {
    /// Goes up on every restart, so a pass can tell its key and site are out of date.
    generation: u64,
    stopped: bool,
}

struct Inner {
    view: ReservesView,
    /// When the latest write was made, Unix seconds.
    written_at: i64,
}

struct Shared {
    control: Mutex<Control>,
    wake: Condvar,
    inner: Mutex<Inner>,
}

impl Shared {
    fn control(&self) -> MutexGuard<'_, Control> {
        self.control.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn outdated(&self, generation: u64) -> bool {
        let control = self.control();
        control.stopped || control.generation != generation
    }

    /// Waits for `wait`; false if a stop or restart came first.
    fn sleep(&self, generation: u64, wait: Duration) -> bool {
        let deadline = Instant::now() + wait;
        let mut control = self.control();
        while !control.stopped && control.generation == generation {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return true;
            }
            control = self
                .wake
                .wait_timeout(control, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        false
    }
}

/// The running reserves timer, held as app state.
#[derive(Clone)]
pub struct Reserves {
    shared: Arc<Shared>,
    on_change: Arc<dyn Fn(&ReservesView) + Send + Sync>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

/// What the timer reads from the app.
pub struct Inputs<R, V, T, N> {
    pub source: R,
    /// The upcoming raids list.
    pub raids: V,
    /// The inbox file, once a game folder is known.
    pub target: T,
    /// The time now, Unix seconds.
    pub now: N,
    pub timing: Timing,
}

impl Reserves {
    /// Starts the timer with the key and site from `engine`. `on_change` hears every change
    /// of the view.
    pub fn start<S, R, V, T, N>(
        engine: Engine<S>,
        inputs: Inputs<R, V, T, N>,
        on_change: impl Fn(&ReservesView) + Send + Sync + 'static,
    ) -> Reserves
    where
        S: SecretStore + Send + Sync + 'static,
        R: ReservesSource + Send + 'static,
        V: Fn() -> RaidsView + Send + 'static,
        T: Fn() -> Option<PathBuf> + Send + 'static,
        N: Fn() -> i64 + Send + 'static,
    {
        let shared = Arc::new(Shared {
            control: Mutex::default(),
            wake: Condvar::new(),
            inner: Mutex::new(Inner {
                view: ReservesView::waiting(),
                written_at: 0,
            }),
        });
        let on_change: Arc<dyn Fn(&ReservesView) + Send + Sync> = Arc::new(on_change);
        let thread = {
            let (shared, on_change) = (Arc::clone(&shared), Arc::clone(&on_change));
            std::thread::spawn(move || run(&shared, &engine, inputs, &*on_change))
        };
        Reserves {
            shared,
            on_change,
            thread: Arc::new(Mutex::new(Some(thread))),
        }
    }

    /// How the reserves stand now.
    pub fn view(&self) -> ReservesView {
        self.shared.inner().view.clone()
    }

    /// The key or site changed: forget what was written and fetch again now.
    pub fn restart(&self) {
        let view = {
            // The inner state first, then the control, the order a pass publishes in.
            let mut inner = self.shared.inner();
            self.shared.control().generation += 1;
            // The count keeps rising, so a notice already raised isn't raised again.
            inner.view = ReservesView {
                changes: inner.view.changes,
                ..ReservesView::waiting()
            };
            inner.view.clone()
        };
        self.shared.wake.notify_all();
        (self.on_change)(&view);
    }

    /// The game saved its variables at `saved_at` (Unix seconds), which it does at `/reload`
    /// and logout: a reload that was waiting for the latest write has happened.
    pub fn game_saved(&self, saved_at: i64) {
        let mut inner = self.shared.inner();
        if saved_at < inner.written_at {
            return;
        }
        if let ReservesState::Reload { raids, .. } = &inner.view.state {
            inner.view.state = ReservesState::InGame {
                raids: raids.clone(),
            };
            let view = inner.view.clone();
            drop(inner);
            (self.on_change)(&view);
        }
    }

    /// Stops the timer. It doesn't wait for a request in flight, whose answer is dropped.
    pub fn stop(&self) {
        self.shared.control().stopped = true;
        self.shared.wake.notify_all();
    }

    /// Waits for the timer to end after `stop`.
    pub fn join(&self) {
        let thread = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

/// One raid as last written: enough to tell whether it changed and to name it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    id: String,
    name: String,
    /// The raid's bytes in the file; empty if it can't be written.
    bytes: Vec<u8>,
}

enum Write {
    First,
    Changed(Vec<String>),
}

fn run<S, R, V, T, N>(
    shared: &Shared,
    engine: &Engine<S>,
    inputs: Inputs<R, V, T, N>,
    on_change: &dyn Fn(&ReservesView),
) where
    S: SecretStore,
    R: ReservesSource,
    V: Fn() -> RaidsView,
    T: Fn() -> Option<PathBuf>,
    N: Fn() -> i64,
{
    let Inputs {
        source,
        raids,
        target,
        now,
        timing,
    } = inputs;
    let mut last: Option<Vec<Entry>> = None;
    let mut seen = 0;
    loop {
        let generation = {
            let control = shared.control();
            if control.stopped {
                return;
            }
            control.generation
        };
        if generation != seen {
            // A new key or site: what was written belongs to the old one.
            (last, seen) = (None, generation);
        }
        // The change is only kept if no restart came meanwhile.
        let publish = |change: &dyn Fn(&mut Inner)| {
            let mut inner = shared.inner();
            if shared.outdated(generation) {
                return;
            }
            let before = inner.view.clone();
            change(&mut inner);
            let after = inner.view.clone();
            drop(inner);
            if after != before {
                on_change(&after);
            }
        };
        let wait = match engine.upload_target() {
            (_, Err(_)) => timing.slow,
            (_, Ok(None)) => timing.tick,
            (site, Ok(Some(key))) => {
                let list = raids();
                match (&list.state, target()) {
                    (RaidsState::Loading, _) => timing.pending,
                    (RaidsState::Fresh | RaidsState::Empty, Some(path)) => {
                        let fetched =
                            fetch_all(shared, generation, &source, site, &key, &list.raids, timing);
                        match fetched {
                            Err(wait) => wait,
                            Ok(data) => {
                                let entries = entries_of(&list.raids, &data);
                                settle(&path, &data, entries, &mut last, now(), &publish);
                                timing.tick
                            }
                        }
                    }
                    _ => timing.tick,
                }
            }
        };
        shared.sleep(generation, wait);
    }
}

/// Fetches every raid's snapshot and catch-up, one request at a time at least `spacing`
/// apart. On a failure, or a restart, gives the time to wait before the next pass.
fn fetch_all(
    shared: &Shared,
    generation: u64,
    source: &impl ReservesSource,
    site: Site,
    key: &HelperKey,
    raids: &[Raid],
    timing: Timing,
) -> Result<Vec<RaidData>, Duration> {
    let mut requests = 0;
    let mut get = |raid: &Raid, part| {
        if requests > 0 && !shared.sleep(generation, timing.spacing) {
            return Err(Duration::ZERO);
        }
        requests += 1;
        source
            .fetch(site, key, &raid.id, part)
            .map_err(|failure| timing.after_failure(&failure))
    };
    let mut data = Vec::with_capacity(raids.len());
    for raid in raids {
        let snapshot = get(raid, Part::Snapshot)?;
        let catchup = get(raid, Part::Catchup)?;
        data.push(RaidData {
            snapshot,
            catchup: Some(catchup),
        });
    }
    Ok(data)
}

fn entries_of(raids: &[Raid], data: &[RaidData]) -> Vec<Entry> {
    raids
        .iter()
        .zip(data)
        .map(|(raid, data)| Entry {
            id: raid.id.clone(),
            name: raid.name.clone(),
            bytes: inbox::render_inbox(std::slice::from_ref(data), 0).unwrap_or_default(),
        })
        .collect()
}

fn names(entries: &[Entry]) -> Vec<String> {
    entries.iter().map(|e| e.name.clone()).collect()
}

fn in_game(entries: &[Entry]) -> ReservesState {
    match entries.is_empty() {
        true => ReservesState::Waiting,
        false => ReservesState::InGame {
            raids: names(entries),
        },
    }
}

/// Applies a change to the shared state, if no restart came meanwhile, and tells the window.
type Publish<'a> = dyn Fn(&dyn Fn(&mut Inner)) + 'a;

/// Compares the fetched `data` with what was last written (or, after a start, with the file
/// on disk) and writes it only if it really changed, publishing what the window should show.
fn settle(
    path: &Path,
    data: &[RaidData],
    entries: Vec<Entry>,
    last: &mut Option<Vec<Entry>>,
    now: i64,
    publish: &Publish<'_>,
) {
    let action = match last.as_ref() {
        Some(old) if *old == entries => None,
        Some(old) => Some(Write::Changed(changed_names(old, &entries))),
        None => match inbox::compare_on_disk(path, data) {
            OnDisk::Same => None,
            OnDisk::Missing => Some(Write::First),
            OnDisk::Different => Some(Write::Changed(names(&entries))),
        },
    };
    let Some(action) = action else {
        // Nothing to write. After a start, the file on disk is already right.
        let first_look = last.is_none();
        *last = Some(entries.clone());
        publish(&|inner| {
            inner.view.problem = None;
            if first_look {
                inner.view.state = in_game(&entries);
            }
        });
        return;
    };
    match inbox::write_inbox(path, data, now) {
        Err(error) => publish(&|inner| inner.view.problem = Some(error.to_string())),
        Ok(()) => {
            *last = Some(entries.clone());
            publish(&|inner| {
                inner.written_at = now;
                inner.view.problem = None;
                if entries.is_empty() {
                    // Nothing to load, so nothing to prompt for.
                    inner.view.state = ReservesState::Waiting;
                    return;
                }
                inner.view.changes += 1;
                match &action {
                    Write::First => {
                        inner.view.first = true;
                        inner.view.state = in_game(&entries);
                    }
                    Write::Changed(changed) => {
                        inner.view.first = false;
                        inner.view.state = ReservesState::Reload {
                            raids: names(&entries),
                            changed: changed.clone(),
                        };
                    }
                }
            });
        }
    }
}

/// The raids that are new or different, then those that were dropped; every raid if only
/// the order changed.
fn changed_names(old: &[Entry], new: &[Entry]) -> Vec<String> {
    let differs = |e: &&Entry| !old.contains(e);
    let gone = old.iter().filter(|o| !new.iter().any(|e| e.id == o.id));
    let changed: Vec<String> = new
        .iter()
        .filter(differs)
        .chain(gone)
        .map(|e| e.name.clone())
        .collect();
    match changed.is_empty() {
        true => names(new),
        false => changed,
    }
}

#[cfg(test)]
mod tests;
