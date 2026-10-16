//! Follows the game's chat log (`Logs/WoWChatLog.txt`) as it grows, reads each complete
//! line, and hands the add-on's award, undo and replacement lines to the waiting list.
//!
//! Every other line is dropped as it is read: nothing from it is stored, queued, logged or
//! sent. The saved read position (`chatlog.json` in the helper's app data) holds only the
//! log's path, a byte offset and a hash of the log's first kilobyte, which tells a replaced
//! log from the same one grown.
//!
//! The saved position moves past a line only once the site has acknowledged its operation
//! (the uploader removes it from the waiting list), so after a crash, a restart or an
//! offline spell the follower reads again from the oldest line not yet acknowledged. A line
//! read twice gives the same operation, which the queue holds once and the site answers
//! `replayed` (ADDON-SPEC 4.2).
//!
//! - With no saved position, reading starts at the end of the log: the add-on's own upload
//!   at logout or `/reload` sends anything earlier with the same keys.
//! - A log shorter than the read position, or whose start changed, was truncated or
//!   replaced: reading starts again from its beginning.
//! - A last line without its line end is still being written and waits for the next pass.
//! - A missing log does nothing; the log is read in pieces, never whole.

use super::{parse_line_now, KnownRaid, LineResult, NamedRaid, Operation};
use crate::queue::{Queue, QueueError};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The saved position's file name in the helper's app data folder.
pub const FILE_NAME: &str = "chatlog.json";
/// The event that carries each chat log event to the window.
pub const CHAT_LOG_EVENT: &str = "chat-log";

const FORMAT_VERSION: i64 = 1;
/// The most read at once. A chat line is far shorter, so a piece this size with no line end
/// in it is not chat and is skipped.
const PIECE: usize = 256 * 1024;
/// How much of the log's start is hashed to tell a replaced log.
const HEAD: u64 = 1024;

/// The chat log inside a game version folder.
pub fn log_in(version_folder: &Path) -> PathBuf {
    version_folder.join("Logs").join("WoWChatLog.txt")
}

/// What the window hears about the chat log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatEvent {
    /// The site acknowledged a line's operation.
    Sent(SentLine),
    /// An add-on line that was not used; nothing from it is kept.
    Ignored { reason: Ignored },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Ignored {
    /// The line breaks the format (ADDON-SPEC 9.4).
    Malformed,
    /// The line is in a newer format than this helper reads.
    NewerVersion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LineKind {
    Award,
    Replacement,
    Undo,
}

/// A sent line, as the window shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentLine {
    pub line: LineKind,
    /// The game item id and name from the line's item link.
    pub item: Option<u32>,
    pub item_name: Option<String>,
    /// The winner; none for an undo or a disenchant.
    pub character: Option<String>,
    /// The boss's name from the snapshot; none for an undo.
    pub boss: Option<String>,
    /// The raid's id and name.
    pub raid: String,
    pub raid_name: String,
    /// When the line was posted, Unix seconds.
    pub at: i64,
}

/// The read position as saved.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Saved {
    log: String,
    offset: u64,
    head: Head,
}

/// The length and hash of the log's first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Head {
    len: u64,
    hash: u64,
}

/// A line handed to the queue and not yet past the saved position.
struct Handed {
    start: u64,
    op: Value,
    sent: SentLine,
    acknowledged: bool,
}

pub struct Follower {
    file: PathBuf,
    saved: Option<Saved>,
    /// The log the state below is about.
    log: Option<PathBuf>,
    /// Where the next line starts.
    read_to: u64,
    head: Head,
    handed: VecDeque<Handed>,
}

impl Follower {
    /// Opens the saved position at `file`. A missing or unreadable one is no position.
    pub fn open(file: impl Into<PathBuf>) -> Follower {
        let file = file.into();
        let saved = fs::read(&file).ok().and_then(|bytes| load(&bytes));
        Follower {
            file,
            saved,
            log: None,
            read_to: 0,
            head: Head { len: 0, hash: 0 },
            handed: VecDeque::new(),
        }
    }

    /// One pass: reads the lines `log` gained, hands their operations to `queue`, and
    /// reports what the site acknowledged since. Nothing is read without a log or before
    /// the raids are known, since a line naming a raid not yet known would be lost.
    pub fn pass(
        &mut self,
        log: Option<&Path>,
        raids: Option<&[NamedRaid]>,
        queue: &Mutex<Queue>,
    ) -> Vec<ChatEvent> {
        let (Some(log), Some(raids)) = (log, raids) else {
            return Vec::new();
        };
        if self.log.as_deref() != Some(log) {
            self.start(log);
        }
        let mut events = Vec::new();
        if let Ok(mut file) = File::open(log) {
            let _ = self.read(&mut file, raids, queue, &mut events);
        }
        self.acknowledge(queue, &mut events);
        self.save();
        events
    }

    /// Starts on `log`: from the saved position if it is for this log, else at its end.
    fn start(&mut self, log: &Path) {
        self.log = Some(log.to_path_buf());
        self.handed.clear();
        let name = log.to_string_lossy();
        match self.saved.as_ref().filter(|s| s.log == name) {
            Some(saved) => {
                self.read_to = saved.offset;
                self.head = saved.head;
            }
            None => {
                let mut file = File::open(log).ok();
                let len = file
                    .as_ref()
                    .and_then(|f| f.metadata().ok())
                    .map_or(0, |m| m.len());
                self.read_to = len;
                self.head = file
                    .as_mut()
                    .and_then(|f| head_of(f, len.min(HEAD)).ok())
                    .unwrap_or(Head {
                        len: 0,
                        hash: hash(&[]),
                    });
            }
        }
    }

    fn read(
        &mut self,
        file: &mut File,
        raids: &[NamedRaid],
        queue: &Mutex<Queue>,
        events: &mut Vec<ChatEvent>,
    ) -> io::Result<()> {
        let len = file.metadata()?.len();
        let same = len >= self.read_to
            && len >= self.head.len
            && head_of(file, self.head.len)? == self.head;
        if !same {
            // Truncated or replaced: what was handed on stays in the queue and still goes.
            self.read_to = 0;
            self.handed.clear();
            self.head = head_of(file, 0)?;
        }
        if self.head.len < HEAD && len > self.head.len {
            self.head = head_of(file, len.min(HEAD))?;
        }
        let known: Vec<KnownRaid> = raids.iter().map(|r| r.raid.clone()).collect();
        let mut piece = Vec::with_capacity(PIECE);
        loop {
            file.seek(SeekFrom::Start(self.read_to))?;
            piece.clear();
            file.by_ref().take(PIECE as u64).read_to_end(&mut piece)?;
            let Some(last) = piece.iter().rposition(|&b| b == b'\n') else {
                if piece.len() == PIECE {
                    self.read_to += PIECE as u64;
                    continue;
                }
                return Ok(());
            };
            for line in piece[..=last].split_inclusive(|&b| b == b'\n') {
                let text = String::from_utf8_lossy(line);
                if !self.line(&text, &known, raids, queue, events) {
                    return Ok(());
                }
                self.read_to += line.len() as u64;
            }
        }
    }

    /// Handles one line. Returns false if the queue couldn't take its operation, so the
    /// line is read again next pass.
    fn line(
        &mut self,
        text: &str,
        known: &[KnownRaid],
        raids: &[NamedRaid],
        queue: &Mutex<Queue>,
        events: &mut Vec<ChatEvent>,
    ) -> bool {
        let op = match parse_line_now(text, known) {
            LineResult::NotOurs => return true,
            LineResult::Malformed(_) => {
                events.push(ChatEvent::Ignored {
                    reason: Ignored::Malformed,
                });
                return true;
            }
            LineResult::NeedsUpdate => {
                events.push(ChatEvent::Ignored {
                    reason: Ignored::NewerVersion,
                });
                return true;
            }
            LineResult::Operation(op) => op,
        };
        let value = op.to_json();
        let added = queue
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .add(value.clone());
        match added {
            Ok(_) => {}
            // The parser checks every id, so this can't happen; the line is dropped.
            Err(QueueError::BadOperation { .. }) => return true,
            Err(_) => return false,
        }
        self.handed.push_back(Handed {
            start: self.read_to,
            op: value,
            sent: sent_line(&op, text, raids),
            acknowledged: false,
        });
        true
    }

    /// An operation no longer in the queue was acknowledged by the site.
    fn acknowledge(&mut self, queue: &Mutex<Queue>, events: &mut Vec<ChatEvent>) {
        let queue = queue.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(ops) = queue.operations() else {
            return;
        };
        for handed in self.handed.iter_mut().filter(|h| !h.acknowledged) {
            if !ops.contains(&handed.op) {
                handed.acknowledged = true;
                events.push(ChatEvent::Sent(handed.sent.clone()));
            }
        }
        drop(queue);
        while self.handed.front().is_some_and(|h| h.acknowledged) {
            self.handed.pop_front();
        }
    }

    /// Saves the position: the start of the oldest line not yet acknowledged. A failed
    /// write is tried again next pass.
    fn save(&mut self) {
        let Some(log) = &self.log else {
            return;
        };
        let next = Saved {
            log: log.to_string_lossy().into_owned(),
            offset: self.handed.front().map_or(self.read_to, |h| h.start),
            head: self.head,
        };
        if self.saved.as_ref() == Some(&next) {
            return;
        }
        let bytes = json!({
            "formatVersion": FORMAT_VERSION,
            "log": next.log,
            "offset": next.offset,
            "headLength": next.head.len,
            "headHash": next.head.hash,
        })
        .to_string();
        let temp = self.file.with_extension("json.tmp");
        if fs::write(&temp, bytes)
            .and_then(|()| fs::rename(&temp, &self.file))
            .is_ok()
        {
            self.saved = Some(next);
        }
    }
}

fn load(bytes: &[u8]) -> Option<Saved> {
    let v: Value = serde_json::from_slice(bytes).ok()?;
    if v["formatVersion"].as_i64() != Some(FORMAT_VERSION) {
        return None;
    }
    Some(Saved {
        log: v["log"].as_str()?.to_string(),
        offset: v["offset"].as_u64()?,
        head: Head {
            len: v["headLength"].as_u64()?,
            hash: v["headHash"].as_u64()?,
        },
    })
}

fn head_of(file: &mut File, len: u64) -> io::Result<Head> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    file.by_ref().take(len).read_to_end(&mut bytes)?;
    Ok(Head {
        len: bytes.len() as u64,
        hash: hash(&bytes),
    })
}

/// FNV-1a: stable across versions and machines, unlike std's hasher.
fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

fn sent_line(op: &Operation, text: &str, raids: &[NamedRaid]) -> SentLine {
    let (line, award, raid, env, at) = match op {
        Operation::Award(a) => (LineKind::Award, Some(a), &a.raid, a.env, a.at),
        Operation::Replace(r) => (LineKind::Replacement, Some(&*r.award), &r.raid, r.env, r.at),
        Operation::Void(v) => (LineKind::Undo, None, &v.raid, v.env, v.at),
    };
    let named = raids
        .iter()
        .find(|r| r.raid.id == *raid && r.raid.env == env);
    let (link_item, item_name) = item_link(text);
    SentLine {
        line,
        item: award.map(|a| a.item).or(link_item),
        item_name,
        character: award.and_then(|a| a.char.clone()),
        boss: award.map(|a| {
            named
                .and_then(|r| r.boss_names.get(&a.boss).cloned())
                .unwrap_or_else(|| a.boss.clone())
        }),
        raid: raid.clone(),
        raid_name: named.map(|r| r.name.clone()).unwrap_or_default(),
        at,
    }
}

/// The item id and name of the line's first item link (`|Hitem:16805:…|h[Name]|h`).
fn item_link(text: &str) -> (Option<u32>, Option<String>) {
    let Some(at) = text.find("|Hitem:") else {
        return (None, None);
    };
    let link = &text[at + 7..];
    let id: String = link.chars().take_while(char::is_ascii_digit).collect();
    let name = link
        .split_once("|h[")
        .and_then(|(_, rest)| rest.split_once("]|h"))
        .map(|(name, _)| name.to_string());
    (id.parse().ok(), name)
}

/// How often the follower looks at the log.
pub const POLL: Duration = Duration::from_secs(1);

/// The running follower, held as app state.
pub struct Tail {
    stop: mpsc::Sender<()>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

impl Tail {
    /// Follows the log in whatever `version_folder` returns, with the raids `raids` knows,
    /// every `poll`. `on_event` is called from the follower's thread.
    pub fn start(
        mut follower: Follower,
        version_folder: impl Fn() -> Option<PathBuf> + Send + 'static,
        raids: impl Fn() -> Option<Vec<NamedRaid>> + Send + 'static,
        queue: Arc<Mutex<Queue>>,
        poll: Duration,
        on_event: impl Fn(&ChatEvent) + Send + 'static,
    ) -> Tail {
        let (stop, stop_rx) = mpsc::channel();
        let thread = std::thread::spawn(move || loop {
            let log = version_folder().map(|f| log_in(&f));
            for event in follower.pass(log.as_deref(), raids().as_deref(), &queue) {
                on_event(&event);
            }
            match stop_rx.recv_timeout(poll) {
                Err(RecvTimeoutError::Timeout) => {}
                _ => return,
            }
        });
        Tail {
            stop,
            thread: Mutex::new(Some(thread)),
        }
    }

    /// Stops following and waits for the thread to end.
    pub fn stop(&self) {
        let _ = self.stop.send(());
        let thread = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(thread) = thread {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests;
