//! The helper's one request budget for a site's key (ADDON-SPEC 7.6: 30 requests a minute
//! per key). The raids list, the reserves and the uploader each pace themselves, but they
//! share the key, so every request the real site client sends first asks this for its turn:
//! when the last minute is full it waits until the oldest request is a minute old.

use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Two under the site's 30, for a clock that disagrees with the site's by a moment.
pub const MAX_PER_MINUTE: usize = 28;
const WINDOW: Duration = Duration::from_secs(60);

/// Sliding one-minute window of when requests were (or will be) sent, by site address.
pub struct Throttle {
    max: usize,
    window: Duration,
    sent: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl Throttle {
    pub fn new(max: usize, window: Duration) -> Self {
        Throttle {
            max,
            window,
            sent: Mutex::default(),
        }
    }

    /// Takes the next free turn for `site` as seen at `now` and says how long to wait for it.
    /// Turns are handed out in order, so callers that wait are served first come, first
    /// served.
    pub fn reserve(&self, site: &str, now: Instant) -> Duration {
        let mut sent = self.sent.lock().unwrap_or_else(|e| e.into_inner());
        let turns = sent.entry(site.to_string()).or_default();
        while turns.front().is_some_and(|t| now >= *t + self.window) {
            turns.pop_front();
        }
        // The newest turn can lie ahead of `now`, for a caller still waiting.
        let at = match turns.len() >= self.max {
            true => (turns[turns.len() - self.max] + self.window).max(now),
            false => now,
        };
        turns.push_back(at);
        at.saturating_duration_since(now)
    }

    /// Waits for a turn to send to `site`.
    pub fn wait(&self, site: &str) {
        let wait = self.reserve(site, Instant::now());
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }
}

/// The budget every real site client shares.
pub static SHARED: LazyLock<Throttle> = LazyLock::new(|| Throttle::new(MAX_PER_MINUTE, WINDOW));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_requests_go_at_once_and_the_one_over_waits_for_the_oldest_to_age() {
        let throttle = Throttle::new(3, Duration::from_secs(60));
        let start = Instant::now();
        for i in 0..3 {
            let now = start + Duration::from_secs(i);
            assert_eq!(throttle.reserve("a", now), Duration::ZERO);
        }
        // The fourth, at 3 s, waits until the first (at 0 s) is a minute old.
        let wait = throttle.reserve("a", start + Duration::from_secs(3));
        assert_eq!(wait, Duration::from_secs(57));
        // Another site has its own budget.
        assert_eq!(throttle.reserve("b", start), Duration::ZERO);
    }

    #[test]
    fn no_minute_ever_holds_more_than_the_limit_however_many_ask() {
        // Everything at once, as an upload backlog with a poller beside it would ask.
        let throttle = Throttle::new(MAX_PER_MINUTE, WINDOW);
        let start = Instant::now();
        let mut sent: Vec<Duration> = (0..500).map(|_| throttle.reserve("site", start)).collect();
        sent.sort();
        for (i, first) in sent.iter().enumerate() {
            let in_minute = sent[i..]
                .iter()
                .take_while(|t| **t < *first + WINDOW)
                .count();
            assert!(in_minute <= MAX_PER_MINUTE, "{in_minute} in one minute");
        }
    }
}
