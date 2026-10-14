use super::*;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::AtomicUsize;

/// A fresh, empty folder for one test.
fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-queue-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn files_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// An award as in ADDON-SPEC 4.3, with its own id.
fn award(id: &str) -> Value {
    json!({ "type": "award", "id": id, "formatVersion": 1, "raid": "cm9xr2k4a0001",
        "env": "staging", "boss": "lb_lucifron", "item": 16805, "char": "Ana Vale",
        "user": "u_ana", "method": "HR", "roll": 91, "sender": "Rae Quill", "at": 1792001200 })
}

fn ids(queue: &Queue) -> Vec<String> {
    queue
        .operations()
        .unwrap()
        .iter()
        .map(|op| op["id"].as_str().unwrap().to_string())
        .collect()
}

/// A queue at `path` holding `ids`, in that order.
fn queue_with(path: &Path, ids: &[&str]) -> Queue {
    let mut queue = Queue::open(path);
    for id in ids {
        assert_eq!(queue.add(award(id)).unwrap(), Added::Queued);
    }
    queue
}

#[test]
fn added_entries_come_back_in_order_also_after_a_restart() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    let queue = queue_with(&path, &["aw-3333aaaa", "aw-1111aaaa", "aw-2222aaaa"]);
    let order = ["aw-3333aaaa", "aw-1111aaaa", "aw-2222aaaa"];
    assert_eq!(ids(&queue), order);
    drop(queue);

    let mut reopened = Queue::open(&path);
    assert_eq!(reopened.damage(), None);
    assert_eq!(ids(&reopened), order);
    assert_eq!(reopened.operations().unwrap()[0], award("aw-3333aaaa"));

    reopened.add(award("aw-4444aaaa")).unwrap();
    assert_eq!(
        ids(&Queue::open(&path)),
        ["aw-3333aaaa", "aw-1111aaaa", "aw-2222aaaa", "aw-4444aaaa"]
    );
    assert_eq!(files_in(&dir), [FILE_NAME]);
}

#[test]
fn operations_are_kept_whole() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    let replace = json!({ "type": "replace", "id": "rp-9c4v6t0e", "formatVersion": 1,
        "raid": "cm9xr2k4a0001", "env": "staging", "key": "aw-7f3k2m9q", "at": 1792001300,
        "award": award("aw-2h6j9z4b") });
    let note = json!({ "type": "void", "id": "vd-5b2n8c1w", "formatVersion": 1,
        "raid": "cm9xr2k4a0001", "env": "staging", "key": "aw-7f3k2m9q",
        "reason": "Zoé's \"wrong\" winner \u{1F600}", "at": 1792001260 });
    let mut queue = Queue::open(&path);
    queue.add(replace.clone()).unwrap();
    queue.add(note.clone()).unwrap();
    assert_eq!(Queue::open(&path).operations().unwrap(), [replace, note]);
}

#[test]
fn removing_an_acknowledged_entry_removes_only_that_entry() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    let mut queue = queue_with(&path, &["aw-1111aaaa", "aw-2222aaaa", "aw-3333aaaa"]);

    assert!(queue
        .remove_acknowledged("aw-2222aaaa", &Ack::Recorded)
        .unwrap());
    assert_eq!(ids(&queue), ["aw-1111aaaa", "aw-3333aaaa"]);
    assert_eq!(ids(&Queue::open(&path)), ["aw-1111aaaa", "aw-3333aaaa"]);

    // An id that isn't waiting changes nothing, and the file isn't rewritten.
    let before = fs::metadata(&path).unwrap().modified().unwrap();
    assert!(!queue
        .remove_acknowledged("aw-9999aaaa", &Ack::Replayed)
        .unwrap());
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);

    let rejected = Ack::Rejected {
        code: "item-not-in-table".to_string(),
    };
    assert!(queue.remove_acknowledged("aw-3333aaaa", &rejected).unwrap());
    assert!(queue
        .remove_acknowledged("aw-1111aaaa", &Ack::Voided)
        .unwrap());
    assert!(queue.operations().unwrap().is_empty());
    assert!(Queue::open(&path).operations().unwrap().is_empty());
}

#[test]
fn an_entry_never_acknowledged_stays() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    let mut queue = queue_with(&path, &["aw-1111aaaa", "aw-2222aaaa"]);
    queue
        .remove_acknowledged("aw-1111aaaa", &Ack::Recorded)
        .unwrap();
    queue.reload();
    assert_eq!(ids(&queue), ["aw-2222aaaa"]);
    assert_eq!(ids(&Queue::open(&path)), ["aw-2222aaaa"]);
}

#[test]
fn only_the_acknowledging_results_count() {
    assert_eq!(Ack::from_result("recorded", None), Some(Ack::Recorded));
    assert_eq!(Ack::from_result("replayed", None), Some(Ack::Replayed));
    assert_eq!(Ack::from_result("voided", None), Some(Ack::Voided));
    assert_eq!(
        Ack::from_result("rejected", Some("key-conflict")),
        Some(Ack::Rejected {
            code: "key-conflict".to_string()
        })
    );
    assert_eq!(Ack::from_result("rejected", None), None);
    assert_eq!(Ack::from_result("rejected", Some("")), None);
    assert_eq!(Ack::from_result("pending", None), None);
    assert_eq!(Ack::from_result("Recorded", None), None);
    assert_eq!(Ack::from_result("", None), None);
}

#[test]
fn an_id_already_waiting_is_not_added_again() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    let mut queue = queue_with(&path, &["aw-1111aaaa"]);
    let mut changed = award("aw-1111aaaa");
    changed["char"] = json!("Bo Marsh");
    assert_eq!(queue.add(changed).unwrap(), Added::AlreadyWaiting);
    assert_eq!(queue.operations().unwrap(), [award("aw-1111aaaa")]);
    assert_eq!(
        Queue::open(&path).operations().unwrap(),
        [award("aw-1111aaaa")]
    );
}

#[test]
fn an_operation_without_a_valid_id_is_refused() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    let mut queue = Queue::open(&path);
    let bad = [
        json!("aw-1111aaaa"),
        json!(["aw-1111aaaa"]),
        json!({ "type": "award" }),
        json!({ "id": 12345678 }),
        json!({ "id": "short" }),
        json!({ "id": "a".repeat(129) }),
        json!({ "id": "aw 1111aaaa" }),
        json!({ "id": "aw-1111aaaé" }),
    ];
    for op in bad {
        let err = queue.add(op.clone()).unwrap_err();
        assert!(
            matches!(err, QueueError::BadOperation { .. }),
            "{op}: {err:?}"
        );
    }
    assert_eq!(
        queue.add(json!({ "id": "a".repeat(128) })).unwrap(),
        Added::Queued
    );
    assert_eq!(
        queue.add(json!({ "id": "Ab_-0123" })).unwrap(),
        Added::Queued
    );
    assert_eq!(Queue::open(&path).operations().unwrap().len(), 2);
}

#[test]
fn an_empty_or_missing_file_is_an_empty_queue() {
    let dir = scratch_dir();
    let missing = dir.join(FILE_NAME);
    let queue = Queue::open(&missing);
    assert_eq!(queue.damage(), None);
    assert!(queue.operations().unwrap().is_empty());
    assert!(!missing.exists(), "opening doesn't create the file");

    let empty = dir.join("empty.json");
    fs::write(&empty, "").unwrap();
    let mut queue = Queue::open(&empty);
    assert_eq!(queue.damage(), None);
    assert!(queue.operations().unwrap().is_empty());
    queue.add(award("aw-1111aaaa")).unwrap();
    assert_eq!(ids(&Queue::open(&empty)), ["aw-1111aaaa"]);
}

/// Runs `change` on a queue at `path`, stopping it at `step` the way a crash would:
/// nothing after the step runs, not even cleanup.
fn crash_at(path: &Path, step: Step, change: impl FnOnce(&mut Queue, &mut dyn FnMut(Step))) {
    struct Crashed;
    let mut queue = Queue::open(path);
    let mut at = |s: Step| {
        if s == step {
            panic::resume_unwind(Box::new(Crashed));
        }
    };
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| change(&mut queue, &mut at)));
    assert!(
        outcome.is_err_and(|e| e.is::<Crashed>()),
        "{step:?} was never reached"
    );
}

#[test]
fn a_crash_at_any_step_of_a_write_leaves_the_old_or_the_new_list() {
    let old = ["aw-1111aaaa", "aw-2222aaaa"];
    let steps = [
        (Step::TempCreated, false),
        (Step::HalfWritten, false),
        (Step::Written, false),
        (Step::Swapped, true),
    ];
    for (step, swapped) in steps {
        // An add.
        let dir = scratch_dir();
        let path = dir.join(FILE_NAME);
        drop(queue_with(&path, &old));
        crash_at(&path, step, |queue, at| {
            let _ = queue.add_with(award("aw-3333aaaa"), at);
        });
        let mut after = Queue::open(&path);
        assert_eq!(after.damage(), None, "add, {step:?}");
        let expected: &[&str] = if swapped {
            &["aw-1111aaaa", "aw-2222aaaa", "aw-3333aaaa"]
        } else {
            &old
        };
        assert_eq!(ids(&after), expected, "add, {step:?}");
        // A leftover temporary file doesn't get in the way of the next write.
        after.add(award("aw-4444aaaa")).unwrap();
        assert_eq!(ids(&Queue::open(&path)).last().unwrap(), "aw-4444aaaa");

        // A remove.
        let dir = scratch_dir();
        let path = dir.join(FILE_NAME);
        drop(queue_with(&path, &old));
        crash_at(&path, step, |queue, at| {
            let _ = queue.remove_with("aw-1111aaaa", at);
        });
        let after = Queue::open(&path);
        assert_eq!(after.damage(), None, "remove, {step:?}");
        let expected: &[&str] = if swapped { &["aw-2222aaaa"] } else { &old };
        assert_eq!(ids(&after), expected, "remove, {step:?}");
    }
}

#[test]
fn a_crash_during_the_first_write_leaves_no_list_or_a_full_one() {
    for step in [Step::TempCreated, Step::HalfWritten, Step::Written] {
        let dir = scratch_dir();
        let path = dir.join(FILE_NAME);
        crash_at(&path, step, |queue, at| {
            let _ = queue.add_with(award("aw-1111aaaa"), at);
        });
        assert!(!path.exists(), "{step:?}");
        assert!(Queue::open(&path).operations().unwrap().is_empty());
    }
}

#[test]
fn a_failed_write_changes_nothing_and_leaves_no_temp_file() {
    let dir = scratch_dir();
    // The list's path is taken by a folder with something in it, so the swap fails.
    let path = dir.join("blocked");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("keep"), "x").unwrap();
    let mut queue = Queue {
        path: path.clone(),
        state: Ok(vec![award("aw-1111aaaa")]),
    };
    let err = queue.add(award("aw-2222aaaa")).unwrap_err();
    assert!(matches!(err, QueueError::Io(_)), "{err:?}");
    assert!(err
        .to_string()
        .starts_with("The waiting list couldn't be saved"));
    assert_eq!(ids(&queue), ["aw-1111aaaa"]);
    assert!(queue
        .remove_acknowledged("aw-1111aaaa", &Ack::Recorded)
        .is_err());
    assert_eq!(ids(&queue), ["aw-1111aaaa"]);
    assert_eq!(files_in(&dir), ["blocked"]);
    assert_eq!(files_in(&path), ["keep"]);
}

#[test]
fn a_damaged_file_is_reported_left_as_it_is_and_refuses_changes() {
    let good = serde_json::to_vec(&json!({
        "formatVersion": 1, "operations": [award("aw-1111aaaa")] }))
    .unwrap();
    let cases: Vec<(&str, Vec<u8>)> = vec![
        ("cut off", good[..good.len() - 5].to_vec()),
        ("garbage", vec![0xff, 0xfe, 0x00, 0x13, 0x37]),
        ("not JSON", b"hello".to_vec()),
        ("only spaces", b"  \n".to_vec()),
        ("a bare list", br#"[{"id":"aw-1111aaaa"}]"#.to_vec()),
        ("null", b"null".to_vec()),
        ("no version", br#"{"operations":[]}"#.to_vec()),
        (
            "newer version",
            br#"{"formatVersion":2,"operations":[]}"#.to_vec(),
        ),
        ("no operations", br#"{"formatVersion":1}"#.to_vec()),
        (
            "operations not a list",
            br#"{"formatVersion":1,"operations":{}}"#.to_vec(),
        ),
        (
            "entry not an object",
            br#"{"formatVersion":1,"operations":[1]}"#.to_vec(),
        ),
        (
            "entry without id",
            br#"{"formatVersion":1,"operations":[{"type":"award"}]}"#.to_vec(),
        ),
        (
            "bad id",
            br#"{"formatVersion":1,"operations":[{"id":"x"}]}"#.to_vec(),
        ),
        (
            "repeated id",
            br#"{"formatVersion":1,"operations":[{"id":"aw-1111aaaa"},{"id":"aw-1111aaaa"}]}"#
                .to_vec(),
        ),
    ];
    for (name, bytes) in cases {
        let dir = scratch_dir();
        let path = dir.join(FILE_NAME);
        fs::write(&path, &bytes).unwrap();
        let mut queue = Queue::open(&path);
        assert!(queue.damage().is_some(), "{name}");
        let read = queue.operations().unwrap_err();
        assert!(
            matches!(read, QueueError::Damaged { .. }),
            "{name}: {read:?}"
        );
        let add = queue.add(award("aw-2222aaaa")).unwrap_err();
        assert!(matches!(add, QueueError::Damaged { .. }), "{name}: {add:?}");
        assert!(add.to_string().ends_with("Nothing was deleted."), "{name}");
        let remove = queue
            .remove_acknowledged("aw-1111aaaa", &Ack::Recorded)
            .unwrap_err();
        assert!(matches!(remove, QueueError::Damaged { .. }), "{name}");
        queue.reload();
        assert!(queue.damage().is_some(), "{name}");
        assert_eq!(fs::read(&path).unwrap(), bytes, "{name}: the file changed");
        assert_eq!(files_in(&dir), [FILE_NAME], "{name}");
    }
}

#[test]
fn an_unreadable_file_is_damaged_until_it_is_fixed() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    fs::create_dir(&path).unwrap();
    let mut queue = Queue::open(&path);
    assert!(queue.damage().unwrap().starts_with("it can't be read"));
    assert!(queue.add(award("aw-1111aaaa")).is_err());
    assert!(path.is_dir());

    fs::remove_dir(&path).unwrap();
    fs::write(
        &path,
        br#"{"formatVersion":1,"operations":[{"id":"aw-1111aaaa"}]}"#,
    )
    .unwrap();
    queue.reload();
    assert_eq!(queue.damage(), None);
    assert_eq!(ids(&queue), ["aw-1111aaaa"]);
    assert_eq!(queue.add(award("aw-2222aaaa")).unwrap(), Added::Queued);
}
