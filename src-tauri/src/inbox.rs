//! Writes the add-on's inbox file (ADDON-SPEC 7.3): each listed raid's snapshot and
//! catch-up as a Lua data file the add-on loads on `/reload`.
//!
//! The game runs the file as code, so the writer emits data only: one assignment and
//! one table literal, keys from a fixed list or integers, every string fully escaped.
//! Before replacing the old file it reads its own output back with a parser for the
//! grammar of ADDON-SPEC 8.2 (never a Lua interpreter) and checks it matches.

use serde_json::Value;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const GLOBAL_NAME: &str = "BureaucracyLootInbox";
const FORMAT_VERSION: i64 = 1;
const MAX_INTEGER: i64 = 9_007_199_254_740_991;
const MAX_DEPTH: usize = 8;
const MAX_STRING_BYTES: usize = 200;
const MAX_RAID_BYTES: usize = 200_000;
const MAX_FILE_BYTES: usize = 700_000;

/// The only identifiers a key may be (ADDON-SPEC 3.1, 3.4 and 7.3). Site text never
/// becomes a key; an object key outside this list must be a whole number.
const KEYS: &[&str] = &[
    "addon",
    "at",
    "awards",
    "blocked",
    "boss",
    "bosses",
    "catchup",
    "char",
    "code",
    "durationMin",
    "env",
    "formatVersion",
    "generated",
    "id",
    "instance",
    "item",
    "items",
    "key",
    "kind",
    "method",
    "name",
    "practice",
    "raid",
    "raids",
    "reason",
    "reserves",
    "roll",
    "snapshot",
    "startsAt",
    "state",
    "trash",
    "trial",
    "url",
    "user",
    "version",
    "voided",
    "voids",
    "written",
];

/// One raid as fetched from the helper API: its snapshot and, once fetched, its catch-up.
pub struct RaidData {
    pub snapshot: Value,
    pub catchup: Option<Value>,
}

/// Why the inbox was not written. The old file is always left as it was.
#[derive(Debug)]
pub enum InboxError {
    /// The site's data can't be written safely as it is.
    BadData { raid: String, problem: String },
    /// One raid's entry is over the size limit.
    RaidTooBig { raid: String, bytes: usize },
    /// The whole file is over the size limit.
    FileTooBig { bytes: usize },
    /// What was produced did not read back as the same data.
    ReadBackMismatch,
    /// Writing or replacing the file failed.
    Io(io::Error),
}

impl fmt::Display for InboxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InboxError::BadData { raid, problem } => {
                write!(f, "The reserves for {raid} couldn't be saved: {problem}.")
            }
            InboxError::RaidTooBig { raid, bytes } => write!(
                f,
                "The reserves for {raid} are too large to save ({} bytes; the limit is {}).",
                group(*bytes),
                group(MAX_RAID_BYTES)
            ),
            InboxError::FileTooBig { bytes } => write!(
                f,
                "The reserves are too large to save ({} bytes in all; the limit is {}).",
                group(*bytes),
                group(MAX_FILE_BYTES)
            ),
            InboxError::ReadBackMismatch => {
                write!(f, "The reserves couldn't be saved: a safety check failed.")
            }
            InboxError::Io(e) => write!(f, "The reserves couldn't be saved to the add-on: {e}."),
        }
    }
}

impl std::error::Error for InboxError {}

fn group(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Name(&'static str),
    Int(i64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Lua {
    Table(Vec<(Option<Key>, Lua)>),
    Str(String),
    Int(i64),
    Bool(bool),
}

/// Writes the inbox for `raids`, in the given order, to `target`, replacing any old file
/// whole. `written` is the Unix time stored in the file.
pub fn write_inbox(target: &Path, raids: &[RaidData], written: i64) -> Result<(), InboxError> {
    let bytes = render_inbox(raids, written)?;
    replace_file(
        target,
        |file| file.write_all(&bytes),
        |from, to| fs::rename(from, to),
    )
    .map_err(InboxError::Io)
}

/// The exact bytes `write_inbox` would write: the same input gives the same bytes.
pub fn render_inbox(raids: &[RaidData], written: i64) -> Result<Vec<u8>, InboxError> {
    let mut entries = Vec::with_capacity(raids.len());
    for (i, raid) in raids.iter().enumerate() {
        let label = raid_label(raid, i);
        let bad = |problem: String| InboxError::BadData {
            raid: label.clone(),
            problem,
        };
        // A raid entry sits at depth 3 (inbox, raids, entry).
        let mut fields = vec![(
            Some(Key::Name("snapshot")),
            from_json(&raid.snapshot, 4).map_err(bad)?,
        )];
        if let Some(catchup) = &raid.catchup {
            fields.push((
                Some(Key::Name("catchup")),
                from_json(catchup, 4).map_err(bad)?,
            ));
        }
        let entry = Lua::Table(fields);
        let mut text = Vec::new();
        write_value(&entry, &mut text);
        if text.len() > MAX_RAID_BYTES {
            return Err(InboxError::RaidTooBig {
                raid: label,
                bytes: text.len(),
            });
        }
        entries.push((None, entry));
    }
    let inbox = check_int(written)
        .map(|written| {
            Lua::Table(vec![
                (Some(Key::Name("formatVersion")), Lua::Int(FORMAT_VERSION)),
                (Some(Key::Name("written")), Lua::Int(written)),
                (Some(Key::Name("raids")), Lua::Table(entries)),
            ])
        })
        .map_err(|problem| InboxError::BadData {
            raid: "the inbox".into(),
            problem,
        })?;

    let mut out = format!("{GLOBAL_NAME} = ").into_bytes();
    write_value(&inbox, &mut out);
    out.push(b'\n');
    if out.len() > MAX_FILE_BYTES {
        return Err(InboxError::FileTooBig { bytes: out.len() });
    }
    if parse_inbox(&out).as_ref() != Ok(&inbox) {
        return Err(InboxError::ReadBackMismatch);
    }
    Ok(out)
}

/// Names a raid for an error: its name and code from the snapshot, or its place in the list.
fn raid_label(raid: &RaidData, index: usize) -> String {
    let field = |k: &str| raid.snapshot.get(k).and_then(Value::as_str);
    match (field("name"), field("code")) {
        (Some(name), Some(code)) => format!("{name} ({code})"),
        (Some(name), None) => name.to_string(),
        (None, Some(code)) => code.to_string(),
        (None, None) => format!("raid {} in the list", index + 1),
    }
}

/// Converts site JSON into the data tree, refusing anything ADDON-SPEC 7.3 doesn't allow.
/// `depth` is the depth of this value if it is a table.
fn from_json(value: &Value, depth: usize) -> Result<Lua, String> {
    let table_depth = || {
        if depth > MAX_DEPTH {
            Err(format!("data is nested more than {MAX_DEPTH} levels deep"))
        } else {
            Ok(())
        }
    };
    match value {
        Value::Null => Err("it contains an empty value (null)".into()),
        Value::Bool(b) => Ok(Lua::Bool(*b)),
        Value::Number(n) => match n.as_i64() {
            Some(i) => check_int(i).map(Lua::Int),
            None => Err(format!("{n} is not a whole number in range")),
        },
        Value::String(s) => check_str(s).map(|_| Lua::Str(s.clone())),
        Value::Array(items) => {
            table_depth()?;
            items
                .iter()
                .map(|item| Ok((None, from_json(item, depth + 1)?)))
                .collect::<Result<_, String>>()
                .map(Lua::Table)
        }
        Value::Object(map) => {
            table_depth()?;
            let mut fields = map
                .iter()
                .map(|(k, v)| Ok((Some(key_from_json(k)?), from_json(v, depth + 1)?)))
                .collect::<Result<Vec<_>, String>>()?;
            // Sorted so the same data always gives the same bytes.
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            Ok(Lua::Table(fields))
        }
    }
}

fn key_from_json(key: &str) -> Result<Key, String> {
    if let Some(name) = KEYS.iter().find(|k| **k == key) {
        return Ok(Key::Name(name));
    }
    match key.parse::<i64>() {
        Ok(i) if i.to_string() == key => check_int(i).map(Key::Int),
        _ => Err(format!(
            "it has a field named \"{}\" that the add-on doesn't know",
            key.escape_default()
        )),
    }
}

fn check_int(i: i64) -> Result<i64, String> {
    if (-MAX_INTEGER..=MAX_INTEGER).contains(&i) {
        Ok(i)
    } else {
        Err(format!("the number {i} is too large"))
    }
}

fn check_str(s: &str) -> Result<(), String> {
    if s.len() > MAX_STRING_BYTES {
        let start: String = s.chars().take(30).collect();
        Err(format!(
            "a piece of text is {} bytes, over the {MAX_STRING_BYTES}-byte limit (it starts \"{}\")",
            s.len(),
            start.escape_default()
        ))
    } else {
        Ok(())
    }
}

fn write_value(value: &Lua, out: &mut Vec<u8>) {
    match value {
        Lua::Bool(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Lua::Int(i) => out.extend_from_slice(i.to_string().as_bytes()),
        Lua::Str(s) => {
            out.push(b'"');
            for &b in s.as_bytes() {
                match b {
                    b'\\' => out.extend_from_slice(b"\\\\"),
                    b'"' => out.extend_from_slice(b"\\\""),
                    0x20..=0x7e => out.push(b),
                    _ => out.extend_from_slice(format!("\\{b:03}").as_bytes()),
                }
            }
            out.push(b'"');
        }
        Lua::Table(fields) => {
            out.push(b'{');
            for (i, (key, v)) in fields.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                match key {
                    Some(Key::Name(name)) => out.extend_from_slice(format!("{name}=").as_bytes()),
                    Some(Key::Int(k)) => out.extend_from_slice(format!("[{k}]=").as_bytes()),
                    None => {}
                }
                write_value(v, out);
            }
            out.push(b'}');
        }
    }
}

/// Reads an inbox file back as data, using only the grammar of ADDON-SPEC 8.2.
fn parse_inbox(text: &[u8]) -> Result<Lua, String> {
    let mut p = Parser { text, pos: 0 };
    p.skip_space();
    if p.identifier()? != GLOBAL_NAME {
        return Err("not the inbox assignment".into());
    }
    p.expect(b'=')?;
    p.skip_space();
    if p.peek() != Some(b'{') {
        return Err("the inbox is not a table".into());
    }
    let value = p.value(1)?;
    p.skip_space();
    if p.pos != text.len() {
        return Err(format!("unexpected text at byte {}", p.pos));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.text.get(self.pos).copied()
    }

    fn skip_space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        self.skip_space();
        if self.peek() == Some(byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(format!("expected '{}' at byte {}", byte as char, self.pos))
        }
    }

    fn identifier(&mut self) -> Result<&str, String> {
        let start = self.pos;
        if matches!(self.peek(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_')) {
            while matches!(
                self.peek(),
                Some(b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'_')
            ) {
                self.pos += 1;
            }
        }
        if start == self.pos {
            return Err(format!("expected a name at byte {start}"));
        }
        Ok(std::str::from_utf8(&self.text[start..self.pos]).expect("ASCII"))
    }

    fn integer(&mut self) -> Result<i64, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let digits = std::str::from_utf8(&self.text[start..self.pos]).expect("ASCII");
        digits
            .parse::<i64>()
            .ok()
            .filter(|i| (-MAX_INTEGER..=MAX_INTEGER).contains(i))
            .ok_or_else(|| format!("bad number at byte {start}"))
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.pos;
        self.pos += 1; // opening quote
        let mut bytes = Vec::new();
        loop {
            match self.peek() {
                Some(b'"') => break,
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(b @ (b'\\' | b'"')) => {
                            bytes.push(b);
                            self.pos += 1;
                        }
                        _ => {
                            let digits = self.text.get(self.pos..self.pos + 3);
                            let byte = digits
                                .filter(|d| d.iter().all(u8::is_ascii_digit))
                                .and_then(|d| std::str::from_utf8(d).ok()?.parse::<u8>().ok())
                                .ok_or_else(|| format!("bad escape at byte {}", self.pos))?;
                            bytes.push(byte);
                            self.pos += 3;
                        }
                    }
                }
                Some(b @ 0x20..=0x7e) => {
                    bytes.push(b);
                    self.pos += 1;
                }
                _ => return Err(format!("bad string at byte {start}")),
            }
        }
        self.pos += 1; // closing quote
        if bytes.len() > MAX_STRING_BYTES {
            return Err(format!("string too long at byte {start}"));
        }
        String::from_utf8(bytes).map_err(|_| format!("string is not UTF-8 at byte {start}"))
    }

    fn value(&mut self, depth: usize) -> Result<Lua, String> {
        self.skip_space();
        match self.peek() {
            Some(b'{') => self.table(depth),
            Some(b'"') => self.string().map(Lua::Str),
            Some(b'-' | b'0'..=b'9') => self.integer().map(Lua::Int),
            _ => match self.identifier()? {
                "true" => Ok(Lua::Bool(true)),
                "false" => Ok(Lua::Bool(false)),
                other => Err(format!("unexpected name {other}")),
            },
        }
    }

    fn table(&mut self, depth: usize) -> Result<Lua, String> {
        if depth > MAX_DEPTH {
            return Err(format!("too deep at byte {}", self.pos));
        }
        self.pos += 1; // {
        let mut fields = Vec::new();
        loop {
            self.skip_space();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(Lua::Table(fields));
            }
            let key = self.key()?;
            fields.push((key, self.value(depth + 1)?));
            self.skip_space();
            match self.peek() {
                Some(b',') => self.pos += 1,
                Some(b'}') => {}
                _ => return Err(format!("expected ',' or '}}' at byte {}", self.pos)),
            }
        }
    }

    /// An optional `name =` or `[integer] =` before a value.
    fn key(&mut self) -> Result<Option<Key>, String> {
        let start = self.pos;
        if self.peek() == Some(b'[') {
            self.pos += 1;
            self.skip_space();
            let k = self.integer()?;
            self.expect(b']')?;
            self.expect(b'=')?;
            return Ok(Some(Key::Int(k)));
        }
        if matches!(self.peek(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_')) {
            let name = self.identifier()?.to_string();
            self.skip_space();
            if self.peek() == Some(b'=') {
                self.pos += 1;
                let known = KEYS.iter().find(|k| **k == name);
                return known
                    .map(|k| Some(Key::Name(k)))
                    .ok_or_else(|| format!("unknown key {name}"));
            }
        }
        self.pos = start;
        Ok(None)
    }
}

/// Writes the new contents to a temporary file beside `target`, then renames it over `target`, so
/// the game never reads a half-written file. On any failure the old file is untouched
/// and the temporary file is removed.
fn replace_file(
    target: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
    rename: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let temp = temp_path(target)?;
    let result = (|| {
        let mut file = File::create(&temp)?;
        write(&mut file)?;
        file.sync_all()?;
        drop(file);
        rename(&temp, target)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn temp_path(target: &Path) -> io::Result<PathBuf> {
    let name = target.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the inbox path has no file name",
        )
    })?;
    let mut temp_name = name.to_os_string();
    temp_name.push(format!(".{}.tmp", std::process::id()));
    Ok(target.with_file_name(temp_name))
}

#[cfg(test)]
mod tests;
