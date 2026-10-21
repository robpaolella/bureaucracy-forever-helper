//! Follows the game's chat log (`Logs/WoWChatLog.txt`) as it grows, reads each complete
//! line, and hands the add-on's award, undo and replacement lines to the waiting list.
//!
//! Every other line is dropped as it is read: nothing from it is stored, queued, logged or
//! sent. The saved read position (`chatlog.json` in the helper's app data) holds only the
//! log's path, a byte offset, and hashes of the log's first kilobyte and of the kilobyte
//! before the offset, which tell a replaced log from the same one grown.
//!
//! The saved position moves past a line only once the site has acknowledged its operation
//! (the uploader removes it from the waiting list); a line the site refused moves it too,
//! but is not reported as sent. So after a crash, a restart or an
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
use crate::queue::{Ack, Queue, QueueError};
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
/// How much of the log's start, and of what comes before the read position, is hashed to
/// tell a replaced log.
const HEAD: u64 = 1024;

/// The chat log inside a game version folder.
pub fn log_in(version_folder: &Path) -> PathBuf {
    version_folder.join("Logs").join("WoWChatLog.txt")
}

/// What the window hears about the chat log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ChatEvent {
    /// The site acknowledged a line's operation, other than by refusing it.
    Sent(SentLine),
    /// A handed line's item name, reported as soon as the line is handed on, so the window
    /// can name the item if the site refuses the award. It is not a sent award.
    #[serde(rename_all = "camelCase")]
    ItemName { item: u32, item_name: String },
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
    tail: Head,
}

/// The length and hash of some of the log's bytes: its first ones, or those just before a
/// position.
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
    /// The bytes just before `read_to`.
    tail: Head,
    handed: VecDeque<Handed>,
    /// Lines handed on from a log since replaced: still reported once acknowledged, but no
    /// longer holding the position back.
    carried: Vec<Handed>,
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
            head: Head::EMPTY,
            tail: Head::EMPTY,
            handed: VecDeque::new(),
            carried: Vec::new(),
        }
    }

    /// One pass: reads the lines `log` gained, hands their operations to `queue`, and
    /// reports what the site acknowledged since. Nothing is read without a log or before
    /// the raids are known, since a line naming a raid not yet known would be lost.
    ///
    /// An item's name goes to `early` before its operation enters the queue, so the window
    /// has it before the site can refuse the award; every other event is returned.
    pub fn pass(
        &mut self,
        log: Option<&Path>,
        raids: Option<&[NamedRaid]>,
        queue: &Mutex<Queue>,
        early: &mut dyn FnMut(ChatEvent),
    ) -> Vec<ChatEvent> {
        let (Some(log), Some(raids)) = (log, raids) else {
            return Vec::new();
        };
        if self.log.as_deref() != Some(log) {
            self.start(log);
        }
        let mut events = Vec::new();
        let mut file = File::open(log).ok();
        if let Some(file) = file.as_mut() {
            let _ = self.read(file, raids, queue, &mut events, early);
            if let Ok(tail) = tail_of(file, self.read_to) {
                self.tail = tail;
            }
        }
        self.acknowledge(queue, &mut events);
        self.save(file.as_mut());
        events
    }

    /// Stops holding the position for lines of a log that is gone, still reporting them.
    fn carry(&mut self) {
        self.carried.extend(self.handed.drain(..));
    }

    /// Starts on `log`: from the saved position if it is for this log, else at its end.
    fn start(&mut self, log: &Path) {
        self.log = Some(log.to_path_buf());
        self.carry();
        let name = log.to_string_lossy();
        match self.saved.as_ref().filter(|s| s.log == name) {
            Some(saved) => {
                self.read_to = saved.offset;
                self.head = saved.head;
                self.tail = saved.tail;
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
                    .unwrap_or(Head::EMPTY);
                self.tail = file
                    .as_mut()
                    .and_then(|f| tail_of(f, len).ok())
                    .unwrap_or(Head::EMPTY);
            }
        }
    }

    fn read(
        &mut self,
        file: &mut File,
        raids: &[NamedRaid],
        queue: &Mutex<Queue>,
        events: &mut Vec<ChatEvent>,
        early: &mut dyn FnMut(ChatEvent),
    ) -> io::Result<()> {
        let len = file.metadata()?.len();
        let same = len >= self.read_to
            && len >= self.head.len
            && head_of(file, self.head.len)? == self.head
            && tail_of(file, self.read_to)? == self.tail;
        if !same {
            // Truncated or replaced: what was handed on stays in the queue and still goes.
            self.read_to = 0;
            self.carry();
            self.head = Head::EMPTY;
            self.tail = Head::EMPTY;
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
                if !self.line(&text, &known, raids, queue, events, early) {
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
        early: &mut dyn FnMut(ChatEvent),
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
        let sent = sent_line(&op, text, raids);
        if let (Some(item), Some(item_name)) = (sent.item, &sent.item_name) {
            early(ChatEvent::ItemName {
                item,
                item_name: item_name.clone(),
            });
        }
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
            sent,
            acknowledged: false,
        });
        true
    }

    /// An operation no longer in the queue was acknowledged by the site; one it refused is
    /// shown by the upload's status instead, and so is one whose outcome the queue has
    /// forgotten.
    fn acknowledge(&mut self, queue: &Mutex<Queue>, events: &mut Vec<ChatEvent>) {
        let queue = queue.lock().unwrap_or_else(|e| e.into_inner());
        let Ok(ops) = queue.operations() else {
            return;
        };
        let waiting = self.carried.iter_mut().chain(self.handed.iter_mut());
        for handed in waiting.filter(|h| !h.acknowledged) {
            if !ops.contains(&handed.op) {
                handed.acknowledged = true;
                // An outcome no longer remembered may have been a refusal, so it isn't
                // reported as sent.
                if matches!(
                    queue.acknowledgement(&handed.op),
                    Some(Ack::Recorded | Ack::Replayed | Ack::Voided)
                ) {
                    events.push(ChatEvent::Sent(handed.sent.clone()));
                }
            }
        }
        drop(queue);
        self.carried.retain(|h| !h.acknowledged);
        while self.handed.front().is_some_and(|h| h.acknowledged) {
            self.handed.pop_front();
        }
    }

    /// Saves the position: the start of the oldest line not yet acknowledged. A failed
    /// write is tried again next pass, as is one that waits for a missing log.
    fn save(&mut self, file: Option<&mut File>) {
        let Some(log) = &self.log else {
            return;
        };
        let offset = self.handed.front().map_or(self.read_to, |h| h.start);
        let tail = match file {
            _ if offset == self.read_to => self.tail,
            Some(file) => match tail_of(file, offset) {
                Ok(tail) => tail,
                Err(_) => return,
            },
            None => return,
        };
        let next = Saved {
            log: log.to_string_lossy().into_owned(),
            offset,
            head: self.head,
            tail,
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
            "tailLength": next.tail.len,
            "tailHash": next.tail.hash,
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
        tail: Head {
            len: v["tailLength"].as_u64()?,
            hash: v["tailHash"].as_u64()?,
        },
    })
}

impl Head {
    /// No bytes.
    const EMPTY: Head = Head {
        len: 0,
        hash: 0xcbf2_9ce4_8422_2325,
    };
}

/// The log's first `len` bytes.
fn head_of(file: &mut File, len: u64) -> io::Result<Head> {
    bytes_at(file, 0, len)
}

/// The kilobyte, or less, before `end`.
fn tail_of(file: &mut File, end: u64) -> io::Result<Head> {
    let len = end.min(HEAD);
    bytes_at(file, end - len, len)
}

fn bytes_at(file: &mut File, start: u64, len: u64) -> io::Result<Head> {
    file.seek(SeekFrom::Start(start))?;
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
            let events = follower.pass(log.as_deref(), raids().as_deref(), &queue, &mut |e| {
                on_event(&e)
            });
            for event in events {
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
