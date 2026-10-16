//! The waiting list: operations (ADDON-SPEC 4) kept on disk until the site acknowledges
//! them, so an award survives a crash, a restart or an offline spell.
//!
//! The add-on reader and the chat log follower add to it; the upload client reads it and
//! removes what the site acknowledged. It knows nothing of the network or the site:
//! repeats are the site's to settle through each record's key (ADDON-SPEC 4.2).
//!
//! Every change rewrites the whole file through a temporary file that is renamed over
//! it, so a crash leaves either the old list or the new one. A file that can't be read
//! as a list is reported as damaged and never touched again: adds and removes are
//! refused until it reads cleanly (the add-on resends from SavedVariables, ADDON-SPEC 9.5).
//!
//! One `Queue` owns its file; writers in the app share it behind a lock.

use serde_json::{json, Map, Value};
use std::collections::VecDeque;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The file's name in the helper's app data folder.
pub const FILE_NAME: &str = "waiting.json";

const FORMAT_VERSION: i64 = 1;
/// How many of the latest acknowledgements are remembered.
const RECENT: usize = 1000;

/// How the site acknowledged an operation (ADDON-SPEC 7.1). Only these remove an entry;
/// `pending` is not an acknowledgement, so it has no value here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ack {
    Recorded,
    Replayed,
    Voided,
    Rejected { code: String },
}

impl Ack {
    /// Reads one result's `status` and `code`. Anything else (`pending`, an unknown word,
    /// `rejected` without a code) is not an acknowledgement.
    pub fn from_result(status: &str, code: Option<&str>) -> Option<Ack> {
        match (status, code) {
            ("recorded", _) => Some(Ack::Recorded),
            ("replayed", _) => Some(Ack::Replayed),
            ("voided", _) => Some(Ack::Voided),
            ("rejected", Some(code)) if !code.is_empty() => Some(Ack::Rejected {
                code: code.to_string(),
            }),
            _ => None,
        }
    }
}

/// What `add` did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    /// The operation is now on disk, at the end of the list.
    Queued,
    /// The very same operation is already waiting, so nothing changed.
    AlreadyWaiting,
}

#[derive(Debug)]
pub enum QueueError {
    /// The file can't be read as a list. It is left exactly as it is.
    Damaged { problem: String },
    /// The operation has no valid `id` (ADDON-SPEC 4).
    BadOperation { problem: String },
    /// Writing the file failed. The list in memory matches the file: unchanged if the
    /// new file wasn't swapped in, the new list if it was but couldn't be confirmed on disk.
    Io(io::Error),
}

impl fmt::Display for QueueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QueueError::Damaged { problem } => {
                write!(
                    f,
                    "The waiting list is damaged ({problem}). Nothing was deleted."
                )
            }
            QueueError::BadOperation { problem } => {
                write!(
                    f,
                    "An award couldn't be added to the waiting list: {problem}."
                )
            }
            QueueError::Io(e) => write!(f, "The waiting list couldn't be saved: {e}."),
        }
    }
}

impl std::error::Error for QueueError {}

impl From<io::Error> for QueueError {
    fn from(e: io::Error) -> Self {
        QueueError::Io(e)
    }
}

/// Called after each step of a write. Tests use it to fail the write there, or to stop
/// it there as a crash would.
type AtStep = dyn FnMut(Step) -> io::Result<()>;

/// The steps of a write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    TempCreated,
    HalfWritten,
    Written,
    Swapped,
}

pub struct Queue {
    path: PathBuf,
    state: Result<Vec<Value>, String>,
    /// The last new file was swapped in but its rename couldn't be flushed to disk.
    unconfirmed: bool,
    /// The latest removed operations and how the site acknowledged them, oldest first, so
    /// the chat log follower can tell a refusal from a send. Kept in memory only.
    recent: VecDeque<(Value, Ack)>,
    /// How many acknowledgements `recent` keeps.
    keep: usize,
}

impl Queue {
    /// Opens the list at `path`. A missing or empty file is an empty list; a file that
    /// can't be read as one leaves the queue damaged (see `damage`).
    pub fn open(path: impl Into<PathBuf>) -> Queue {
        let path = path.into();
        let state = load(&path);
        Queue {
            path,
            state,
            unconfirmed: false,
            recent: VecDeque::new(),
            keep: RECENT,
        }
    }

    /// Reads the file again, so a fixed or removed file clears the damage.
    pub fn reload(&mut self) {
        self.state = load(&self.path);
    }

    /// Why the file can't be read, if it can't.
    pub fn damage(&self) -> Option<&str> {
        self.state.as_ref().err().map(String::as_str)
    }

    /// The waiting operations, oldest first.
    pub fn operations(&self) -> Result<&[Value], QueueError> {
        match &self.state {
            Ok(ops) => Ok(ops),
            Err(problem) => Err(damaged(problem)),
        }
    }

    /// Adds `op` at the end of the list; it is on disk before this returns. An operation
    /// that reuses a waiting one's `id` with other contents is kept too, so the site can
    /// report the conflict (ADDON-SPEC 4.2).
    pub fn add(&mut self, op: Value) -> Result<Added, QueueError> {
        self.add_with(op, &mut |_| Ok(()))
    }

    /// Removes `op`, exactly as it was read and sent, because the site acknowledged it as
    /// `_ack`; asking for the acknowledgement means nothing else can remove an entry.
    /// Matching the whole operation, not just its `id`, keeps a reused id's other entry.
    /// Returns whether it was waiting.
    pub fn remove_acknowledged(&mut self, op: &Value, ack: &Ack) -> Result<bool, QueueError> {
        let removed = self.remove_with(op, &mut |_| Ok(()))?;
        if removed {
            if self.recent.len() >= self.keep {
                self.recent.pop_front();
            }
            self.recent.push_back((op.clone(), ack.clone()));
        }
        Ok(removed)
    }

    /// Remembers only the latest `n` acknowledgements, so tests can make one be forgotten.
    #[cfg(test)]
    pub(crate) fn keep_only(&mut self, n: usize) {
        self.keep = n;
    }

    /// How the site acknowledged `op`, if it was removed lately.
    pub fn acknowledgement(&self, op: &Value) -> Option<&Ack> {
        self.recent
            .iter()
            .rev()
            .find(|(o, _)| o == op)
            .map(|(_, ack)| ack)
    }

    fn add_with(&mut self, op: Value, at: &mut AtStep) -> Result<Added, QueueError> {
        let ops = self.operations()?;
        op_id(&op).map_err(|problem| QueueError::BadOperation { problem })?;
        if ops.contains(&op) {
            if self.unconfirmed {
                sync_rename(&self.path)?;
                self.unconfirmed = false;
            }
            return Ok(Added::AlreadyWaiting);
        }
        let mut next = ops.to_vec();
        next.push(op);
        self.save(next, at)?;
        Ok(Added::Queued)
    }

    fn remove_with(&mut self, op: &Value, at: &mut AtStep) -> Result<bool, QueueError> {
        let ops = self.operations()?;
        let Some(index) = ops.iter().position(|o| o == op) else {
            return Ok(false);
        };
        let mut next = ops.to_vec();
        next.remove(index);
        self.save(next, at)?;
        Ok(true)
    }

    /// Writes `next` and, once it is in place, keeps it in memory.
    fn save(&mut self, next: Vec<Value>, at: &mut AtStep) -> Result<(), QueueError> {
        let file = json!({ "formatVersion": FORMAT_VERSION, "operations": next });
        let bytes = serde_json::to_vec(&file).expect("JSON values always serialize");
        let result = replace_file(&self.path, &bytes, at);
        match result {
            Ok(()) => {
                self.state = Ok(next);
                self.unconfirmed = false;
                Ok(())
            }
            Err(e) => {
                if e.swapped {
                    self.state = Ok(next);
                    self.unconfirmed = true;
                }
                Err(QueueError::Io(e.error))
            }
        }
    }
}

fn damaged(problem: &str) -> QueueError {
    QueueError::Damaged {
        problem: problem.to_string(),
    }
}

fn load(path: &Path) -> Result<Vec<Value>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("it can't be read: {e}")),
    };
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let file: Value = serde_json::from_slice(&bytes).map_err(|e| format!("it isn't valid: {e}"))?;
    parse_list(&file)
}

fn parse_list(file: &Value) -> Result<Vec<Value>, String> {
    let file = file.as_object().ok_or("it isn't a list")?;
    match file.get("formatVersion").and_then(Value::as_i64) {
        Some(FORMAT_VERSION) => {}
        Some(v) => {
            return Err(format!(
                "it was saved in format {v}, which this helper doesn't know"
            ))
        }
        None => return Err("it has no format version".to_string()),
    }
    let ops = file
        .get("operations")
        .and_then(Value::as_array)
        .ok_or("it has no operations")?;
    for (i, op) in ops.iter().enumerate() {
        op_id(op).map_err(|p| format!("entry {} {p}", i + 1))?;
    }
    Ok(ops.clone())
}

/// The operation's `id`: letters, digits, `_` or `-`, 8 to 128 long (ADDON-SPEC 4).
fn op_id(op: &Value) -> Result<String, String> {
    let op: &Map<String, Value> = op.as_object().ok_or("is not an operation")?;
    let id = op.get("id").and_then(Value::as_str).ok_or("has no id")?;
    let ok = (8..=128).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(id.to_string())
    } else {
        Err("has an id that isn't valid".to_string())
    }
}

/// A failed write, and whether the new file had already replaced the old one.
struct WriteError {
    error: io::Error,
    swapped: bool,
}

/// Writes `bytes` to a new temporary file beside `target`, flushes it to disk and renames
/// it over `target`, so `target` is always either the old file or the new one, then
/// flushes the rename itself. Before the swap, a failure removes the temporary file.
fn replace_file(target: &Path, bytes: &[u8], at: &mut AtStep) -> Result<(), WriteError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let before = |error| WriteError {
        error,
        swapped: false,
    };
    let (temp, mut file) =
        create_temp(target, NEXT.fetch_add(1, Ordering::Relaxed)).map_err(before)?;
    let result = (|| {
        at(Step::TempCreated)?;
        let half = bytes.len() / 2;
        file.write_all(&bytes[..half])?;
        at(Step::HalfWritten)?;
        file.write_all(&bytes[half..])?;
        file.sync_all()?;
        drop(file);
        at(Step::Written)?;
        fs::rename(&temp, target)
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temp);
        return Err(before(error));
    }
    at(Step::Swapped)
        .and_then(|()| sync_rename(target))
        .map_err(|error| WriteError {
            error,
            swapped: true,
        })
}

/// Flushes the rename to disk: on Unix through the folder, which holds the name; on
/// Windows, where std can't open a folder for that, through the renamed file.
fn sync_rename(target: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        let folder = match target.parent() {
            Some(folder) if !folder.as_os_str().is_empty() => folder,
            _ => Path::new("."),
        };
        File::open(folder)?.sync_all()
    }
    #[cfg(not(unix))]
    {
        OpenOptions::new().write(true).open(target)?.sync_all()
    }
}

/// Creates a new temporary file beside `target`, never opening one that already exists
/// (or a link), so no stray file can be written through.
fn create_temp(target: &Path, start: u64) -> io::Result<(PathBuf, File)> {
    let name = target.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the waiting list path has no file name",
        )
    })?;
    for n in start..start + 100 {
        let mut temp_name = name.to_os_string();
        temp_name.push(format!(".{}-{n}.tmp", std::process::id()));
        let temp = target.with_file_name(temp_name);
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => return Ok((temp, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "no free name for a temporary file",
    ))
}

#[cfg(test)]
mod tests;
