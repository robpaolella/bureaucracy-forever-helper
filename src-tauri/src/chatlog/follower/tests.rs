use super::*;
use crate::chatlog::Env;
use crate::queue::Ack;
use std::collections::HashMap;
use std::io::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};

const LINK: &str = "|cffa335ee|Hitem:16805::::::::60:::::::|h[Felheart Gloves]|h|r";
const RAID: &str = "10/13 10:01:51.269  |Hchannel:RAID_LEADER|h[Raid Leader]|h Rae Quill: ";
/// Chat that must never leave the computer or be kept.
const PRIVATE: &str = "meet me at the bank, my password is hunter2";

fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-chatlog-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn raids() -> Vec<NamedRaid> {
    vec![NamedRaid {
        raid: KnownRaid {
            id: "cm9xr2k4a0001".into(),
            env: Env::Staging,
            bosses: vec!["lb_lucifron".into(), "lb_trash".into()],
        },
        name: "Molten Core".into(),
        boss_names: HashMap::from([
            ("lb_lucifron".into(), "Lucifron".into()),
            ("lb_trash".into(), "Trash".into()),
        ]),
    }]
}

fn award(key: &str, winner: &str) -> String {
    format!("{RAID}{LINK} goes to {winner} (HR, roll 91) <BF1;A;SA0001;{key};16805;ifron;HR;91;{winner}>\n")
}

fn undo(id: &str, key: &str) -> String {
    format!("{RAID}Undo: {LINK} no longer goes to Zoé Brûlé <BF1;V;SA0001;{id};{key}>\n")
}

fn replacement(id: &str, key: &str, new_key: &str) -> String {
    format!("{RAID}Correction: {LINK} goes to Bo Marsh (SR, roll 77), not Zoé Brûlé <BF1;R;SA0001;{id};{key};{new_key};16805;ifron;SR;77;Bo Marsh>\n")
}

/// Every kind of chat that isn't ours, each carrying private text.
fn other_chat() -> String {
    let machine = "<BF1;A;SA0001;aw-0guild00;16805;ifron;HR;91;Zoé>";
    [
        format!("10/13 10:01:51.269  |Hchannel:GUILD|h[Guild]|h Rae Quill: {PRIVATE} {machine}"),
        format!("10/13 10:01:51.269  |Hchannel:OFFICER|h[Officer]|h Rae Quill: {PRIVATE}"),
        format!("10/13 10:01:51.269  Rae Quill whispers: {PRIVATE} {machine}"),
        format!("10/13 10:01:51.269  To Rae Quill: {PRIVATE}"),
        format!("10/13 10:01:51.269  Rae Quill says: {PRIVATE}"),
        format!("{RAID}{PRIVATE}"),
        "10/13 10:01:51.269  Shootier Hek receives loot: [Nomad Buckler of Healing].".into(),
    ]
    .map(|l| l + "\n")
    .concat()
}

struct Rig {
    dir: PathBuf,
    log: PathBuf,
    queue: Mutex<Queue>,
    follower: Follower,
}

impl Rig {
    /// A follower with no saved position and an empty log.
    fn new() -> Rig {
        let rig = Rig::without_log();
        fs::write(&rig.log, "").unwrap();
        rig
    }

    fn without_log() -> Rig {
        let dir = scratch_dir();
        let log = dir.join("WoWChatLog.txt");
        Rig {
            queue: Mutex::new(Queue::open(dir.join("waiting.json"))),
            follower: Follower::open(dir.join(FILE_NAME)),
            dir,
            log,
        }
    }

    /// What one pass reports: item names as they were announced, then the rest.
    fn pass(&mut self) -> Vec<ChatEvent> {
        let mut events = Vec::new();
        let rest = self
            .follower
            .pass(Some(&self.log), Some(&raids()), &self.queue, &mut |e| {
                events.push(e)
            });
        events.extend(rest);
        events
    }

    fn append(&self, text: &str) {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&self.log)
            .unwrap();
        file.write_all(text.as_bytes()).unwrap();
    }

    fn ids(&self) -> Vec<String> {
        let queue = self.queue.lock().unwrap();
        let ops = queue.operations().unwrap();
        ops.iter()
            .map(|op| op["id"].as_str().unwrap().to_string())
            .collect()
    }

    /// The site acknowledges every waiting operation.
    fn acknowledge_all(&self) {
        let mut queue = self.queue.lock().unwrap();
        for op in queue.operations().unwrap().to_vec() {
            queue.remove_acknowledged(&op, &Ack::Recorded).unwrap();
        }
    }

    fn acknowledge(&self, id: &str) {
        let mut queue = self.queue.lock().unwrap();
        let op = queue
            .operations()
            .unwrap()
            .iter()
            .find(|op| op["id"] == id)
            .unwrap()
            .clone();
        queue.remove_acknowledged(&op, &Ack::Recorded).unwrap();
    }

    fn saved_offset(&self) -> u64 {
        let saved: Value =
            serde_json::from_slice(&fs::read(self.dir.join(FILE_NAME)).unwrap()).unwrap();
        saved["offset"].as_u64().unwrap()
    }

    fn log_len(&self) -> u64 {
        fs::metadata(&self.log).unwrap().len()
    }

    /// The follower as after a restart: its saved position and queue read from disk again.
    fn restart(&mut self) {
        self.follower = Follower::open(self.dir.join(FILE_NAME));
        self.queue = Mutex::new(Queue::open(self.dir.join("waiting.json")));
    }
}

fn sent_items(events: &[ChatEvent]) -> Vec<(LineKind, Option<String>)> {
    events
        .iter()
        .filter_map(|e| match e {
            ChatEvent::Sent(s) => Some((s.line, s.character.clone())),
            ChatEvent::Ignored { .. } | ChatEvent::ItemName { .. } => None,
        })
        .collect()
}

#[test]
fn only_add_on_lines_reach_the_queue_and_no_other_chat_is_kept() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&other_chat());
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.append(&other_chat());
    rig.append(&undo("vd-2s9t4w7y", "aw-9d2f6h1j"));
    rig.append(&replacement("rp-3n7b5x0c", "aw-9d2f6h1j", "aw-6k1m4v8z"));
    let events = rig.pass();
    assert_eq!(rig.ids(), ["aw-9d2f6h1j", "vd-2s9t4w7y", "rp-3n7b5x0c"]);
    // Handing the lines on only names their item; nothing is sent yet.
    assert!(sent_items(&events).is_empty(), "{events:?}");
    assert!(events
        .iter()
        .all(|e| matches!(e, ChatEvent::ItemName { .. })));
    rig.acknowledge_all();
    rig.pass();
    // Nothing the helper saved holds any other chat.
    for entry in fs::read_dir(&rig.dir).unwrap() {
        let path = entry.unwrap().path();
        if path == rig.log {
            continue;
        }
        let text = fs::read_to_string(&path).unwrap();
        for word in ["hunter2", "bank", "Guild", "Officer", "whispers", "Nomad"] {
            assert!(
                !text.contains(word),
                "{} holds {word}: {text}",
                path.display()
            );
        }
    }
    let saved: Value = serde_json::from_slice(&fs::read(rig.dir.join(FILE_NAME)).unwrap()).unwrap();
    let mut keys: Vec<_> = saved.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "formatVersion",
            "headHash",
            "headLength",
            "log",
            "offset",
            "tailHash",
            "tailLength"
        ]
    );
}

#[test]
fn operations_go_to_the_queue_as_the_site_takes_them() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.append(&undo("vd-2s9t4w7y", "aw-9d2f6h1j"));
    rig.append(&replacement("rp-3n7b5x0c", "aw-9d2f6h1j", "aw-6k1m4v8z"));
    rig.pass();
    let queue = rig.queue.lock().unwrap();
    let ops = queue.operations().unwrap();
    let at = ops[0]["at"].clone();
    assert!(at.as_i64().is_some());
    let award = json!({ "type": "award", "id": "aw-9d2f6h1j", "formatVersion": 1,
        "raid": "cm9xr2k4a0001", "env": "staging", "boss": "lb_lucifron", "item": 16805,
        "char": "Zoé Brûlé", "method": "HR", "roll": 91, "sender": "Rae Quill", "at": at });
    assert_eq!(ops[0], award);
    assert_eq!(
        ops[1],
        json!({ "type": "void", "id": "vd-2s9t4w7y", "formatVersion": 1,
            "raid": "cm9xr2k4a0001", "env": "staging", "key": "aw-9d2f6h1j", "at": at })
    );
    let new_award = json!({ "type": "award", "id": "aw-6k1m4v8z", "formatVersion": 1,
        "raid": "cm9xr2k4a0001", "env": "staging", "boss": "lb_lucifron", "item": 16805,
        "char": "Bo Marsh", "method": "SR", "roll": 77, "sender": "Rae Quill", "at": at });
    assert_eq!(
        ops[2],
        json!({ "type": "replace", "id": "rp-3n7b5x0c", "formatVersion": 1,
            "raid": "cm9xr2k4a0001", "env": "staging", "key": "aw-9d2f6h1j",
            "award": new_award, "at": at })
    );
}

#[test]
fn a_first_start_begins_at_the_end_of_an_existing_log() {
    let mut rig = Rig::without_log();
    rig.append(&award("aw-0ld00001", "Ana Vale"));
    rig.append(&other_chat());
    assert!(rig.pass().is_empty());
    assert!(rig.ids().is_empty());
    assert_eq!(rig.saved_offset(), rig.log_len());
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    assert_eq!(rig.ids(), ["aw-9d2f6h1j"]);
    rig.acknowledge_all();
    let events = rig.pass();
    assert_eq!(
        sent_items(&events),
        [(LineKind::Award, Some("Zoé Brûlé".into()))]
    );
}

#[test]
fn a_log_that_appears_after_the_first_start_is_read_from_its_beginning() {
    let mut rig = Rig::without_log();
    assert!(rig.pass().is_empty());
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    assert_eq!(rig.ids(), ["aw-9d2f6h1j"]);
}

#[test]
fn a_missing_log_does_nothing_and_keeps_the_position() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    rig.acknowledge_all();
    rig.pass();
    let offset = rig.saved_offset();
    let kept = fs::read(&rig.log).unwrap();
    fs::remove_file(&rig.log).unwrap();
    assert!(rig.pass().is_empty());
    assert_eq!(rig.saved_offset(), offset);
    // The same log back, grown: only the new line is read.
    fs::write(&rig.log, kept).unwrap();
    rig.append(&award("aw-6k1m4v8z", "Bo Marsh"));
    rig.pass();
    assert_eq!(rig.ids(), ["aw-6k1m4v8z"]);
}

#[test]
fn nothing_is_read_before_the_raids_are_known_or_without_a_log() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    let log = rig.log.clone();
    assert!(rig
        .follower
        .pass(Some(&log), None, &rig.queue, &mut |_| {})
        .is_empty());
    assert!(rig
        .follower
        .pass(None, Some(&raids()), &rig.queue, &mut |_| {})
        .is_empty());
    assert!(rig.ids().is_empty());
    rig.pass();
    assert_eq!(rig.ids(), ["aw-9d2f6h1j"]);
}

#[test]
fn a_partial_last_line_waits_until_it_is_complete() {
    let mut rig = Rig::new();
    rig.pass();
    let line = award("aw-9d2f6h1j", "Zoé Brûlé");
    let (first, rest) = line.split_at(line.find("goes to").unwrap());
    rig.append(first);
    assert!(rig.pass().is_empty());
    assert!(rig.ids().is_empty());
    assert_eq!(rig.saved_offset(), 0);
    rig.append(rest);
    rig.pass();
    assert_eq!(rig.ids(), ["aw-9d2f6h1j"]);
}

#[test]
fn the_position_moves_only_once_the_site_acknowledges() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&other_chat());
    let before_awards = rig.log_len();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    let between = rig.log_len();
    rig.append(&award("aw-6k1m4v8z", "Bo Marsh"));
    // Waiting: the position stops before the first line not acknowledged.
    assert!(sent_items(&rig.pass()).is_empty());
    assert_eq!(rig.saved_offset(), before_awards);
    // The later line acknowledged first: it shows as sent, the position still waits.
    rig.acknowledge("aw-6k1m4v8z");
    assert_eq!(
        sent_items(&rig.pass()),
        [(LineKind::Award, Some("Bo Marsh".into()))]
    );
    assert_eq!(rig.saved_offset(), before_awards);
    rig.acknowledge("aw-9d2f6h1j");
    assert_eq!(
        sent_items(&rig.pass()),
        [(LineKind::Award, Some("Zoé Brûlé".into()))]
    );
    assert_eq!(rig.saved_offset(), rig.log_len());
    assert!(between < rig.log_len());
    assert!(rig.pass().is_empty());
}

#[test]
fn offline_then_back_loses_and_doubles_nothing() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.append(&award("aw-6k1m4v8z", "Bo Marsh"));
    // The site can't be reached: nothing is acknowledged, pass after pass.
    // The first pass only names their items.
    assert_eq!(rig.pass().len(), 2);
    for _ in 0..3 {
        assert!(rig.pass().is_empty());
    }
    assert_eq!(rig.ids(), ["aw-9d2f6h1j", "aw-6k1m4v8z"]);
    assert_eq!(rig.saved_offset(), 0);
    rig.acknowledge_all();
    assert_eq!(sent_items(&rig.pass()).len(), 2);
    assert_eq!(rig.saved_offset(), rig.log_len());
    assert!(rig.pass().is_empty());
    assert!(rig.ids().is_empty());
}

#[test]
fn a_restart_reads_again_from_the_saved_position() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    let after_first = rig.log_len();
    rig.append(&award("aw-6k1m4v8z", "Bo Marsh"));
    rig.pass();
    rig.acknowledge("aw-9d2f6h1j");
    rig.pass();
    assert_eq!(rig.saved_offset(), after_first);
    rig.restart();
    rig.append(&award("aw-4t1y7g5k", "Dee Holt"));
    rig.pass();
    // The acknowledged line isn't read again; the waiting one is held once.
    assert_eq!(rig.ids(), ["aw-6k1m4v8z", "aw-4t1y7g5k"]);
    rig.acknowledge_all();
    assert_eq!(
        sent_items(&rig.pass()),
        [
            (LineKind::Award, Some("Bo Marsh".into())),
            (LineKind::Award, Some("Dee Holt".into()))
        ]
    );
    assert_eq!(rig.saved_offset(), rig.log_len());
}

#[test]
fn a_truncated_log_is_read_again_from_its_beginning() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&other_chat());
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    rig.acknowledge_all();
    rig.pass();
    fs::write(&rig.log, award("aw-6k1m4v8z", "Bo Marsh")).unwrap();
    rig.pass();
    assert_eq!(rig.ids(), ["aw-6k1m4v8z"]);
    rig.acknowledge_all();
    rig.pass();
    assert_eq!(rig.saved_offset(), rig.log_len());
}

#[test]
fn a_replaced_log_is_read_from_its_beginning_even_when_longer() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    rig.acknowledge_all();
    rig.pass();
    // While the helper was closed, the log was deleted and a new one grew past the old end.
    let replaced = format!(
        "{}{}{}",
        award("aw-6k1m4v8z", "Bo Marsh"),
        other_chat(),
        award("aw-4t1y7g5k", "Dee Holt")
    );
    assert!(replaced.len() as u64 > rig.log_len());
    fs::write(&rig.log, replaced).unwrap();
    rig.restart();
    rig.pass();
    assert_eq!(rig.ids(), ["aw-6k1m4v8z", "aw-4t1y7g5k"]);
}

#[test]
fn a_replaced_log_with_the_same_start_is_read_from_its_beginning() {
    let mut rig = Rig::new();
    rig.pass();
    let start = other_chat().repeat(4);
    assert!(start.len() as u64 > HEAD);
    rig.append(&start);
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    rig.acknowledge_all();
    rig.pass();
    // A new log that begins the same, with a different line where the old one ended.
    let replaced = format!(
        "{start}{}{}",
        award("aw-6k1m4v8z", "Bo Marsh"),
        award("aw-4t1y7g5k", "Dee Holt")
    );
    fs::write(&rig.log, replaced).unwrap();
    rig.restart();
    rig.pass();
    assert_eq!(rig.ids(), ["aw-6k1m4v8z", "aw-4t1y7g5k"]);
    // The same while running.
    rig.acknowledge_all();
    rig.pass();
    fs::write(
        &rig.log,
        format!(
            "{start}{}{}",
            award("aw-5r8u2i6o", "Ana Vale"),
            other_chat()
        ),
    )
    .unwrap();
    rig.pass();
    assert_eq!(rig.ids(), ["aw-5r8u2i6o"]);
}

#[test]
fn a_line_waiting_when_the_log_is_replaced_is_still_reported_once_sent() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    fs::write(&rig.log, other_chat()).unwrap();
    assert!(rig.pass().is_empty());
    // The new log's position doesn't wait for the old log's line.
    assert_eq!(rig.saved_offset(), rig.log_len());
    rig.acknowledge_all();
    assert_eq!(
        sent_items(&rig.pass()),
        [(LineKind::Award, Some("Zoé Brûlé".into()))]
    );
    assert!(rig.pass().is_empty());
}

#[test]
fn a_refused_line_moves_the_position_but_is_not_reported_as_sent() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.append(&award("aw-6k1m4v8z", "Bo Marsh"));
    rig.pass();
    {
        let mut queue = rig.queue.lock().unwrap();
        let ops = queue.operations().unwrap().to_vec();
        let refused = Ack::Rejected {
            code: "unknown-sender".into(),
        };
        queue.remove_acknowledged(&ops[0], &refused).unwrap();
        queue.remove_acknowledged(&ops[1], &Ack::Replayed).unwrap();
    }
    assert_eq!(
        sent_items(&rig.pass()),
        [(LineKind::Award, Some("Bo Marsh".into()))]
    );
    assert_eq!(rig.saved_offset(), rig.log_len());
}

#[test]
fn a_line_whose_outcome_is_forgotten_moves_the_position_but_is_not_reported_as_sent() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.pass();
    {
        let mut queue = rig.queue.lock().unwrap();
        let op = queue.operations().unwrap()[0].clone();
        let refused = Ack::Rejected {
            code: "unknown-sender".into(),
        };
        queue.remove_acknowledged(&op, &refused).unwrap();
        // Enough other acknowledgements that the refusal is forgotten.
        queue.keep_only(2);
        for n in 0..2 {
            let other = json!({ "type": "confirm", "id": format!("cf-{n:08}"),
                "formatVersion": 1, "raid": "cm9xr2k4a0001", "env": "staging",
                "key": "aw-9d2f6h1j", "at": 1 });
            queue.add(other.clone()).unwrap();
            queue.remove_acknowledged(&other, &Ack::Recorded).unwrap();
        }
        assert_eq!(queue.acknowledgement(&op), None);
    }
    assert!(sent_items(&rig.pass()).is_empty());
    assert_eq!(rig.saved_offset(), rig.log_len());
}

#[test]
fn malformed_and_newer_lines_are_ignored_and_the_next_line_still_goes() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&format!(
        "{RAID}{LINK} goes to Zoé <BF1;A;SA0001;aw-9d2f6h1j;16805;ifron;HR>\n"
    ));
    rig.append(&format!(
        "{RAID}{LINK} goes to Zoé <BF2;A;SA0001;aw-9d2f6h1j;16805>\n"
    ));
    rig.append(&award("aw-6k1m4v8z", "Bo Marsh"));
    let events = rig.pass();
    assert_eq!(
        events,
        [
            ChatEvent::ItemName {
                item: 16805,
                item_name: "Felheart Gloves".into()
            },
            ChatEvent::Ignored {
                reason: Ignored::Malformed
            },
            ChatEvent::Ignored {
                reason: Ignored::NewerVersion
            },
        ]
    );
    assert_eq!(rig.ids(), ["aw-6k1m4v8z"]);
    rig.acknowledge_all();
    assert_eq!(
        sent_items(&rig.pass()),
        [(LineKind::Award, Some("Bo Marsh".into()))]
    );
    assert_eq!(rig.saved_offset(), rig.log_len());
}

#[test]
fn a_handed_line_reports_its_item_name_before_the_site_answers() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    let events = rig.pass();
    assert_eq!(
        events,
        [ChatEvent::ItemName {
            item: 16805,
            item_name: "Felheart Gloves".into()
        }]
    );
    assert_eq!(
        serde_json::to_value(&events[0]).unwrap(),
        json!({ "kind": "itemName", "item": 16805, "itemName": "Felheart Gloves" })
    );
    // A refusal still reports nothing further.
    {
        let mut queue = rig.queue.lock().unwrap();
        let ops = queue.operations().unwrap().to_vec();
        let refused = Ack::Rejected {
            code: "unknown-sender".into(),
        };
        queue.remove_acknowledged(&ops[0], &refused).unwrap();
    }
    assert!(rig.pass().is_empty());
}

#[test]
fn an_item_name_is_announced_before_its_operation_can_be_refused() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    let queue = &rig.queue;
    let mut waiting_when_named = None;
    rig.follower
        .pass(Some(&rig.log), Some(&raids()), queue, &mut |e| {
            assert!(matches!(e, ChatEvent::ItemName { .. }));
            waiting_when_named = Some(queue.lock().unwrap().operations().unwrap().len());
        });
    assert_eq!(waiting_when_named, Some(0));
    assert_eq!(rig.ids(), ["aw-9d2f6h1j"]);
}

#[test]
fn a_sent_line_carries_what_the_window_shows() {
    let mut rig = Rig::new();
    rig.pass();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.append(&undo("vd-2s9t4w7y", "aw-9d2f6h1j"));
    rig.append(&replacement("rp-3n7b5x0c", "aw-9d2f6h1j", "aw-6k1m4v8z"));
    rig.pass();
    rig.acknowledge_all();
    let events: Vec<_> = rig.pass();
    let ChatEvent::Sent(first) = &events[0] else {
        panic!("{events:?}");
    };
    let at = first.at;
    let sent = |line, character: Option<&str>, boss: Option<&str>| {
        ChatEvent::Sent(SentLine {
            line,
            item: Some(16805),
            item_name: Some("Felheart Gloves".into()),
            character: character.map(Into::into),
            boss: boss.map(Into::into),
            raid: "cm9xr2k4a0001".into(),
            raid_name: "Molten Core".into(),
            at,
        })
    };
    assert_eq!(
        events,
        [
            sent(LineKind::Award, Some("Zoé Brûlé"), Some("Lucifron")),
            sent(LineKind::Undo, None, None),
            sent(LineKind::Replacement, Some("Bo Marsh"), Some("Lucifron")),
        ]
    );
    assert_eq!(
        serde_json::to_value(&events[0]).unwrap(),
        json!({ "kind": "sent", "line": "award", "item": 16805, "itemName": "Felheart Gloves",
            "character": "Zoé Brûlé", "boss": "Lucifron", "raid": "cm9xr2k4a0001",
            "raidName": "Molten Core", "at": at })
    );
    assert_eq!(
        serde_json::to_value(ChatEvent::Ignored {
            reason: Ignored::NewerVersion
        })
        .unwrap(),
        json!({ "kind": "ignored", "reason": "newerVersion" })
    );
}

#[test]
fn a_large_log_is_read_in_pieces() {
    let mut rig = Rig::without_log();
    rig.pass();
    // Far more than one piece of other chat, a line longer than any chat line, then ours.
    let chat = other_chat();
    while rig.log.metadata().map_or(0, |m| m.len()) < 3 * PIECE as u64 {
        rig.append(&chat);
    }
    rig.append(&"x".repeat(PIECE + 10));
    rig.append("\n");
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    rig.append(&chat);
    rig.append(&award("aw-6k1m4v8z", "Bo Marsh"));
    rig.pass();
    assert_eq!(rig.ids(), ["aw-9d2f6h1j", "aw-6k1m4v8z"]);
    rig.acknowledge_all();
    rig.pass();
    assert_eq!(rig.saved_offset(), rig.log_len());
}

#[test]
fn a_line_the_queue_cant_take_is_read_again_once_it_can() {
    let mut rig = Rig::new();
    rig.pass();
    fs::write(rig.dir.join("waiting.json"), "not a list").unwrap();
    rig.queue.lock().unwrap().reload();
    rig.append(&award("aw-9d2f6h1j", "Zoé Brûlé"));
    // The name is announced each try; nothing is sent.
    assert!(sent_items(&rig.pass()).is_empty());
    assert_eq!(rig.saved_offset(), 0);
    fs::remove_file(rig.dir.join("waiting.json")).unwrap();
    rig.queue.lock().unwrap().reload();
    rig.pass();
    assert_eq!(rig.ids(), ["aw-9d2f6h1j"]);
}

#[test]
fn the_tail_follows_the_log_in_the_game_folder() {
    let dir = scratch_dir();
    fs::create_dir_all(dir.join("Logs")).unwrap();
    fs::write(log_in(&dir), "").unwrap();
    let queue = Arc::new(Mutex::new(Queue::open(dir.join("waiting.json"))));
    let events = Arc::new(Mutex::new(Vec::new()));
    let tail = Tail::start(
        Follower::open(dir.join(FILE_NAME)),
        {
            let dir = dir.clone();
            move || Some(dir.clone())
        },
        || Some(raids()),
        Arc::clone(&queue),
        Duration::from_millis(10),
        {
            let events = Arc::clone(&events);
            move |e: &ChatEvent| events.lock().unwrap().push(e.clone())
        },
    );
    std::thread::sleep(Duration::from_millis(100));
    fs::OpenOptions::new()
        .append(true)
        .open(log_in(&dir))
        .unwrap()
        .write_all(award("aw-9d2f6h1j", "Zoé Brûlé").as_bytes())
        .unwrap();
    for _ in 0..200 {
        if !queue.lock().unwrap().operations().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    {
        let mut queue = queue.lock().unwrap();
        let op = queue.operations().unwrap()[0].clone();
        queue.remove_acknowledged(&op, &Ack::Recorded).unwrap();
    }
    for _ in 0..200 {
        if !sent_items(&events.lock().unwrap()).is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    tail.stop();
    assert_eq!(
        sent_items(&events.lock().unwrap()),
        [(LineKind::Award, Some("Zoé Brûlé".into()))]
    );
}
