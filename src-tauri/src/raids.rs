//! The upcoming raids list: every site raid starting within 12 hours or under way
//! (ADDON-SPEC 7.5), kept current by a timer that starts with the app, so the list stays
//! fresh while the window is hidden. The add-on picks the raid itself (2.1); nothing here
//! chooses one.
//!
//! Each pass reads the current site and that site's stored key together from the key engine,
//! as the uploader does. Connecting, removing the key or switching site calls `restart`,
//! which drops the list and starts again. A failed fetch keeps the last good list. A refused
//! key stops fetching until the key or site changes. `Poller::view` is the read for the
//! window's commands and for the inbox and chat log work.

use crate::commands::Engine;
use crate::key::{Refusal, SecretStore, Site, SiteClient, SiteError};
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The event that carries each new raids view to the window.
pub const RAIDS_EVENT: &str = "raids";

/// One upcoming raid, as the site lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Raid {
    pub id: String,
    pub code: String,
    pub name: String,
    /// Unix seconds.
    pub starts_at: i64,
    pub duration_min: i64,
    /// The game instance number; the raid's template may have none.
    pub instance: Option<i64>,
    pub has_loot_table: bool,
}

impl Raid {
    /// Reads the site's answer: a list of raids, or `{ "raids": [ ... ] }`. `None` if it is
    /// neither, or an entry lacks any field but `instance` (7.5); one bad entry spoils the
    /// whole answer, so the helper never shows a list the site didn't send.
    pub fn parse_list(body: &Value) -> Option<Vec<Raid>> {
        let list = body.as_array().or_else(|| body["raids"].as_array())?;
        list.iter().map(Raid::parse).collect()
    }

    fn parse(entry: &Value) -> Option<Raid> {
        let text = |field: &str| entry[field].as_str().map(str::to_string);
        Some(Raid {
            id: text("id").filter(|s| !s.is_empty())?,
            code: text("code")?,
            name: text("name")?,
            starts_at: entry["startsAt"].as_i64()?,
            duration_min: entry["durationMin"].as_i64()?,
            // The one field a raid may lack (7.5).
            instance: entry["instance"].as_i64(),
            has_loot_table: entry["hasLootTable"].as_bool()?,
        })
    }
}

/// How the list stands. `raids` in a view is the last good list in every state but
/// `loading`, `noKey` and `keyUnreadable`, where there is none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RaidsState {
    /// Nothing fetched yet since the app, the key or the site changed.
    Loading,
    /// The site gave raids.
    Fresh,
    /// The site gave no raids, which is a good answer.
    Empty,
    /// The last fetch failed (no answer, a server error, a limit, or an unreadable answer);
    /// the last good list is kept and the next tick tries again.
    Unreachable,
    /// No key is stored for the current site; nothing is fetched.
    NoKey,
    /// The site refused the key (`401` or `403`); nothing is fetched until the key or site
    /// changes.
    KeyRefused,
    /// The site has the helper switched off.
    SiteOff,
    /// Staging isn't open to the public yet.
    StagingNotOpen,
    /// The secure store couldn't be read.
    #[serde(rename_all = "camelCase")]
    KeyUnreadable { message: String },
}

/// The list and how it stands, as the commands and event carry it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RaidsView {
    pub state: RaidsState,
    pub raids: Vec<Raid>,
}

impl RaidsView {
    fn loading() -> Self {
        RaidsView {
            state: RaidsState::Loading,
            raids: Vec::new(),
        }
    }

    /// The view after a fetch that gave `result`, from the one before.
    fn after(self, result: Result<Vec<Raid>, SiteError>) -> RaidsView {
        match result {
            Ok(raids) => RaidsView {
                state: if raids.is_empty() {
                    RaidsState::Empty
                } else {
                    RaidsState::Fresh
                },
                raids,
            },
            Err(e) => RaidsView {
                state: match e {
                    SiteError::KeyRefused(Refusal::BadKey | Refusal::NotOfficer) => {
                        RaidsState::KeyRefused
                    }
                    SiteError::HelperOff => RaidsState::SiteOff,
                    SiteError::StagingNotOpen => RaidsState::StagingNotOpen,
                    SiteError::Unreachable { .. } => RaidsState::Unreachable,
                },
                raids: self.raids,
            },
        }
    }
}

/// A fetch that gave no list: why, and how long the site asked to be left alone
/// (`Retry-After`, in seconds).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RaidsFailure {
    pub error: SiteError,
    pub retry_after: Option<u64>,
}

impl From<SiteError> for RaidsFailure {
    fn from(error: SiteError) -> Self {
        RaidsFailure {
            error,
            retry_after: None,
        }
    }
}

/// Where raids come from: the site client, or a fake one in tests.
pub trait RaidsSource {
    fn raids(&self, site: Site, key: &crate::key::HelperKey) -> Result<Vec<Raid>, RaidsFailure>;
}

impl RaidsSource for SiteClient {
    fn raids(&self, site: Site, key: &crate::key::HelperKey) -> Result<Vec<Raid>, RaidsFailure> {
        SiteClient::raids(self, site, key)
    }
}

/// How long to wait between passes.
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// The normal gap: the spec expects polling every 30–60 s and limits a key to 30
    /// requests a minute (7.6).
    pub tick: Duration,
    /// After the site said the helper is off, or the secure store failed: soon enough to
    /// recover on its own, rarely enough not to keep asking.
    pub slow: Duration,
    /// After a refused key: until the key or site changes.
    pub stopped: Duration,
    /// The longest wait taken on the site's `Retry-After`.
    pub max_retry_after: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Timing {
            tick: Duration::from_secs(60),
            slow: Duration::from_secs(300),
            stopped: Duration::from_secs(24 * 3600),
            max_retry_after: Duration::from_secs(3600),
        }
    }
}

#[derive(Default)]
struct Control {
    /// Goes up on every restart, so a pass can tell its key and site are out of date.
    generation: u64,
    stopped: bool,
}

struct Shared {
    control: Mutex<Control>,
    wake: Condvar,
    view: Mutex<RaidsView>,
}

impl Shared {
    fn control(&self) -> MutexGuard<'_, Control> {
        self.control.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn view(&self) -> MutexGuard<'_, RaidsView> {
        self.view.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn outdated(&self, generation: u64) -> bool {
        let control = self.control();
        control.stopped || control.generation != generation
    }

    /// Waits for `wait`, or less if stopped or restarted.
    fn sleep(&self, generation: u64, wait: Duration) {
        let deadline = Instant::now() + wait;
        let mut control = self.control();
        while !control.stopped && control.generation == generation {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            control = self
                .wake
                .wait_timeout(control, left)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

/// The running raids timer, held as app state.
#[derive(Clone)]
pub struct Poller {
    shared: Arc<Shared>,
    on_change: Arc<dyn Fn(&RaidsView) + Send + Sync>,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Poller {
    /// Starts fetching with the key and site from `engine`. `on_change` hears every change
    /// of the view.
    pub fn start<S, R>(
        engine: Engine<S>,
        source: R,
        timing: Timing,
        on_change: impl Fn(&RaidsView) + Send + Sync + 'static,
    ) -> Poller
    where
        S: SecretStore + Send + Sync + 'static,
        R: RaidsSource + Send + 'static,
    {
        let shared = Arc::new(Shared {
            control: Mutex::default(),
            wake: Condvar::new(),
            view: Mutex::new(RaidsView::loading()),
        });
        let on_change: Arc<dyn Fn(&RaidsView) + Send + Sync> = Arc::new(on_change);
        let thread = {
            let (shared, on_change) = (Arc::clone(&shared), Arc::clone(&on_change));
            std::thread::spawn(move || run(&shared, &engine, &source, timing, &*on_change))
        };
        Poller {
            shared,
            on_change,
            thread: Arc::new(Mutex::new(Some(thread))),
        }
    }

    /// The list and how it stands now.
    pub fn view(&self) -> RaidsView {
        self.shared.view().clone()
    }

    /// The key or site changed: drop the list and fetch again now.
    pub fn restart(&self) {
        let view = RaidsView::loading();
        {
            // The view first, then the control, the order a pass publishes in.
            let mut current = self.shared.view();
            self.shared.control().generation += 1;
            *current = view.clone();
        }
        self.shared.wake.notify_all();
        (self.on_change)(&view);
    }

    /// Stops fetching. It doesn't wait for a request in flight, whose answer is dropped.
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

fn run<S: SecretStore>(
    shared: &Shared,
    engine: &Engine<S>,
    source: &impl RaidsSource,
    timing: Timing,
    on_change: &dyn Fn(&RaidsView),
) {
    loop {
        let generation = {
            let control = shared.control();
            if control.stopped {
                return;
            }
            control.generation
        };
        // The new view is only kept if no restart came while the site was asked, so an
        // answer for an old key or site never lands in the new list.
        let publish = |next: RaidsView| {
            let mut view = shared.view();
            if shared.outdated(generation) || *view == next {
                return;
            }
            *view = next.clone();
            drop(view);
            on_change(&next);
        };
        let wait = match engine.upload_target() {
            (_, Err(e)) => {
                publish(RaidsView {
                    state: RaidsState::KeyUnreadable {
                        message: e.to_string(),
                    },
                    raids: Vec::new(),
                });
                timing.slow
            }
            (_, Ok(None)) => {
                publish(RaidsView {
                    state: RaidsState::NoKey,
                    raids: Vec::new(),
                });
                timing.tick
            }
            (site, Ok(Some(key))) => {
                let result = source.raids(site, &key);
                let wait = match &result {
                    Err(RaidsFailure { error, retry_after }) => match error {
                        SiteError::KeyRefused(_) => timing.stopped,
                        SiteError::HelperOff | SiteError::StagingNotOpen => timing.slow,
                        // Back off as the site asked, but never faster than the tick.
                        SiteError::Unreachable { .. } => retry_after
                            .map_or(timing.tick, |s| {
                                Duration::from_secs(s).min(timing.max_retry_after)
                            })
                            .max(timing.tick),
                    },
                    Ok(_) => timing.tick,
                };
                let next = shared.view().clone().after(result.map_err(|f| f.error));
                publish(next);
                wait
            }
        };
        shared.sleep(generation, wait);
    }
}

#[cfg(test)]
mod tests;
