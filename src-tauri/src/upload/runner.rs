//! Runs the upload client in the background for as long as the app runs.
//!
//! Before every pass it reads the current site and that site's stored key together from
//! the key engine, so a key only ever goes to the site it belongs to. Connecting, removing
//! the key or switching site calls `restart`, which wakes it to start again with the new
//! key and site; a pass already under way sends nothing more with the old one. Quit calls
//! `stop`. A record in flight then stays in the list and is sent again next time, and the
//! site answers it as a repeat (ADDON-SPEC 4.2).

use super::{StatusWatch, SyncStatus, Uplink, Uploader, IDLE};
use crate::commands::Engine;
use crate::key::{HelperKey, SecretStore, Site, UploadReply};
use crate::queue::Queue;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// The event that carries each new sync status to the window.
pub const SYNC_STATUS_EVENT: &str = "sync-status";

/// How long to wait before reading the secure store again after it failed: soon enough to
/// recover on its own, rarely enough not to keep asking the officer for access.
const STORE_RETRY: Duration = Duration::from_secs(300);

#[derive(Default)]
struct Control {
    /// Goes up on every restart, so a run can tell its key and site are out of date.
    generation: u64,
    stopped: bool,
    /// Refused records the window has shown, waiting for the thread to clear them.
    shown: Vec<String>,
}

#[derive(Default)]
struct Shared {
    control: Mutex<Control>,
    wake: Condvar,
}

impl Shared {
    fn control(&self) -> MutexGuard<'_, Control> {
        self.control.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn outdated(&self, generation: u64) -> bool {
        let control = self.control();
        control.stopped || control.generation != generation
    }

    fn take_shown(&self) -> Vec<String> {
        std::mem::take(&mut self.control().shown)
    }

    /// Waits for `wait`, or less if the runner is stopped or restarted, or the window has
    /// shown refused records.
    fn sleep(&self, generation: u64, wait: Duration) {
        let deadline = Instant::now() + wait;
        let mut control = self.control();
        while !control.stopped && control.generation == generation && control.shown.is_empty() {
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

/// The running upload client, held as app state.
#[derive(Clone)]
pub struct Runner {
    shared: Arc<Shared>,
    watch: StatusWatch,
    thread: Arc<Mutex<Option<JoinHandle<()>>>>,
}

impl Runner {
    /// Starts sending `queue` with the key and site from `engine`. `on_change` hears every
    /// new status, from the first.
    pub fn start<S, U>(
        engine: Engine<S>,
        queue: Arc<Mutex<Queue>>,
        uplink: U,
        on_change: impl Fn(&SyncStatus) + Send + Sync + 'static,
    ) -> Runner
    where
        S: SecretStore + Send + Sync + 'static,
        U: Uplink + Send + 'static,
    {
        let shared = Arc::new(Shared::default());
        let mut uploader = Uploader::new();
        let watch = uploader.watch();
        watch.on_change(on_change);
        let thread = {
            let shared = Arc::clone(&shared);
            std::thread::spawn(move || run(&shared, &engine, &queue, &uplink, &mut uploader))
        };
        Runner {
            shared,
            watch,
            thread: Arc::new(Mutex::new(Some(thread))),
        }
    }

    pub fn status(&self) -> SyncStatus {
        self.watch.get()
    }

    /// The key or site changed: start again with the new ones now.
    pub fn restart(&self) {
        self.shared.control().generation += 1;
        self.shared.wake.notify_all();
    }

    /// The window has shown these refused records (by id): they stop counting. Records
    /// refused since stay.
    pub fn clear_refused(&self, shown: Vec<String>) {
        self.shared.control().shown.extend(shown);
        self.shared.wake.notify_all();
    }

    /// Stops sending. It doesn't wait for a request in flight, which is resent next time.
    pub fn stop(&self) {
        self.shared.control().stopped = true;
        self.shared.wake.notify_all();
    }

    /// Waits for the runner to end after `stop`.
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
    queue: &Mutex<Queue>,
    uplink: &impl Uplink,
    uploader: &mut Uploader,
) {
    let mut last = None;
    loop {
        let generation = {
            let control = shared.control();
            if control.stopped {
                return;
            }
            control.generation
        };
        let shown = shared.take_shown();
        if !shown.is_empty() {
            uploader.clear_refused(&shown);
        }
        if last.replace(generation).is_some_and(|g| g != generation) {
            uploader.changed();
        }
        let wait = match engine.upload_target() {
            (_, Err(e)) => uploader.halt_for(
                SyncStatus::KeyUnreadable {
                    message: e.to_string(),
                },
                STORE_RETRY,
            ),
            (_, Ok(None)) => uploader.halt_for(SyncStatus::NoKey, IDLE),
            (site, Ok(Some(key))) => {
                let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
                // A file fixed or removed since clears the damage.
                if queue.damage().is_some() {
                    queue.reload();
                }
                let guarded = Guarded {
                    uplink,
                    shared,
                    generation,
                };
                uploader.pass(&mut queue, &guarded, site, &key, Instant::now())
            }
        };
        // Waking for shown refusals clears them and goes on waiting out the rest of `wait`.
        let deadline = Instant::now() + wait;
        loop {
            shared.sleep(
                generation,
                deadline.saturating_duration_since(Instant::now()),
            );
            let shown = shared.take_shown();
            if shown.is_empty() {
                break;
            }
            uploader.clear_refused(&shown);
        }
    }
}

/// Sends only while the key and site it was given are still current and the runner isn't
/// stopped, so a pass under way sends nothing more after a restart or Quit. An answer that
/// comes back after one is dropped, so its records stay in the list and are sent again;
/// the site answers them as repeats.
struct Guarded<'a, U> {
    uplink: &'a U,
    shared: &'a Shared,
    generation: u64,
}

impl<U: Uplink> Uplink for Guarded<'_, U> {
    fn post_operations(
        &self,
        site: Site,
        key: &HelperKey,
        body: &str,
    ) -> Result<UploadReply, String> {
        let outdated = || Err("the key or site changed".to_string());
        if self.shared.outdated(self.generation) {
            return outdated();
        }
        let reply = self.uplink.post_operations(site, key, body);
        if self.shared.outdated(self.generation) {
            return outdated();
        }
        reply
    }
}

#[cfg(test)]
mod tests;
