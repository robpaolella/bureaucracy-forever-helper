use super::watcher::{Event, Timing, Tracker, Watcher};
use super::*;
use crate::queue::FILE_NAME as QUEUE_FILE;
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

const FIXTURE: &str = include_str!("../../tests/fixtures/BureaucracyForever.lua");

fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-savedvars-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn read(text: &str) -> Result<Reading, ReadError> {
    read_text(text.as_bytes(), "ACCOUNT", 1_792_005_000)
}

fn refused(text: &str) -> String {
    match read(text) {
        Err(e @ ReadError::Refused { .. }) => e.to_string(),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

fn op_ids(reading: &Reading, raid: &str) -> Vec<String> {
    reading.operations[raid]
        .iter()
        .map(|o| o["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn the_add_ons_saved_file_reads_into_the_specs_records() {
    let reading = read(FIXTURE).unwrap();
    assert_eq!(reading.account, "ACCOUNT");
    assert_eq!(reading.saved_at, 1_792_005_000);
    assert_eq!(
        reading.operations.keys().collect::<Vec<_>>(),
        ["cm9xr2k4a0001"]
    );
    assert_eq!(
        op_ids(&reading, "cm9xr2k4a0001"),
        ["aw-1111aaaa", "cf-2222bbbb", "vd-3333cccc"]
    );
    assert_eq!(
        reading.operations["cm9xr2k4a0001"][0],
        json!({ "type": "award", "id": "aw-1111aaaa", "formatVersion": 1,
            "raid": "cm9xr2k4a0001", "env": "staging", "boss": "lb_lucifron", "item": 16805,
            "char": "Ana Vale", "method": "HR", "roll": 91, "sender": "Rae Quill",
            "at": 1792001200 })
    );
    // Escapes read back as the text the add-on saved.
    assert_eq!(
        reading.operations["cm9xr2k4a0001"][2]["reason"],
        "Wrong \"player\"\nsee chat"
    );
    assert_eq!(reading.awards["cm9xr2k4a0001"][0]["key"], "aw-1111aaaa");
    assert_eq!(reading.awards["cm9xr2k4a0001"][0]["voided"], true);
    assert_eq!(reading.no_raid.len(), 1);
    assert_eq!(reading.no_raid_ops.len(), 1);
}

#[test]
fn the_file_time_and_the_selection_fields_are_passed_on() {
    let reading = read(FIXTURE).unwrap();
    assert_eq!(reading.saved_at, 1_792_005_000);
    assert_eq!(
        reading.selection,
        Selection {
            active_raid: Some("cm9xr2k4a0001".into()),
            active_by: Some("chosen".into()),
            chosen_by: Some("Rae Quill".into()),
        }
    );
    let none = read("BureaucracyForeverDB = {}").unwrap();
    assert_eq!(none.selection, Selection::default());
}

#[test]
fn pending_site_voids_are_ignored_but_still_subject_to_parser_limits() {
    let voids = r#"
        BureaucracyForeverDB = {
            ["siteVoids"] = {
                ["aw-1111aaaa"] = { ["raid"] = "r1", ["env"] = "live", ["at"] = 123 },
                ["aw-2222bbbb"] = { ["raid"] = "r2", ["env"] = "live", ["reason"] = "pending", ["at"] = 456 },
            },
            ["activeRaid"] = "r1",
            ["ops"] = { ["r1"] = { { ["id"] = "aw-1111aaaa" } } },
            ["awards"] = { ["r1"] = { { ["key"] = "aw-1111aaaa" } } },
            ["noRaid"] = { { ["id"] = "cf-2222bbbb" } },
            ["noRaidOps"] = { { ["id"] = "vd-3333cccc" } },
        }
    "#;
    let with_voids = read(voids).unwrap();
    let stripped_voids = voids.replace(
        r#"["siteVoids"] = {
                ["aw-1111aaaa"] = { ["raid"] = "r1", ["env"] = "live", ["at"] = 123 },
                ["aw-2222bbbb"] = { ["raid"] = "r2", ["env"] = "live", ["reason"] = "pending", ["at"] = 456 },
            },"#,
        "",
    );
    assert_ne!(voids, stripped_voids, "comparison must remove siteVoids");
    let without_voids = read(&stripped_voids).unwrap();
    assert_eq!(with_voids, without_voids);

    for reading in [&with_voids, &without_voids] {
        let dir = scratch_dir();
        let queue = open_queue(&dir);
        let result = hand_off(&queue, reading);
        assert_eq!(result.queued, 1);
        assert_eq!(waiting_ids(&queue), ["aw-1111aaaa"]);
    }

    assert!(read(r#"BureaucracyForeverDB = { ["siteVoids"] = {} }"#).is_ok());
    let too_long = format!(
        r#"BureaucracyForeverDB = {{ ["siteVoids"] = {{ ["k"] = {{ ["reason"] = "{}" }} }} }}"#,
        "x".repeat(1001)
    );
    assert!(refused(&too_long).contains("longer than"));
    let too_deep = format!(
        r#"BureaucracyForeverDB = {{ ["siteVoids"] = {{ ["k"] = {{ ["x"] = {} }} }} }}"#,
        "{".repeat(6) + &"}".repeat(6)
    );
    assert!(refused(&too_deep).contains("deeper"));
}

#[test]
fn a_file_missing_any_of_the_four_lists_reads_it_as_empty() {
    let reading = read("BureaucracyForeverDB = {\n}\n").unwrap();
    assert!(reading.operations.is_empty() && reading.awards.is_empty());
    assert!(reading.no_raid.is_empty() && reading.no_raid_ops.is_empty());
    let only_ops =
        read(r#"BureaucracyForeverDB = { ["ops"] = { ["r1"] = { { ["id"] = "aw-1111aaaa" } } } }"#)
            .unwrap();
    assert_eq!(op_ids(&only_ops, "r1"), ["aw-1111aaaa"]);
    assert!(only_ops.awards.is_empty() && only_ops.no_raid.is_empty());
    // A raid that saved an empty list reads as an empty list.
    let empty = read(r#"BureaucracyForeverDB = { ["ops"] = { ["r1"] = {} } }"#).unwrap();
    assert!(empty.operations["r1"].is_empty());
}

#[test]
fn code_is_refused_and_nothing_runs() {
    for text in [
        "os.execute(\"echo hi\")\nBureaucracyForeverDB = {}",
        "BureaucracyForeverDB = {}\nos.execute(\"echo hi\")",
        "BureaucracyForeverDB = os.execute(\"echo hi\")",
        "BureaucracyForeverDB = { [\"a\"] = os.execute(\"echo hi\") }",
        "BureaucracyForeverDB = { [\"a\"] = function() end }",
        "BureaucracyForeverDB = { [\"a\"] = (1) }",
        "BureaucracyForeverDB = { [\"a\"] = \"x\" .. \"y\" }",
        "for i = 1, 10 do end\nBureaucracyForeverDB = {}",
        "BureaucracyForeverDB = { [\"a\"] = loadstring(\"x\")() }",
        "BureaucracyForeverDB = { x }",
        "BureaucracyForeverDB = { [os.time()] = 1 }",
        "other = {}",
    ] {
        let message = refused(text);
        assert!(
            message.contains("Nothing in it was used"),
            "{text}: {message}"
        );
        assert!(message.contains("line "), "{text}: {message}");
    }
    let message = refused("BureaucracyForeverDB = {}\nos.execute(\"echo hi\")");
    assert!(
        message.contains("\"os\"") && message.contains("line 2"),
        "{message}"
    );
}

#[test]
fn a_file_that_would_create_a_marker_if_run_never_does() {
    let dir = scratch_dir();
    let marker = dir.join("ran");
    let text = format!(
        "BureaucracyForeverDB = {{ x = io.open(\"{}\", \"w\") }}",
        marker.display()
    );
    assert!(read(&text).is_err());
    assert!(!marker.exists());
}

#[test]
fn an_empty_file_a_syntax_error_and_a_missing_file_end_in_plain_words() {
    assert!(matches!(read(""), Err(ReadError::Empty)));
    assert!(matches!(read("  \n"), Err(ReadError::Empty)));
    assert!(matches!(
        read("-- nothing\n"),
        Err(ReadError::Refused { .. })
    ));
    for text in [
        "BureaucracyForeverDB = {",
        "BureaucracyForeverDB = { [\"a\"] = }",
        "BureaucracyForeverDB = { [\"a\"] = \"unclosed }",
        "BureaucracyForeverDB = { [\"a\"] = 1 [\"b\"] = 2 }",
        "BureaucracyForeverDB = { [\"a\"] = 1e999 }",
        "BureaucracyForeverDB = { [\"a\"] = \"\\q\" }",
        "BureaucracyForeverDB = { [1] = 1, [1] = 2 }",
        "BureaucracyForeverDB = \"text\"",
        "BureaucracyForeverDB = { \u{1} }",
    ] {
        let message = read(text).unwrap_err().to_string();
        assert!(
            message.starts_with("The add-on’s saved file"),
            "{text}: {message}"
        );
    }
    let dir = scratch_dir();
    let missing = read_file(&dir.join("nope.lua"), "A").unwrap_err();
    assert!(matches!(missing, ReadError::Io(_)));
    assert!(missing
        .to_string()
        .starts_with("The add-on’s saved file couldn’t be opened"));
}

#[test]
fn the_depth_and_string_limits_apply() {
    let deep = format!("BureaucracyForeverDB = {}{}", "{".repeat(9), "}".repeat(9));
    assert!(refused(&deep).contains("deeper"));
    let fine = format!("BureaucracyForeverDB = {}{}", "{".repeat(8), "}".repeat(8));
    assert!(read(&fine).is_ok());
    let long = format!("BureaucracyForeverDB = {{ a = \"{}\" }}", "x".repeat(1001));
    assert!(refused(&long).contains("longer than"));
    let hostile = format!("BureaucracyForeverDB = {}", "{".repeat(100_000));
    assert!(read(&hostile).is_err());
}

#[test]
fn an_oversized_file_is_refused_without_being_read() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_FILE_BYTES + 1).unwrap();
    let err = read_file(&path, "A").unwrap_err();
    assert!(matches!(err, ReadError::TooBig { .. }));
    assert!(err.to_string().contains("too large"));
}

#[test]
fn a_wrong_layout_is_refused_whole() {
    for text in [
        r#"BureaucracyForeverDB = { ["ops"] = "x" }"#,
        r#"BureaucracyForeverDB = { ["ops"] = { ["r1"] = 5 } }"#,
        r#"BureaucracyForeverDB = { ["ops"] = { ["r1"] = { 1, 2 } } }"#,
        r#"BureaucracyForeverDB = { ["noRaidOps"] = { ["a"] = 1 } }"#,
        r#"BureaucracyForeverDB = { ["activeBy"] = 3 }"#,
    ] {
        assert!(
            matches!(read(text), Err(ReadError::Layout { .. })),
            "{text}"
        );
    }
}

fn open_queue(dir: &Path) -> Mutex<Queue> {
    Mutex::new(Queue::open(dir.join(QUEUE_FILE)))
}

fn waiting_ids(queue: &Mutex<Queue>) -> Vec<String> {
    let queue = queue.lock().unwrap();
    queue
        .operations()
        .unwrap()
        .iter()
        .map(|o| o["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn only_ops_are_handed_on_in_saved_order_and_no_raid_ops_are_not() {
    let dir = scratch_dir();
    let queue = open_queue(&dir);
    let reading = read(FIXTURE).unwrap();
    let result = hand_off(&queue, &reading);
    assert_eq!(result.queued, 3);
    assert_eq!(result.queue_problem, None);
    assert_eq!(
        waiting_ids(&queue),
        ["aw-1111aaaa", "cf-2222bbbb", "vd-3333cccc"]
    );
}

#[test]
fn reading_the_same_file_twice_queues_each_operation_once() {
    let dir = scratch_dir();
    let queue = open_queue(&dir);
    let first = hand_off(&queue, &read(FIXTURE).unwrap());
    let second = hand_off(&queue, &read(FIXTURE).unwrap());
    assert_eq!((first.queued, first.already_waiting), (3, 0));
    assert_eq!((second.queued, second.already_waiting), (0, 3));
    assert_eq!(waiting_ids(&queue).len(), 3);
}

#[test]
fn an_operation_without_an_id_is_skipped_and_the_rest_go_on() {
    let dir = scratch_dir();
    let queue = open_queue(&dir);
    let reading = read(
        r#"BureaucracyForeverDB = { ["ops"] = { ["r1"] = { { ["type"] = "award" }, { ["id"] = "aw-1111aaaa" } } } }"#,
    )
    .unwrap();
    let result = hand_off(&queue, &reading);
    assert_eq!((result.queued, result.refused), (1, 1));
}

#[test]
fn a_damaged_queue_is_reported_and_nothing_is_lost() {
    let dir = scratch_dir();
    fs::write(dir.join(QUEUE_FILE), "not json").unwrap();
    let queue = open_queue(&dir);
    let result = hand_off(&queue, &read(FIXTURE).unwrap());
    assert_eq!(result.queued, 0);
    assert!(result.queue_problem.unwrap().contains("damaged"));
}

// --- the watcher ---

fn stamp_of(path: &Path) -> super::watcher::Stamp {
    super::watcher::stamp(path).unwrap()
}

#[test]
fn a_changing_file_waits_until_it_has_stopped_changing() {
    let dir = scratch_dir();
    let path = dir.join("f");
    fs::write(&path, "a").unwrap();
    let settle = Duration::from_secs(3);
    let t0 = Instant::now();
    let mut tracker = Tracker::default();
    assert!(!tracker.ready(t0, Some(stamp_of(&path)), settle));
    assert!(!tracker.ready(t0 + Duration::from_secs(2), Some(stamp_of(&path)), settle));
    fs::write(&path, "ab").unwrap(); // changed again: the wait starts over
    assert!(!tracker.ready(t0 + Duration::from_secs(4), Some(stamp_of(&path)), settle));
    assert!(!tracker.ready(t0 + Duration::from_secs(6), Some(stamp_of(&path)), settle));
    assert!(tracker.ready(t0 + Duration::from_secs(7), Some(stamp_of(&path)), settle));
    // A file that vanishes is forgotten, and read again when it comes back.
    assert!(!tracker.ready(t0 + Duration::from_secs(8), None, settle));
}

const FAST: Timing = Timing {
    poll: Duration::from_millis(10),
    settle: Duration::from_millis(150),
};

fn version_folder_with_file(text: &str) -> (PathBuf, PathBuf) {
    let folder = scratch_dir();
    let saved = folder.join("WTF/Account/ACCT/SavedVariables");
    fs::create_dir_all(&saved).unwrap();
    let file = saved.join("BureaucracyForever.lua");
    fs::write(&file, text).unwrap();
    (folder, file)
}

struct Running {
    watcher: Watcher,
    events: mpsc::Receiver<Event>,
    queue: Arc<Mutex<Queue>>,
}

fn run(folder: &Path, queue_dir: &Path) -> Running {
    let queue = Arc::new(open_queue(queue_dir).into_inner().unwrap().into());
    let (tx, events) = mpsc::channel();
    let tx = Mutex::new(tx);
    let folder = folder.to_path_buf();
    let watcher = Watcher::start(
        move || Some(folder.clone()),
        Arc::clone(&queue),
        FAST,
        move |e| {
            let _ = tx.lock().unwrap().send(e);
        },
    );
    Running {
        watcher,
        events,
        queue,
    }
}

impl Running {
    fn next(&self) -> Event {
        self.events
            .recv_timeout(Duration::from_secs(5))
            .expect("an event")
    }

    fn finish(self) {
        self.watcher.stop();
        self.watcher.join();
    }
}

#[test]
fn a_file_still_being_written_is_not_read_until_it_stops_changing() {
    let (folder, file) = version_folder_with_file("BureaucracyForeverDB = {");
    let r = run(&folder, &scratch_dir());
    // Keep rewriting for longer than the settle time: every state is half-written.
    let start = Instant::now();
    let mut n = 0;
    while start.elapsed() < Duration::from_millis(600) {
        n += 1;
        fs::write(
            &file,
            format!("BureaucracyForeverDB = {{ {}", " ".repeat(n)),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(r.events.try_recv().is_err(), "read while still changing");
    fs::write(&file, FIXTURE).unwrap();
    match r.next() {
        Event::Read(reading) => assert_eq!(reading.operations["cm9xr2k4a0001"].len(), 3),
        other => panic!("{other:?}"),
    }
    r.finish();
}

#[test]
fn a_bad_file_is_reported_once_and_hands_nothing_on() {
    let (folder, _) = version_folder_with_file("os.execute('x')");
    let r = run(&folder, &scratch_dir());
    match r.next() {
        Event::Failed { account, message } => {
            assert_eq!(account, "ACCT");
            assert!(message.contains("code"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    std::thread::sleep(Duration::from_millis(500));
    assert!(r.events.try_recv().is_err(), "reported again unchanged");
    assert!(r.queue.lock().unwrap().operations().unwrap().is_empty());
    r.finish();
}

#[test]
fn several_accounts_are_each_read_and_a_missing_folder_does_nothing() {
    let (folder, _) = version_folder_with_file(FIXTURE);
    let second = folder.join("WTF/Account/OTHER/SavedVariables");
    fs::create_dir_all(&second).unwrap();
    fs::write(
        second.join("BureaucracyForever.lua"),
        "BureaucracyForeverDB = {}",
    )
    .unwrap();
    let r = run(&folder, &scratch_dir());
    let mut accounts = vec![];
    for _ in 0..2 {
        if let Event::Read(reading) = r.next() {
            accounts.push(reading.account);
        }
    }
    accounts.sort();
    assert_eq!(accounts, ["ACCT", "OTHER"]);
    assert_eq!(r.watcher.latest().len(), 2);
    r.finish();

    let empty = scratch_dir();
    let r = run(&empty, &scratch_dir());
    std::thread::sleep(Duration::from_millis(300));
    assert!(r.events.try_recv().is_err());
    r.finish();
}

#[test]
fn a_changed_file_is_read_again_even_while_the_queue_is_busy() {
    let (folder, file) = version_folder_with_file(FIXTURE);
    let r = run(&folder, &scratch_dir());
    // An upload in progress holds the queue's lock.
    let busy = r.queue.lock().unwrap();
    let Event::Read(first) = r.next() else {
        panic!("expected a reading")
    };
    assert_eq!(first.operations["cm9xr2k4a0001"].len(), 3);
    let more = FIXTURE.replace(
        "\t[\"noRaid\"] = {",
        "\t[\"ops2\"] = {},\n\t[\"noRaid\"] = {",
    );
    let more = more.replacen(
        "\t\t\t}, -- [3]",
        "\t\t\t}, -- [3]\n\t\t\t{ [\"type\"] = \"void\", [\"id\"] = \"vd-5555eeee\", [\"key\"] = \"aw-1111aaaa\", [\"at\"] = 1792009999 },",
        1,
    );
    fs::write(&file, more).unwrap();
    // Noticed and read while the lock is still held.
    let Event::Read(second) = r.next() else {
        panic!("expected a second reading")
    };
    assert_eq!(second.operations["cm9xr2k4a0001"].len(), 4);
    assert!(r.watcher.latest()[0].operations["cm9xr2k4a0001"].len() == 4);
    drop(busy);
    let mut handed = 0;
    while handed < 2 {
        if let Event::HandedOn { .. } = r.next() {
            handed += 1;
        }
    }
    assert_eq!(waiting_ids(&r.queue).len(), 4);
    r.finish();
}
