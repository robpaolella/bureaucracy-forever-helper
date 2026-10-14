//! Reads one line of the game's chat log (`WoWChatLog.txt`) and turns the add-on's own
//! award, undo and replacement lines into the operation to send (ADDON-SPEC 9).
//!
//! This is the privacy boundary: only raid chat carrying a `<BF…>` machine part is
//! ours. Every other line gives `NotOurs` and nothing is kept from it.

use chrono::{Datelike, Local, LocalResult, NaiveDate, NaiveTime, TimeZone};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Env {
    Production,
    Staging,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LootMethod {
    Hr,
    Sr,
    MainSpec,
    OffSpec,
    OpenRoll,
    DisenchantBank,
}

impl LootMethod {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "HR" => Self::Hr,
            "SR" => Self::Sr,
            "MAIN_SPEC" => Self::MainSpec,
            "OFF_SPEC" => Self::OffSpec,
            "OPEN_ROLL" => Self::OpenRoll,
            "DISENCHANT_BANK" => Self::DisenchantBank,
            _ => return None,
        })
    }
}

/// A raid the helper knows, from the site's snapshots: its id, environment and the ids
/// of its bosses (Trash included).
#[derive(Debug, Clone)]
pub struct KnownRaid {
    pub id: String,
    pub env: Env,
    pub bosses: Vec<String>,
}

/// The `award` operation of ADDON-SPEC section 4 (format version 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Award {
    pub id: String,
    pub raid: String,
    pub env: Env,
    pub practice: bool,
    pub boss: String,
    pub item: u32,
    pub char: Option<String>,
    pub method: LootMethod,
    pub roll: Option<u8>,
    pub sender: String,
    pub at: i64,
}

/// The `void` operation of section 4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Void {
    pub id: String,
    pub raid: String,
    pub env: Env,
    pub practice: bool,
    pub key: String,
    pub at: i64,
}

/// The `replace` operation of section 4: void `key` and record `award`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Replace {
    pub id: String,
    pub raid: String,
    pub env: Env,
    pub practice: bool,
    pub key: String,
    pub award: Box<Award>,
    pub at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    Award(Award),
    Void(Void),
    Replace(Replace),
}

/// Why a line of ours was rejected (ADDON-SPEC 9.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Malformed {
    SeveralMachineParts,
    Unclosed,
    TextAfterMachinePart,
    Version,
    Type,
    FieldCount,
    Escape,
    Raid,
    UnknownRaid,
    SeveralRaids,
    Id,
    Key,
    NewKey,
    SameKey,
    Item,
    Roll,
    Method,
    Winner,
    NoItemLink,
    ItemLinkMismatch,
    Practice,
    UnknownBoss,
    SeveralBosses,
    Sender,
    Time,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LineResult {
    /// Not an add-on line: ignore it and keep nothing.
    NotOurs,
    Operation(Operation),
    /// An add-on line that breaks the format; show it as a problem.
    Malformed(Malformed),
    /// An add-on line in a newer format than this helper reads.
    NeedsUpdate,
}

/// Reads a line with today's date and the time zone of this computer.
pub fn parse_line_now(line: &str, raids: &[KnownRaid]) -> LineResult {
    parse_line(line, Local::now().date_naive(), &Local, raids)
}

/// Reads one chat log line. `today` is the date in `tz`, the zone the log was written in.
pub fn parse_line<Tz: TimeZone>(
    line: &str,
    today: NaiveDate,
    tz: &Tz,
    raids: &[KnownRaid],
) -> LineResult {
    let Some(chat) = raid_chat(line) else {
        return LineResult::NotOurs;
    };
    let starts = machine_starts(chat.text);
    let Some(&start) = starts.first() else {
        return LineResult::NotOurs;
    };
    if starts.len() > 1 {
        return LineResult::Malformed(Malformed::SeveralMachineParts);
    }
    let version: String = chat.text[start + 3..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    if version != "1" && !version.starts_with('0') {
        return LineResult::NeedsUpdate;
    }
    match read_line(&chat, start, today, tz, raids) {
        Ok(op) => LineResult::Operation(op),
        Err(problem) => LineResult::Malformed(problem),
    }
}

struct RaidChat<'a> {
    date: &'a str,
    time: &'a str,
    sender: &'a str,
    text: &'a str,
}

/// Splits `10/13 10:01:51.269  |Hchannel:RAID|h[Raid Leader]|h Name: text`, or gives
/// `None` for anything but the RAID and RAID_LEADER channels.
fn raid_chat(line: &str) -> Option<RaidChat<'_>> {
    let line = line.trim_end_matches(['\r', '\n']);
    let (date, rest) = line.split_once(' ')?;
    let (time, rest) = rest.split_once(' ')?;
    let rest = rest.trim_start_matches(' ').strip_prefix("|Hchannel:")?;
    let (channel, rest) = rest.split_once("|h")?;
    if channel != "RAID" && channel != "RAID_LEADER" {
        return None;
    }
    let (_label, rest) = rest.strip_prefix('[')?.split_once("]|h ")?;
    let (sender, text) = rest.split_once(": ")?;
    Some(RaidChat {
        date,
        time,
        sender,
        text,
    })
}

/// Where each `<BF` followed by a digit starts.
fn machine_starts(text: &str) -> Vec<usize> {
    text.match_indices("<BF")
        .filter(|(i, _)| text.as_bytes().get(i + 3).is_some_and(u8::is_ascii_digit))
        .map(|(i, _)| i)
        .collect()
}

fn read_line<Tz: TimeZone>(
    chat: &RaidChat,
    start: usize,
    today: NaiveDate,
    tz: &Tz,
    raids: &[KnownRaid],
) -> Result<Operation, Malformed> {
    let machine = &chat.text[start + 1..];
    let Some((inner, after)) = machine.split_once('>') else {
        return Err(Malformed::Unclosed);
    };
    if !after.trim().is_empty() {
        return Err(Malformed::TextAfterMachinePart);
    }
    let raw: Vec<&str> = inner.split(';').collect();
    let practice = match raw[0] {
        "BF1" => false,
        "BF1P" => true,
        _ => return Err(Malformed::Version),
    };
    let human = &chat.text[..start];
    if human.starts_with("[Practice] ") != practice {
        return Err(Malformed::Practice);
    }
    let fields = raw[1..]
        .iter()
        .map(|f| decode(f))
        .collect::<Option<Vec<String>>>()
        .ok_or(Malformed::Escape)?;
    let want = match fields.first().map(String::as_str) {
        Some("A") => 8,
        Some("V") => 4,
        Some("R") => 10,
        _ => return Err(Malformed::Type),
    };
    if fields.len() != want {
        return Err(Malformed::FieldCount);
    }
    if chat.sender.is_empty() {
        return Err(Malformed::Sender);
    }
    let at = log_time(chat.date, chat.time, today, tz).ok_or(Malformed::Time)?;
    let raid = find_raid(&fields[1], raids)?;
    let line = Line {
        raid,
        practice,
        sender: chat.sender,
        at,
    };
    let op = match fields[0].as_str() {
        "A" => {
            let key = check_key(&fields[2], Malformed::Key)?;
            Operation::Award(line.award(key, &fields[3..], human)?)
        }
        "V" => {
            let id = check_key(&fields[2], Malformed::Id)?;
            let key = check_key(&fields[3], Malformed::Key)?;
            item_links(human, None)?;
            Operation::Void(Void {
                id,
                raid: raid.id.clone(),
                env: raid.env,
                practice,
                key,
                at,
            })
        }
        _ => {
            let id = check_key(&fields[2], Malformed::Id)?;
            let key = check_key(&fields[3], Malformed::Key)?;
            let new_key = check_key(&fields[4], Malformed::NewKey)?;
            if new_key == key {
                return Err(Malformed::SameKey);
            }
            Operation::Replace(Replace {
                id,
                raid: raid.id.clone(),
                env: raid.env,
                practice,
                key,
                award: Box::new(line.award(new_key, &fields[5..], human)?),
                at,
            })
        }
    };
    Ok(op)
}

/// What a line's context gives every operation in it.
struct Line<'a> {
    raid: &'a KnownRaid,
    practice: bool,
    sender: &'a str,
    at: i64,
}

impl Line<'_> {
    /// Builds an award from the fields `item;boss;method;roll;winner`.
    fn award(&self, key: String, f: &[String], human: &str) -> Result<Award, Malformed> {
        let item = digits(&f[0], 7).filter(|&n| n > 0).ok_or(Malformed::Item)?;
        let method = LootMethod::parse(&f[2]).ok_or(Malformed::Method)?;
        let roll = match f[3].as_str() {
            "" => None,
            r => Some(
                digits(r, 3)
                    .filter(|n| (1..=100).contains(n))
                    .ok_or(Malformed::Roll)? as u8,
            ),
        };
        let char = match (method, f[4].as_str()) {
            (LootMethod::DisenchantBank, "") => None,
            (LootMethod::DisenchantBank, _) | (_, "") => return Err(Malformed::Winner),
            (_, w) => Some(w.to_string()),
        };
        item_links(human, Some(item))?;
        let boss = find_boss(&f[1], self.raid)?;
        Ok(Award {
            id: key,
            raid: self.raid.id.clone(),
            env: self.raid.env,
            practice: self.practice,
            boss,
            item,
            char,
            method,
            roll,
            sender: self.sender.to_string(),
            at: self.at,
        })
    }
}

/// Decodes `%XX` escapes (ADDON-SPEC 9.3); `None` for any other `%`, a character that
/// should have been escaped, or invalid UTF-8.
fn decode(field: &str) -> Option<String> {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
            i += 3;
        } else if matches!(bytes[i], b'<' | b'>' | b'|' | 0..=0x1F | 0x7F) {
            return None;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A number written only in ASCII digits, at most `max` of them.
fn digits(s: &str, max: usize) -> Option<u32> {
    if s.is_empty() || s.len() > max || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse().ok()
}

/// An id or key: 8 to 24 letters, digits, `_` or `-` (sections 4 and 9.4).
fn check_key(s: &str, problem: Malformed) -> Result<String, Malformed> {
    let ok = (8..=24).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(s.to_string())
    } else {
        Err(problem)
    }
}

/// The human part needs an item link; with `item`, every `|Hitem:` number must equal it.
fn item_links(human: &str, item: Option<u32>) -> Result<(), Malformed> {
    let mut found = false;
    for (i, _) in human.match_indices("|Hitem:") {
        let n: String = human[i + 7..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        match (digits(&n, 10), item) {
            (Some(n), Some(item)) if n != item => return Err(Malformed::ItemLinkMismatch),
            (Some(_), _) => found = true,
            (None, _) => {}
        }
    }
    if found {
        Ok(())
    } else {
        Err(Malformed::NoItemLink)
    }
}

/// The last five characters of an id.
fn code_of(id: &str) -> String {
    let n = id.chars().count();
    id.chars().skip(n.saturating_sub(5)).collect()
}

/// The raid a code names: `P` or `S` and the raid id's last five characters, upper-case.
fn find_raid<'a>(code: &str, raids: &'a [KnownRaid]) -> Result<&'a KnownRaid, Malformed> {
    let env = match code.as_bytes().first() {
        Some(b'P') => Env::Production,
        Some(b'S') => Env::Staging,
        _ => return Err(Malformed::Raid),
    };
    let tail = &code[1..];
    if tail.len() != 5
        || !tail
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    {
        return Err(Malformed::Raid);
    }
    let mut matches = raids
        .iter()
        .filter(|r| r.env == env && code_of(&r.id).to_uppercase() == tail);
    match (matches.next(), matches.next()) {
        (Some(raid), None) => Ok(raid),
        (None, _) => Err(Malformed::UnknownRaid),
        _ => Err(Malformed::SeveralRaids),
    }
}

/// The boss id a boss code (its last five characters, as stored) names in the raid.
fn find_boss(code: &str, raid: &KnownRaid) -> Result<String, Malformed> {
    let mut matches = raid.bosses.iter().filter(|b| code_of(b) == code);
    match (matches.next(), matches.next()) {
        (Some(boss), None) => Ok(boss.clone()),
        (None, _) => Err(Malformed::UnknownBoss),
        _ => Err(Malformed::SeveralBosses),
    }
}

/// The log's `MM/DD HH:MM:SS.mmm` in `tz`, in the year that date was most recently in
/// (ADDON-SPEC 9.5), as Unix seconds.
fn log_time<Tz: TimeZone>(date: &str, time: &str, today: NaiveDate, tz: &Tz) -> Option<i64> {
    let (month, day) = date.split_once('/')?;
    let (month, day) = (digits(month, 2)?, digits(day, 2)?);
    let time = NaiveTime::parse_from_str(time, "%H:%M:%S%.f").ok()?;
    let mut year = today.year();
    if (month, day) > (today.month(), today.day()) {
        year -= 1;
    }
    // 29 February is in the last leap year, up to eight years back.
    let date = (0..8).find_map(|back| NaiveDate::from_ymd_opt(year - back, month, day))?;
    match tz.from_local_datetime(&date.and_time(time)) {
        LocalResult::Single(at) | LocalResult::Ambiguous(at, _) => Some(at.timestamp()),
        LocalResult::None => None,
    }
}
