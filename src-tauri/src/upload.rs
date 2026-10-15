//! The upload client: sends the waiting list's operations to the site's
//! `POST /api/helper/operations` (ADDON-SPEC 7.1) in order and in batches, removes each
//! only once the site acknowledges it, and keeps a plain sync status for the window.
//!
//! It runs as passes: the app calls `pass` with the current site and key, and the pass
//! sends what it can and says how long to wait before the next one. A crash between an
//! upload and its acknowledgement only means the next pass sends the same operations
//! again, which the site answers `replayed` (ADDON-SPEC 4.2).
//!
//! An operation is only sent to the site the helper is set to, with that site's key: a
//! site refuses another site's records (ADDON-SPEC 2) and a key works only on the site
//! that made it (7.2). One whose `env` names the other site, and any practice record
//! while on production, waits in the list untouched, neither sent nor refused, and the
//! status counts it.

use crate::key::{HelperKey, Site, SiteClient, UploadReply};
use crate::queue::{Ack, Queue, QueueError};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Operations and bytes per request, and requests per minute (ADDON-SPEC 7.6).
const MAX_OPERATIONS: usize = 100;
const MAX_BODY: usize = 64 * 1024;
const MAX_REQUESTS: usize = 30;
const RATE_WINDOW: Duration = Duration::from_secs(60);

/// How long to wait between passes with nothing to send, and after a refused key, a
/// damaged list or a switched-off site.
const IDLE: Duration = Duration::from_secs(30);
const SITE_OFF: Duration = Duration::from_secs(300);
/// Waits that double after each failure in a row: no answer or a server error, and an
/// operation the site keeps `pending`.
const RETRY: (Duration, Duration) = (Duration::from_secs(5), Duration::from_secs(300));
const PENDING: (Duration, Duration) = (Duration::from_secs(30), Duration::from_secs(1800));
/// The wait on `413` or `429` without a usable `Retry-After`, and the longest one taken.
const BACK_OFF: Duration = Duration::from_secs(60);
const MAX_RETRY_AFTER: u64 = 3600;

/// Where operations go: the site client, or a fake one in tests.
pub trait Uplink {
    fn post_operations(
        &self,
        site: Site,
        key: &HelperKey,
        body: &str,
    ) -> Result<UploadReply, String>;
}

impl Uplink for SiteClient {
    fn post_operations(
        &self,
        site: Site,
        key: &HelperKey,
        body: &str,
    ) -> Result<UploadReply, String> {
        SiteClient::post_operations(self, site, key, body)
    }
}

/// A record kept out of the upload, for the window to name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Refused {
    pub id: String,
    /// The game item id and character, when the record is an award or a replacement.
    pub item: Option<i64>,
    pub character: Option<String>,
    /// The site's `rejected` code, or `too-large` for a record too big to send at all.
    pub code: String,
}

/// What the window and tray show about the upload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SyncStatus {
    /// Nothing waiting that can be sent now; `other_site` records wait for the other site.
    #[serde(rename_all = "camelCase")]
    Idle { other_site: usize },
    /// Sending the operations of this raid.
    Sending { raid: String },
    /// The site can't be reached, or failed on its side; this many wait for it.
    Offline { waiting: usize },
    /// Records the site refused, or too big to send; the rest of the list still goes.
    /// `other_site` records wait for the other site.
    #[serde(rename_all = "camelCase")]
    Refused {
        records: Vec<Refused>,
        other_site: usize,
    },
    /// The site refused the key (`401` or `403`). Nothing is sent until the key or the
    /// site changes.
    KeyStopped,
    /// The site has the helper switched off (`503` `helper-off`).
    SiteOff,
    /// The waiting list can't be read or saved; nothing is sent and it is left as it is.
    QueueDamaged { problem: String },
}

/// Reads the uploader's status from anywhere, also while a pass is sending.
#[derive(Clone, Default)]
pub struct StatusWatch(Arc<Mutex<Option<SyncStatus>>>);

impl StatusWatch {
    pub fn get(&self) -> SyncStatus {
        let status = self.0.lock().unwrap_or_else(|e| e.into_inner());
        status.clone().unwrap_or(SyncStatus::Idle { other_site: 0 })
    }

    fn set(&self, status: SyncStatus) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(status);
    }
}

#[derive(Default)]
pub struct Uploader {
    status: StatusWatch,
    /// Records refused since the window last cleared them.
    refused: Vec<Refused>,
    /// Records too big to send, by id; they stay in the list.
    too_large: HashSet<String>,
    /// Operations the site answered `pending`, by id: failures in a row and when to retry.
    pending: HashMap<String, (u32, Instant)>,
    /// No request before this.
    not_before: Option<Instant>,
    /// Failures in a row without an answer.
    failures: u32,
    /// The site and key the site refused, so a new key or site starts sending again.
    stopped: Option<(Site, HelperKey)>,
    /// The largest batch the site took; halved on `413`.
    batch_limit: Option<usize>,
    /// When the requests of the last minute were sent.
    sent: VecDeque<Instant>,
    /// Records in the list waiting for the other site, as of the last look.
    other_site: usize,
}

impl Uploader {
    pub fn new() -> Self {
        Uploader::default()
    }

    /// The status now.
    pub fn status(&self) -> SyncStatus {
        self.status.get()
    }

    /// A reader of the status for the window and tray.
    pub fn watch(&self) -> StatusWatch {
        self.status.clone()
    }

    /// Forgets the refused records once the window has shown them.
    pub fn clear_refused(&mut self) {
        self.refused.clear();
        if matches!(self.status(), SyncStatus::Refused { .. }) {
            self.status.set(SyncStatus::Idle {
                other_site: self.other_site,
            });
        }
    }

    /// Sends what it can of `queue` to `site` with `key`, as of `now`, and returns how long
    /// to wait before the next pass.
    pub fn pass(
        &mut self,
        queue: &mut Queue,
        uplink: &impl Uplink,
        site: Site,
        key: &HelperKey,
        now: Instant,
    ) -> Duration {
        if let Some(problem) = queue.damage() {
            return self.halt(SyncStatus::QueueDamaged {
                problem: problem.to_string(),
            });
        }
        if self.stopped.as_ref() == Some(&(site, key.clone())) {
            return self.halt(SyncStatus::KeyStopped);
        }
        if self.stopped.take().is_some() {
            self.not_before = None;
        }
        if let Some(at) = self.not_before.filter(|at| *at > now) {
            return at - now;
        }
        let started = Instant::now();
        loop {
            // Requests take time, so each is counted from when it actually goes.
            let at = now + started.elapsed();
            while self.sent.front().is_some_and(|t| at - *t >= RATE_WINDOW) {
                self.sent.pop_front();
            }
            let batch = match queue.operations() {
                Ok(ops) => self.batch(ops, site, now),
                Err(e) => return self.halt(damaged(e)),
            };
            if batch.is_empty() {
                self.status.set(self.settled());
                return self.next_pending(now).unwrap_or(IDLE);
            }
            if self.sent.len() >= MAX_REQUESTS {
                return self.sent[0] + RATE_WINDOW - at;
            }
            self.status.set(SyncStatus::Sending {
                raid: batch[0]["raid"].as_str().unwrap_or_default().to_string(),
            });
            let body = json!({ "operations": batch }).to_string();
            self.sent.push_back(at);
            let reply = uplink.post_operations(site, key, &body);
            if let Some(wait) = self.answer(queue, &batch, reply, site, key, now) {
                return wait;
            }
        }
    }

    /// The next operations to send, oldest first, within the request limits. An operation
    /// alone over the size limit is refused here and left in the list.
    fn batch(&mut self, ops: &[Value], site: Site, now: Instant) -> Vec<Value> {
        let limit = self.batch_limit.unwrap_or(MAX_OPERATIONS);
        let mut batch = Vec::new();
        let mut size = r#"{"operations":[]}"#.len();
        self.other_site = ops.iter().filter(|op| !belongs_to(op, site)).count();
        for op in ops {
            let id = op["id"].as_str().unwrap_or_default();
            let waiting = self.pending.get(id).is_some_and(|(_, at)| *at > now);
            if waiting || self.too_large.contains(id) || !belongs_to(op, site) {
                continue;
            }
            let op_size = op.to_string().len() + 1;
            if size + op_size > MAX_BODY {
                if batch.is_empty() {
                    self.refuse(op, "too-large");
                    continue;
                }
                break;
            }
            size += op_size;
            batch.push(op.clone());
            if batch.len() == limit {
                break;
            }
        }
        batch
    }

    /// Acts on the site's answer to `batch`. Returns the wait before the next pass when this
    /// pass should stop, or `None` to send the next batch.
    fn answer(
        &mut self,
        queue: &mut Queue,
        batch: &[Value],
        reply: Result<UploadReply, String>,
        site: Site,
        key: &HelperKey,
        now: Instant,
    ) -> Option<Duration> {
        let reply = match reply {
            Ok(reply) => reply,
            Err(_) => return Some(self.retry(queue, now)),
        };
        let code = reply.body.as_ref().and_then(|b| b["code"].as_str());
        match (reply.status, code) {
            (200, _) => {}
            (401, _) | (403, _) => {
                self.stopped = Some((site, key.clone()));
                return Some(self.halt(SyncStatus::KeyStopped));
            }
            (503, Some("helper-off")) => {
                self.not_before = Some(now + SITE_OFF);
                self.status.set(SyncStatus::SiteOff);
                return Some(SITE_OFF);
            }
            (413, _) | (429, _) => {
                if reply.status == 413 {
                    self.batch_limit = Some((batch.len() / 2).max(1));
                    if batch.len() == 1 {
                        self.refuse(&batch[0], "too-large");
                    }
                }
                let wait = reply
                    .retry_after
                    .map(|s| Duration::from_secs(s.clamp(1, MAX_RETRY_AFTER)))
                    .unwrap_or(BACK_OFF);
                self.not_before = Some(now + wait);
                return Some(wait);
            }
            // A server error, or any other answer: the same operations go again later.
            _ => return Some(self.retry(queue, now)),
        }
        let results = reply.body.as_ref().and_then(|b| b["results"].as_array());
        let Some(results) = results.filter(|r| r.len() == batch.len()) else {
            return Some(self.retry(queue, now));
        };
        self.failures = 0;
        for (op, result) in batch.iter().zip(results) {
            let id = op["id"].as_str().unwrap_or_default();
            let status = result["status"].as_str().unwrap_or_default();
            let ack = Ack::from_result(status, result["code"].as_str())
                .filter(|_| result["id"].as_str() == Some(id));
            let Some(ack) = ack else {
                let tries = self.pending.get(id).map_or(0, |(n, _)| n + 1);
                self.pending
                    .insert(id.to_string(), (tries, now + doubled(PENDING, tries)));
                continue;
            };
            if let Err(e) = queue.remove_acknowledged(op, &ack) {
                // Already recorded on the site, so sending it again is harmless.
                self.not_before = Some(now + RETRY.0);
                return Some(self.halt(damaged(e)));
            }
            self.pending.remove(id);
            if let Ack::Rejected { code } = ack {
                self.refuse(op, &code);
            }
        }
        None
    }

    /// No usable answer: the same operations go again after a wait that grows.
    fn retry(&mut self, queue: &Queue, now: Instant) -> Duration {
        let wait = doubled(RETRY, self.failures);
        self.failures += 1;
        self.not_before = Some(now + wait);
        let waiting = queue
            .operations()
            .map_or(0, |ops| ops.len().saturating_sub(self.other_site));
        self.status.set(SyncStatus::Offline { waiting });
        wait
    }

    fn halt(&mut self, status: SyncStatus) -> Duration {
        self.status.set(status);
        IDLE
    }

    fn refuse(&mut self, op: &Value, code: &str) {
        let id = op["id"].as_str().unwrap_or_default().to_string();
        if code == "too-large" {
            self.too_large.insert(id.clone());
        }
        let award = if op["type"] == "replace" {
            &op["award"]
        } else {
            op
        };
        self.refused.push(Refused {
            id,
            item: award["item"].as_i64(),
            character: award["char"].as_str().map(str::to_string),
            code: code.to_string(),
        });
    }

    /// The status once nothing more can be sent now.
    fn settled(&self) -> SyncStatus {
        let other_site = self.other_site;
        if self.refused.is_empty() {
            SyncStatus::Idle { other_site }
        } else {
            SyncStatus::Refused {
                records: self.refused.clone(),
                other_site,
            }
        }
    }

    fn next_pending(&self, now: Instant) -> Option<Duration> {
        let at = self.pending.values().map(|(_, at)| *at).min()?;
        Some(at.saturating_duration_since(now).min(IDLE))
    }
}

/// Whether `op` may go to `site`: not when its `env` names the other site, and a practice
/// record, or a replacement carrying one, never to production.
fn belongs_to(op: &Value, site: Site) -> bool {
    let other = match site {
        Site::Production => "staging",
        Site::Staging => "production",
    };
    let practice = [op, &op["award"]]
        .iter()
        .any(|o| o["practice"] == Value::Bool(true));
    op["env"] != other && !(practice && site == Site::Production)
}

fn doubled((first, most): (Duration, Duration), times: u32) -> Duration {
    first.saturating_mul(1 << times.min(16)).min(most)
}

fn damaged(e: QueueError) -> SyncStatus {
    let problem = match e {
        QueueError::Damaged { problem } => problem,
        other => other.to_string(),
    };
    SyncStatus::QueueDamaged { problem }
}

#[cfg(test)]
mod tests;
