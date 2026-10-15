//! A reader for the one assignment in the add-on's SavedVariables file, in the grammar of
//! ADDON-SPEC 8.2. It never runs anything: it reads characters and builds data, and
//! anything that isn't a literal table, string, number, boolean or `nil` is refused.
//!
//! The game writes this file itself, in its own layout (`["key"] = value,` entries,
//! `-- [1]` comments after list items, tabs, decimals), so beyond the grammar's plain
//! `name =` keys this accepts `["text"]` and `[number]` keys, `--` line comments, decimal
//! and hexadecimal numbers, and the usual string escapes. Tables become JSON: a table
//! whose keys are 1, 2, 3… in order is a list, any other is an object (empty is a list).

use serde_json::{Map, Number, Value};
use std::collections::HashSet;

/// ADDON-SPEC 7.3 rule 5. The spec's strings are 200 bytes, but a `note` may be 200
/// *characters*, which is up to 800 bytes, so the limit leaves room for that.
const MAX_STRING_BYTES: usize = 1000;
const MAX_DEPTH: usize = 8;

/// Why the text was refused, with the line it happened on (counting from 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refused {
    pub line: usize,
    pub why: String,
}

/// Reads `<global> = { … }` and returns the table. Nothing may follow it but space and comments.
pub fn parse_assignment(text: &[u8], global: &str) -> Result<Value, Refused> {
    let mut p = Parser { text, pos: 0 };
    p.skip_space();
    let name = p.identifier()?;
    if name != global {
        return Err(p.not_data(&name));
    }
    p.skip_space();
    if p.peek() != Some(b'=') {
        return Err(p.unexpected());
    }
    p.pos += 1;
    p.skip_space();
    if p.peek() != Some(b'{') {
        return Err(match p.identifier() {
            Ok(name) => p.not_data(&name),
            Err(_) => p.unexpected(),
        });
    }
    let value = p.table(1)?;
    p.skip_space();
    if p.peek().is_some() {
        return Err(match p.identifier() {
            Ok(name) => p.not_data(&name),
            Err(_) => p.unexpected(),
        });
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

    fn line(&self) -> usize {
        1 + self.text[..self.pos.min(self.text.len())]
            .iter()
            .filter(|b| **b == b'\n')
            .count()
    }

    fn refuse(&self, why: String) -> Refused {
        Refused {
            line: self.line(),
            why,
        }
    }

    /// A name that isn't `true`, `false` or `nil`: code, or at least not saved data.
    fn not_data(&self, name: &str) -> Refused {
        self.refuse(format!(
            "it contains code or other text that isn't saved data (\"{}\")",
            shown(name.as_bytes())
        ))
    }

    fn unexpected(&self) -> Refused {
        match self.peek() {
            Some(b) => self.refuse(format!(
                "it isn't valid saved data (unexpected \"{}\")",
                shown(&[b])
            )),
            None => self.refuse("it ends too soon".into()),
        }
    }

    /// Skips spaces, newlines and `--` comments to the end of their line.
    fn skip_space(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.pos += 1,
                Some(b'-') if self.text.get(self.pos + 1) == Some(&b'-') => {
                    while !matches!(self.peek(), None | Some(b'\n')) {
                        self.pos += 1;
                    }
                }
                _ => return,
            }
        }
    }

    fn identifier(&mut self) -> Result<String, Refused> {
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
            return Err(self.unexpected());
        }
        Ok(String::from_utf8_lossy(&self.text[start..self.pos]).into_owned())
    }

    fn number(&mut self) -> Result<Value, Refused> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        let hex =
            self.peek() == Some(b'0') && matches!(self.text.get(self.pos + 1), Some(b'x' | b'X'));
        if hex {
            self.pos += 2;
        }
        let mut float = false;
        while let Some(b) = self.peek() {
            let part = match b {
                b'0'..=b'9' => true,
                b'a'..=b'f' | b'A'..=b'F' if hex => true,
                b'.' | b'e' | b'E' if !hex => {
                    float = true;
                    true
                }
                b'+' | b'-' if !hex && matches!(self.text[self.pos - 1], b'e' | b'E') => true,
                _ => false,
            };
            if !part {
                break;
            }
            self.pos += 1;
        }
        let raw = std::str::from_utf8(&self.text[start..self.pos]).expect("ASCII");
        let (negative, digits) = match raw.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, raw),
        };
        let number = if hex {
            i64::from_str_radix(&digits[2..], 16)
                .ok()
                .map(|n| Number::from(if negative { -n } else { n }))
        } else if float {
            raw.parse::<f64>().ok().and_then(Number::from_f64)
        } else {
            raw.parse::<i64>().ok().map(Number::from)
        };
        number.map(Value::Number).ok_or_else(|| {
            self.refuse(format!(
                "a number isn't valid (\"{}\")",
                shown(raw.as_bytes())
            ))
        })
    }

    fn string(&mut self) -> Result<String, Refused> {
        let quote = self.text[self.pos];
        self.pos += 1;
        let mut bytes = Vec::new();
        loop {
            match self.peek() {
                Some(b) if b == quote => break,
                None | Some(b'\n') => return Err(self.refuse("a text isn't closed".into())),
                Some(b'\\') => {
                    self.pos += 1;
                    bytes.push(self.escape()?);
                }
                Some(b) => {
                    bytes.push(b);
                    self.pos += 1;
                }
            }
            if bytes.len() > MAX_STRING_BYTES {
                return Err(self.refuse(format!(
                    "a text is longer than the limit of {MAX_STRING_BYTES} bytes"
                )));
            }
        }
        self.pos += 1;
        String::from_utf8(bytes).map_err(|_| self.refuse("a text isn't valid UTF-8".into()))
    }

    /// The byte for the escape after a backslash.
    fn escape(&mut self) -> Result<u8, Refused> {
        let Some(b) = self.peek() else {
            return Err(self.refuse("a text isn't closed".into()));
        };
        self.pos += 1;
        Ok(match b {
            b'n' | b'\n' => b'\n',
            b'r' => b'\r',
            b't' => b'\t',
            b'a' => 7,
            b'b' => 8,
            b'f' => 12,
            b'v' => 11,
            b'\\' | b'"' | b'\'' => b,
            b'0'..=b'9' => {
                let mut n = u32::from(b - b'0');
                for _ in 0..2 {
                    match self.peek() {
                        Some(d @ b'0'..=b'9') => {
                            n = n * 10 + u32::from(d - b'0');
                            self.pos += 1;
                        }
                        _ => break,
                    }
                }
                u8::try_from(n).map_err(|_| self.refuse("a text has a bad \\ code".into()))?
            }
            _ => return Err(self.refuse("a text has a bad \\ code".into())),
        })
    }

    fn value(&mut self, depth: usize) -> Result<Option<Value>, Refused> {
        self.skip_space();
        match self.peek() {
            Some(b'{') => self.table(depth).map(Some),
            Some(b'"' | b'\'') => self.string().map(|s| Some(Value::String(s))),
            Some(b'-' | b'0'..=b'9') => self.number().map(Some),
            _ => match self.identifier()?.as_str() {
                "true" => Ok(Some(Value::Bool(true))),
                "false" => Ok(Some(Value::Bool(false))),
                "nil" => Ok(None),
                other => Err(self.not_data(other)),
            },
        }
    }

    fn table(&mut self, depth: usize) -> Result<Value, Refused> {
        if depth > MAX_DEPTH {
            return Err(self.refuse(format!("it nests deeper than the limit of {MAX_DEPTH}")));
        }
        self.pos += 1; // {
        let mut entries: Vec<(Key, Value)> = Vec::new();
        let mut next = 1;
        let mut seen: HashSet<Key> = HashSet::new();
        loop {
            self.skip_space();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                break;
            }
            let key = self.key()?.unwrap_or_else(|| {
                next += 1;
                Key::Int(next - 1)
            });
            let at = self.pos;
            if let Some(value) = self.value(depth + 1)? {
                if !seen.insert(key.clone()) {
                    self.pos = at;
                    return Err(self.refuse("a table repeats a key".into()));
                }
                entries.push((key, value));
            }
            self.skip_space();
            match self.peek() {
                Some(b',' | b';') => self.pos += 1,
                Some(b'}') => {}
                _ => return Err(self.unexpected()),
            }
        }
        Ok(to_json(entries))
    }

    /// The key before `=`, if there is one: `name`, `["text"]` or `[number]`.
    fn key(&mut self) -> Result<Option<Key>, Refused> {
        let start = self.pos;
        let key = match self.peek() {
            Some(b'[') => {
                self.pos += 1;
                self.skip_space();
                let key = match self.peek() {
                    Some(b'"' | b'\'') => Key::Name(self.string()?),
                    Some(b'-' | b'0'..=b'9') => match self.number()? {
                        Value::Number(n) if n.is_i64() => Key::Int(n.as_i64().unwrap_or(0)),
                        Value::Number(n) => Key::Name(n.to_string()),
                        _ => unreachable!("number() returns numbers"),
                    },
                    _ => return Err(self.unexpected()),
                };
                self.skip_space();
                if self.peek() != Some(b']') {
                    return Err(self.unexpected());
                }
                self.pos += 1;
                key
            }
            Some(b'A'..=b'Z' | b'a'..=b'z' | b'_') => Key::Name(self.identifier()?),
            _ => return Ok(None),
        };
        self.skip_space();
        if self.peek() == Some(b'=') {
            self.pos += 1;
            Ok(Some(key))
        } else {
            // A bare name is a value (`true`, or code, which `value` refuses).
            self.pos = start;
            Ok(None)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    Name(String),
    Int(i64),
}

fn to_json(entries: Vec<(Key, Value)>) -> Value {
    let is_list = entries
        .iter()
        .enumerate()
        .all(|(i, (k, _))| *k == Key::Int(i as i64 + 1));
    if is_list {
        return Value::Array(entries.into_iter().map(|(_, v)| v).collect());
    }
    let mut map = Map::new();
    for (key, value) in entries {
        match key {
            Key::Name(name) => map.insert(name, value),
            Key::Int(n) => map.insert(n.to_string(), value),
        };
    }
    Value::Object(map)
}

/// Up to 24 printable characters of `bytes`, so a message can quote the file safely.
fn shown(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(24)
        .map(|b| match b {
            0x20..=0x7e => *b as char,
            _ => '?',
        })
        .collect()
}
