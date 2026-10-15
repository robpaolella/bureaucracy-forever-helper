//! Reads the add-on's SavedVariables file, which the game writes at logout or `/reload`
//! (ADDON-SPEC 4.4, 8.3), and hands its operations to the waiting list.
//!
//! The file is read as data by a parser that never runs anything (`lua`); a file with
//! code in it, a syntax error or the wrong shape is refused whole, with a reason in plain
//! words, and nothing from it is handed on. Only `ops[raidId]` goes to the queue, in the
//! order saved. The final-state lists (`awards[raidId]`, `noRaid`) are parsed and checked
//! but not uploaded, and `noRaidOps` are read but not handed on: they carry no `raid` or
//! `env` and are never uploaded until an officer attaches them to a raid (ADDON-SPEC 1),
//! which moves them into `ops[raidId]`.
//!
//! The reader keeps no record of what it handed over. Reading an unchanged file again adds
//! nothing new, because the queue holds an identical operation once and the site settles
//! one it already has (ADDON-SPEC 4.2).
//!
//! `watcher` finds the file for each account, waits until it has stopped changing, reads
//! it and hands the result on from a thread of its own, so a busy queue never makes the
//! reader miss a change.

mod lua;
pub mod watcher;

use crate::queue::{Added, Queue, QueueError};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

/// The file's name in each account's `SavedVariables` folder: the add-on's `.toc` name.
pub const FILE_NAME: &str = "BureaucracyForever.lua";
/// The global the add-on saves (`## SavedVariables:` in its `.toc`).
const GLOBAL_NAME: &str = "BureaucracyForeverDB";
/// Bigger than any real file (all the raids' reserves and a long season of awards);
/// anything over this is refused unread.
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// How the officers' group chose the raid (ADDON-SPEC 2.1), as the add-on last saved it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub active_raid: Option<String>,
    pub active_by: Option<String>,
    pub chosen_by: Option<String>,
}

/// One reading of one account's file, in the form the queue and the status display use.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reading {
    /// The account folder the file came from.
    pub account: String,
    /// When the game last wrote the file, Unix seconds.
    pub saved_at: i64,
    pub selection: Selection,
    /// `ops[raidId]`: each raid's operations in the order saved. These go to the queue.
    pub operations: BTreeMap<String, Vec<Value>>,
    /// `awards[raidId]`: the final-state records. Read, not uploaded.
    pub awards: BTreeMap<String, Vec<Value>>,
    /// `noRaid` and `noRaidOps`: held until attached to a raid. Read, not uploaded.
    pub no_raid: Vec<Value>,
    pub no_raid_ops: Vec<Value>,
}

/// Why a file couldn't be read. Nothing is handed on from it.
#[derive(Debug)]
pub enum ReadError {
    Empty,
    TooBig {
        bytes: u64,
    },
    /// Code, a syntax error or a limit.
    Refused {
        line: usize,
        why: String,
    },
    /// Valid data, but not the add-on's saved-data layout (ADDON-SPEC 4.4).
    Layout {
        problem: String,
    },
    Io(std::io::Error),
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReadError::Empty => write!(f, "The add-on’s saved file is empty."),
            ReadError::TooBig { bytes } => write!(
                f,
                "The add-on’s saved file is too large to read ({} MB; the limit is {} MB).",
                bytes.div_ceil(1_000_000),
                MAX_FILE_BYTES / 1_000_000
            ),
            ReadError::Refused { line, why } => write!(
                f,
                "The add-on’s saved file wasn’t read, because {why} (line {line}). Nothing in it was used."
            ),
            ReadError::Layout { problem } => write!(
                f,
                "The add-on’s saved file wasn’t read, because {problem}. Nothing in it was used."
            ),
            ReadError::Io(e) => write!(f, "The add-on’s saved file couldn’t be opened: {e}."),
        }
    }
}

impl std::error::Error for ReadError {}

impl From<std::io::Error> for ReadError {
    fn from(e: std::io::Error) -> Self {
        ReadError::Io(e)
    }
}

/// Reads the saved text of `account` written at `saved_at`.
pub fn read_text(text: &[u8], account: &str, saved_at: i64) -> Result<Reading, ReadError> {
    if text.iter().all(u8::is_ascii_whitespace) {
        return Err(ReadError::Empty);
    }
    let db = lua::parse_assignment(text, GLOBAL_NAME).map_err(|r| ReadError::Refused {
        line: r.line,
        why: r.why,
    })?;
    // A table with no entries reads as an empty list.
    let db = match db {
        Value::Object(map) => map,
        _ => Default::default(),
    };
    let by_raid = |name: &str| -> Result<BTreeMap<String, Vec<Value>>, ReadError> {
        let mut lists = BTreeMap::new();
        for (raid, list) in object(db.get(name), name)? {
            lists.insert(
                raid.clone(),
                records(Some(&list), &format!("{name} for {raid}"))?,
            );
        }
        Ok(lists)
    };
    Ok(Reading {
        account: account.to_string(),
        saved_at,
        selection: Selection {
            active_raid: text_field(db.get("activeRaid"), "activeRaid")?,
            active_by: text_field(db.get("activeBy"), "activeBy")?,
            chosen_by: text_field(db.get("chosenBy"), "chosenBy")?,
        },
        operations: by_raid("ops")?,
        awards: by_raid("awards")?,
        no_raid: records(db.get("noRaid"), "noRaid")?,
        no_raid_ops: records(db.get("noRaidOps"), "noRaidOps")?,
    })
}

/// Opens and reads the file at `path`, refusing one over the size limit without reading it.
pub fn read_file(path: &Path, account: &str) -> Result<Reading, ReadError> {
    let file = File::open(path)?;
    let meta = file.metadata()?;
    if meta.len() > MAX_FILE_BYTES {
        return Err(ReadError::TooBig { bytes: meta.len() });
    }
    let saved_at = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs() as i64);
    let mut bytes = Vec::new();
    // The file may grow after the check above; never read more than the limit.
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ReadError::TooBig {
            bytes: bytes.len() as u64,
        });
    }
    read_text(&bytes, account, saved_at)
}

fn layout(problem: String) -> ReadError {
    ReadError::Layout { problem }
}

/// A missing value is an empty table; anything but a table is wrong.
fn object(value: Option<&Value>, name: &str) -> Result<serde_json::Map<String, Value>, ReadError> {
    match value {
        None => Ok(Default::default()),
        Some(Value::Object(map)) => Ok(map.clone()),
        Some(Value::Array(a)) if a.is_empty() => Ok(Default::default()),
        Some(_) => Err(layout(format!("“{name}” isn’t a table keyed by raid"))),
    }
}

/// A missing list is empty; a list holds only records (tables).
fn records(value: Option<&Value>, name: &str) -> Result<Vec<Value>, ReadError> {
    match value {
        None => Ok(Vec::new()),
        Some(Value::Array(items)) if items.iter().all(Value::is_object) || items.is_empty() => {
            Ok(items.clone())
        }
        Some(_) => Err(layout(format!("“{name}” isn’t a list of records"))),
    }
}

fn text_field(value: Option<&Value>, name: &str) -> Result<Option<String>, ReadError> {
    match value {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(layout(format!("“{name}” isn’t text"))),
    }
}

/// What handing a reading's operations to the queue did.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandOff {
    pub queued: usize,
    /// Operations the queue already held, so nothing changed.
    pub already_waiting: usize,
    /// Operations the queue refused (no valid `id`), skipped; the rest went on.
    pub refused: usize,
    /// Set when the queue itself failed (a damaged or unwritable list). The operations
    /// after the failing one were not handed on, so the saved order is kept.
    pub queue_problem: Option<String>,
}

/// Adds every operation of `reading` to `queue`, raid by raid, each raid in its saved order.
/// It waits for the queue's lock, which is held while an upload is in progress, so the
/// watcher calls it from a thread of its own.
pub fn hand_off(queue: &Mutex<Queue>, reading: &Reading) -> HandOff {
    let mut result = HandOff::default();
    let mut queue = queue.lock().unwrap_or_else(|e| e.into_inner());
    for op in reading.operations.values().flatten() {
        match queue.add(op.clone()) {
            Ok(Added::Queued) => result.queued += 1,
            Ok(Added::AlreadyWaiting) => result.already_waiting += 1,
            Err(QueueError::BadOperation { .. }) => result.refused += 1,
            Err(e) => {
                result.queue_problem = Some(e.to_string());
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests;
