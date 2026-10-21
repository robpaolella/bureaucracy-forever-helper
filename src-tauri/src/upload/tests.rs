use super::*;
use crate::queue::FILE_NAME;
use std::cell::RefCell;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const KEY: &str = "bfh_q7Xk2mP9vR4tW8yZ1aB3cD5eF6gH0iJ-kL_nO2pQ4sA";
const OTHER_KEY: &str = "bfh_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-upload-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn key(text: &str) -> HelperKey {
    HelperKey::parse(text).unwrap()
}

/// An award as in ADDON-SPEC 4.3, with its own id, for `env`.
fn award(id: &str, env: &str) -> Value {
    json!({ "type": "award", "id": id, "formatVersion": 1, "raid": "cm9xr2k4a0001",
        "env": env, "boss": "lb_lucifron", "item": 16805, "char": "Ana Vale",
        "method": "HR", "roll": 91, "sender": "Rae Quill", "at": 1792001200 })
}

fn practice(id: &str, env: &str) -> Value {
    let mut op = award(id, env);
    op["practice"] = json!(true);
    op
}

/// A queue in a fresh folder holding `ops`, and its file's path. The file is written in
/// one go, as adding a long backlog one by one is slow.
fn queue_with(ops: &[Value]) -> (Queue, PathBuf) {
    let path = scratch_dir().join(FILE_NAME);
    let file = json!({ "formatVersion": 1, "operations": ops });
    fs::write(&path, file.to_string()).unwrap();
    let queue = Queue::open(&path);
    assert_eq!(queue.operations().unwrap(), ops);
    (queue, path)
}

fn ids(queue: &Queue) -> Vec<String> {
    queue
        .operations()
        .unwrap()
        .iter()
        .map(|op| op["id"].as_str().unwrap().to_string())
        .collect()
}

/// A scripted answer, used before the fake site goes back to answering normally.
enum Script {
    /// No answer at all.
    NoAnswer,
    /// Stores the upload, then the answer is lost, as in a crash or a dropped connection.
    StoreThenLose,
    /// This status, `Retry-After` and body, storing nothing.
    Answer(u16, Option<u64>, Value),
}

/// A site in memory. It applies ADDON-SPEC 4.2's repeat rules: an id already held with the
/// same payload is `replayed`, so a record can only be stored once.
#[derive(Default)]
struct FakeSite {
    script: RefCell<VecDeque<Script>>,
    /// Every request: the site, and the ids it carried.
    requests: RefCell<Vec<(Site, Vec<String>)>>,
    sizes: RefCell<Vec<usize>>,
    /// Stored records by site, in arrival order.
    stored: RefCell<Vec<(Site, Value)>>,
    reject: HashMap<String, String>,
    pending: RefCell<HashSet<String>>,
}

impl FakeSite {
    fn then(&self, script: Script) -> &Self {
        self.script.borrow_mut().push_back(script);
        self
    }

    fn requests(&self) -> usize {
        self.requests.borrow().len()
    }

    fn sent_ids(&self) -> Vec<String> {
        self.requests
            .borrow()
            .iter()
            .flat_map(|(_, ids)| ids.clone())
            .collect()
    }

    fn stored_ids(&self) -> Vec<String> {
        self.stored
            .borrow()
            .iter()
            .map(|(_, op)| op["id"].as_str().unwrap().to_string())
            .collect()
    }

    fn apply(&self, site: Site, ops: &[Value]) -> Value {
        let results: Vec<Value> = ops
            .iter()
            .map(|op| {
                let id = op["id"].as_str().unwrap();
                if let Some(code) = self.reject.get(id) {
                    return json!({ "id": id, "status": "rejected", "code": code });
                }
                if site == Site::Production && op["practice"] == json!(true) {
                    return json!({ "id": id, "status": "rejected", "code": "practice-refused" });
                }
                if self.pending.borrow().contains(id) {
                    return json!({ "id": id, "status": "pending" });
                }
                let mut stored = self.stored.borrow_mut();
                let status = if stored.iter().any(|(s, held)| *s == site && held == op) {
                    "replayed"
                } else {
                    stored.push((site, op.clone()));
                    "recorded"
                };
                json!({ "id": id, "status": status })
            })
            .collect();
        json!({ "results": results })
    }
}

impl Uplink for FakeSite {
    fn post_operations(
        &self,
        site: Site,
        _key: &HelperKey,
        body: &str,
    ) -> Result<UploadReply, String> {
        let body: Value = serde_json::from_str(body).unwrap();
        let ops = body["operations"].as_array().unwrap().clone();
        self.sizes.borrow_mut().push(body.to_string().len());
        self.requests.borrow_mut().push((
            site,
            ops.iter()
                .map(|op| op["id"].as_str().unwrap().to_string())
                .collect(),
        ));
        let ok = |body| UploadReply {
            status: 200,
            retry_after: None,
            body: Some(body),
        };
        match self.script.borrow_mut().pop_front() {
            None => Ok(ok(self.apply(site, &ops))),
            Some(Script::NoAnswer) => Err("the connection failed".into()),
            Some(Script::StoreThenLose) => {
                self.apply(site, &ops);
                Err("the connection failed".into())
            }
            Some(Script::Answer(status, retry_after, body)) => Ok(UploadReply {
                status,
                retry_after,
                body: Some(body),
            }),
        }
    }
}

fn error(code: &str) -> Value {
    json!({ "error": "Some plain sentence.", "code": code })
}

#[test]
fn the_queue_is_sent_in_order_and_removed_once_acknowledged() {
    let (mut queue, _) = queue_with(&[
        award("aw-1111aaaa", "staging"),
        award("aw-2222aaaa", "staging"),
        award("aw-3333aaaa", "staging"),
    ]);
    let site = FakeSite::default();
    let mut up = Uploader::new();
    let wait = up.pass(&mut queue, &site, Site::Staging, &key(KEY), Instant::now());
    assert_eq!(
        site.stored_ids(),
        ["aw-1111aaaa", "aw-2222aaaa", "aw-3333aaaa"]
    );
    assert_eq!(site.requests(), 1);
    assert!(ids(&queue).is_empty());
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
    assert_eq!(wait, IDLE);
}

#[test]
fn a_crash_between_upload_and_acknowledgement_resends_without_recording_twice() {
    let ops = [
        award("aw-1111aaaa", "staging"),
        award("aw-2222aaaa", "staging"),
    ];
    let (mut queue, path) = queue_with(&ops);
    let site = FakeSite::default();
    site.then(Script::StoreThenLose);
    let now = Instant::now();
    Uploader::new().pass(&mut queue, &site, Site::Staging, &key(KEY), now);
    assert_eq!(site.stored_ids().len(), 2);
    assert_eq!(
        ids(&queue).len(),
        2,
        "nothing acknowledged, so nothing removed"
    );

    // The helper restarts: a fresh queue from the file and a fresh uploader.
    drop(queue);
    let mut queue = Queue::open(&path);
    let mut up = Uploader::new();
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now);
    assert_eq!(site.requests(), 2);
    assert_eq!(site.stored_ids(), ["aw-1111aaaa", "aw-2222aaaa"]);
    assert!(ids(&queue).is_empty());
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
}

#[test]
fn offline_entries_stay_queued_and_drain_in_order_when_the_site_is_back() {
    let (mut queue, _) = queue_with(&[
        award("aw-1111aaaa", "staging"),
        award("aw-2222aaaa", "staging"),
    ]);
    let site = FakeSite::default();
    site.then(Script::NoAnswer).then(Script::NoAnswer);
    let mut up = Uploader::new();
    let start = Instant::now();
    let first = up.pass(&mut queue, &site, Site::Staging, &key(KEY), start);
    assert_eq!(up.status(), SyncStatus::Offline { waiting: 2 });
    assert_eq!(ids(&queue).len(), 2);

    // Too soon: nothing is sent.
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        start + first / 2,
    );
    assert_eq!(site.requests(), 1);

    let second = up.pass(&mut queue, &site, Site::Staging, &key(KEY), start + first);
    assert!(second > first, "the wait grows: {first:?} then {second:?}");
    assert_eq!(up.status(), SyncStatus::Offline { waiting: 2 });

    queue.add(award("aw-3333aaaa", "staging")).unwrap();
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        start + first + second,
    );
    assert_eq!(
        site.stored_ids(),
        ["aw-1111aaaa", "aw-2222aaaa", "aw-3333aaaa"]
    );
    assert!(ids(&queue).is_empty());
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
}

#[test]
fn a_server_error_resends_the_same_upload() {
    let (mut queue, _) = queue_with(&[award("aw-1111aaaa", "staging")]);
    let site = FakeSite::default();
    site.then(Script::StoreThenLose)
        .then(Script::Answer(500, None, error("server-error")));
    let mut up = Uploader::new();
    let mut now = Instant::now();
    for _ in 0..3 {
        now += up.pass(&mut queue, &site, Site::Staging, &key(KEY), now);
    }
    assert_eq!(site.sent_ids(), ["aw-1111aaaa"; 3]);
    assert_eq!(site.stored_ids(), ["aw-1111aaaa"]);
    assert!(ids(&queue).is_empty());
}

#[test]
fn a_refused_record_is_reported_once_and_the_rest_still_goes() {
    let (mut queue, _) = queue_with(&[
        award("aw-1111aaaa", "staging"),
        json!({ "type": "replace", "id": "rp-2222aaaa", "formatVersion": 1,
            "raid": "cm9xr2k4a0001", "env": "staging", "key": "aw-1111aaaa", "at": 1,
            "award": award("aw-4444aaaa", "staging") }),
        award("aw-3333aaaa", "staging"),
    ]);
    let site = FakeSite {
        reject: HashMap::from([("rp-2222aaaa".to_string(), "unknown-character".to_string())]),
        ..FakeSite::default()
    };
    let mut up = Uploader::new();
    let now = Instant::now();
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now);
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + IDLE);
    assert_eq!(
        site.sent_ids(),
        ["aw-1111aaaa", "rp-2222aaaa", "aw-3333aaaa"]
    );
    assert_eq!(site.stored_ids(), ["aw-1111aaaa", "aw-3333aaaa"]);
    assert!(ids(&queue).is_empty(), "a rejection is an acknowledgement");
    let refused = Refused {
        id: "rp-2222aaaa".into(),
        item: Some(16805),
        character: Some("Ana Vale".into()),
        code: "unknown-character".into(),
    };
    assert_eq!(
        up.status(),
        SyncStatus::Refused {
            records: vec![refused],
            other_site: 0,
        }
    );
    up.clear_refused(&["rp-2222aaaa".to_string()]);
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
    // Cleared for good: a pass with nothing new doesn't bring it back.
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + IDLE * 2);
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
}

#[test]
fn clearing_by_ids_leaves_a_refusal_the_window_has_not_shown() {
    let (mut queue, _) = queue_with(&[
        award("aw-1111aaaa", "staging"),
        award("aw-3333aaaa", "staging"),
    ]);
    let site = FakeSite {
        reject: HashMap::from([
            ("aw-1111aaaa".to_string(), "unknown-character".to_string()),
            ("aw-3333aaaa".to_string(), "unknown-character".to_string()),
        ]),
        ..FakeSite::default()
    };
    let mut up = Uploader::new();
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), Instant::now());
    // The window showed only the first when it asked for it to be cleared.
    up.clear_refused(&["aw-1111aaaa".to_string()]);
    match up.status() {
        SyncStatus::Refused { records, .. } => {
            let ids: Vec<_> = records.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(ids, ["aw-3333aaaa"]);
        }
        other => panic!("expected a refusal, got {other:?}"),
    }
    up.clear_refused(&["aw-3333aaaa".to_string()]);
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
}

#[test]
fn a_pending_record_stays_queued_and_is_retried_spaced_out() {
    let (mut queue, _) = queue_with(&[
        json!({ "type": "void", "id": "vd-1111aaaa", "formatVersion": 1,
            "raid": "cm9xr2k4a0001", "env": "staging", "key": "aw-9999aaaa", "at": 1 }),
        award("aw-2222aaaa", "staging"),
    ]);
    let site = FakeSite::default();
    site.pending.borrow_mut().insert("vd-1111aaaa".into());
    let mut up = Uploader::new();
    let start = Instant::now();
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), start);
    assert_eq!(ids(&queue), ["vd-1111aaaa"]);
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });

    // Not resent at once.
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        start + Duration::from_secs(1),
    );
    assert_eq!(site.requests(), 1);

    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        start + PENDING.0,
    );
    assert_eq!(site.requests(), 2);
    // Still pending: the next try is further off.
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        start + PENDING.0 * 2,
    );
    assert_eq!(site.requests(), 2);
    site.pending.borrow_mut().clear();
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        start + PENDING.0 * 3,
    );
    assert_eq!(site.requests(), 3);
    assert!(ids(&queue).is_empty());
}

#[test]
fn a_practice_record_is_never_sent_to_production() {
    let (mut queue, _) = queue_with(&[
        practice("aw-1111aaaa", "production"),
        practice("aw-2222aaaa", "staging"),
        award("aw-3333aaaa", "production"),
    ]);
    let site = FakeSite::default();
    let mut up = Uploader::new();
    let now = Instant::now();
    up.pass(&mut queue, &site, Site::Production, &key(KEY), now);
    up.pass(&mut queue, &site, Site::Production, &key(KEY), now + IDLE);
    assert_eq!(site.sent_ids(), ["aw-3333aaaa"]);
    assert_eq!(
        ids(&queue),
        ["aw-1111aaaa", "aw-2222aaaa"],
        "kept for staging"
    );
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 2 });

    // On staging the staging record goes; the production one stays for production,
    // where it can never go either.
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + IDLE * 2);
    let sent = site.requests.borrow().clone();
    assert_eq!(
        sent,
        [
            (Site::Production, vec!["aw-3333aaaa".to_string()]),
            (Site::Staging, vec!["aw-2222aaaa".to_string()]),
        ]
    );
    assert_eq!(ids(&queue), ["aw-1111aaaa"]);
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 1 });
}

#[test]
fn a_replacement_carrying_a_practice_award_is_never_sent_to_production() {
    let (mut queue, _) = queue_with(&[json!({ "type": "replace", "id": "rp-1111aaaa",
        "formatVersion": 1, "raid": "cm9xr2k4a0001", "env": "production",
        "key": "aw-9999aaaa", "at": 1, "award": practice("aw-2222aaaa", "production") })]);
    let site = FakeSite::default();
    Uploader::new().pass(
        &mut queue,
        &site,
        Site::Production,
        &key(KEY),
        Instant::now(),
    );
    assert_eq!(site.requests(), 0);
    assert_eq!(ids(&queue), ["rp-1111aaaa"]);
}

#[test]
fn requests_are_timed_from_when_they_go() {
    struct Slow<'a>(&'a FakeSite);
    impl Uplink for Slow<'_> {
        fn post_operations(
            &self,
            site: Site,
            key: &HelperKey,
            body: &str,
        ) -> Result<UploadReply, String> {
            std::thread::sleep(Duration::from_millis(20));
            self.0.post_operations(site, key, body)
        }
    }
    let ops: Vec<Value> = (0..31)
        .map(|n| award(&format!("aw-{n:04}aaaa"), "staging"))
        .collect();
    let (mut queue, _) = queue_with(&ops);
    let site = FakeSite::default();
    let mut up = Uploader::new();
    up.batch_limit = Some(1);
    let wait = up.pass(
        &mut queue,
        &site_slow(&site),
        Site::Staging,
        &key(KEY),
        Instant::now(),
    );
    assert_eq!(site.requests(), 30);
    assert!(wait < RATE_WINDOW, "the first request went first: {wait:?}");

    fn site_slow(site: &FakeSite) -> Slow<'_> {
        Slow(site)
    }
}

/// Records for the other site wait untouched in both directions, never sent and never
/// refused, and the status counts them.
#[test]
fn a_record_for_the_other_site_waits_untouched_and_is_counted() {
    for (site_now, other) in [(Site::Production, "staging"), (Site::Staging, "production")] {
        let mine = if other == "staging" {
            "production"
        } else {
            "staging"
        };
        let waiting = [award("aw-1111aaaa", other), award("aw-3333aaaa", other)];
        let (mut queue, path) = queue_with(&[
            waiting[0].clone(),
            award("aw-2222aaaa", mine),
            waiting[1].clone(),
        ]);
        let site = FakeSite::default();
        site.then(Script::NoAnswer);
        let mut up = Uploader::new();
        let now = Instant::now();
        let wait = up.pass(&mut queue, &site, site_now, &key(KEY), now);
        assert_eq!(up.status(), SyncStatus::Offline { waiting: 1 });
        up.pass(&mut queue, &site, site_now, &key(KEY), now + wait);
        assert_eq!(site.sent_ids(), ["aw-2222aaaa", "aw-2222aaaa"]);
        assert_eq!(queue.operations().unwrap(), waiting);
        assert_eq!(Queue::open(&path).operations().unwrap(), waiting);
        assert_eq!(up.status(), SyncStatus::Idle { other_site: 2 });
    }
}

#[test]
fn a_stopped_key_keeps_the_queue_and_sends_nothing_until_it_changes() {
    for status in [401, 403] {
        let (mut queue, _) = queue_with(&[award("aw-1111aaaa", "staging")]);
        let site = FakeSite::default();
        site.then(Script::Answer(status, None, error("bad-key")));
        let mut up = Uploader::new();
        let now = Instant::now();
        up.pass(&mut queue, &site, Site::Staging, &key(KEY), now);
        assert_eq!(up.status(), SyncStatus::KeyStopped);
        for later in 1..4 {
            up.pass(
                &mut queue,
                &site,
                Site::Staging,
                &key(KEY),
                now + IDLE * later,
            );
        }
        assert_eq!(site.requests(), 1);
        assert_eq!(ids(&queue), ["aw-1111aaaa"]);
        assert_eq!(up.status(), SyncStatus::KeyStopped);

        up.pass(
            &mut queue,
            &site,
            Site::Staging,
            &key(OTHER_KEY),
            now + IDLE * 5,
        );
        assert_eq!(site.requests(), 2);
        assert!(ids(&queue).is_empty());
        assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
    }
}

#[test]
fn a_switched_off_site_keeps_the_queue_and_is_asked_again_later() {
    let (mut queue, _) = queue_with(&[award("aw-1111aaaa", "staging")]);
    let site = FakeSite::default();
    site.then(Script::Answer(503, None, error("helper-off")));
    let mut up = Uploader::new();
    let now = Instant::now();
    let wait = up.pass(&mut queue, &site, Site::Staging, &key(KEY), now);
    assert_eq!(wait, SITE_OFF);
    assert_eq!(up.status(), SyncStatus::SiteOff);
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + IDLE);
    assert_eq!(site.requests(), 1);
    assert_eq!(ids(&queue), ["aw-1111aaaa"]);
    assert_eq!(up.status(), SyncStatus::SiteOff);

    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + SITE_OFF);
    assert!(ids(&queue).is_empty());
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
}

#[test]
fn a_damaged_queue_sends_nothing_and_is_not_changed() {
    let path = scratch_dir().join(FILE_NAME);
    fs::write(&path, b"{ not a list").unwrap();
    let mut queue = Queue::open(&path);
    let site = FakeSite::default();
    let mut up = Uploader::new();
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), Instant::now());
    assert_eq!(site.requests(), 0);
    assert_eq!(fs::read(&path).unwrap(), b"{ not a list");
    assert!(matches!(up.status(), SyncStatus::QueueDamaged { .. }));

    // Fixed on disk: it sends again.
    fs::remove_file(&path).unwrap();
    queue.reload();
    queue.add(award("aw-1111aaaa", "staging")).unwrap();
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), Instant::now());
    assert_eq!(site.stored_ids(), ["aw-1111aaaa"]);
    assert_eq!(up.status(), SyncStatus::Idle { other_site: 0 });
}

#[test]
fn too_many_requests_and_too_large_back_off_as_told() {
    let (mut queue, _) = queue_with(&[award("aw-1111aaaa", "staging")]);
    let site = FakeSite::default();
    site.then(Script::Answer(429, Some(42), error("rate-limited")))
        .then(Script::Answer(429, None, error("rate-limited")));
    let mut up = Uploader::new();
    let now = Instant::now();
    assert_eq!(
        up.pass(&mut queue, &site, Site::Staging, &key(KEY), now),
        Duration::from_secs(42)
    );
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        now + Duration::from_secs(41),
    );
    assert_eq!(site.requests(), 1);
    let now = now + Duration::from_secs(42);
    assert_eq!(
        up.pass(&mut queue, &site, Site::Staging, &key(KEY), now),
        BACK_OFF
    );
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + BACK_OFF);
    assert_eq!(site.requests(), 3);
    assert!(ids(&queue).is_empty());
}

#[test]
fn a_413_halves_the_batch_and_backs_off() {
    let ops: Vec<Value> = (0..4)
        .map(|n| award(&format!("aw-{n:04}aaaa"), "staging"))
        .collect();
    let (mut queue, _) = queue_with(&ops);
    let site = FakeSite::default();
    site.then(Script::Answer(413, Some(5), error("too-large")));
    let mut up = Uploader::new();
    let now = Instant::now();
    assert_eq!(
        up.pass(&mut queue, &site, Site::Staging, &key(KEY), now),
        Duration::from_secs(5)
    );
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        now + Duration::from_secs(5),
    );
    let sizes: Vec<usize> = site
        .requests
        .borrow()
        .iter()
        .map(|(_, i)| i.len())
        .collect();
    assert_eq!(sizes, [4, 2, 2]);
    assert!(ids(&queue).is_empty());
}

#[test]
fn a_backlog_over_one_batch_goes_in_several_requests() {
    let ops: Vec<Value> = (0..201)
        .map(|n| award(&format!("aw-{n:04}aaaa"), "staging"))
        .collect();
    let (mut queue, _) = queue_with(&ops);
    let site = FakeSite::default();
    Uploader::new().pass(&mut queue, &site, Site::Staging, &key(KEY), Instant::now());
    let counts: Vec<usize> = site
        .requests
        .borrow()
        .iter()
        .map(|(_, i)| i.len())
        .collect();
    assert_eq!(counts, [100, 100, 1]);
    let expected: Vec<String> = ops
        .iter()
        .map(|o| o["id"].as_str().unwrap().into())
        .collect();
    assert_eq!(site.stored_ids(), expected);
}

#[test]
fn a_batch_stays_under_64_kib_and_an_oversized_record_is_refused_not_dropped() {
    let mut ops: Vec<Value> = (0..60)
        .map(|n| {
            let mut op = award(&format!("aw-{n:04}aaaa"), "staging");
            op["note"] = json!("x".repeat(1500));
            op
        })
        .collect();
    let mut huge = award("aw-hugeaaaa", "staging");
    huge["note"] = json!("x".repeat(70_000));
    ops.insert(10, huge);
    let (mut queue, _) = queue_with(&ops);
    let site = FakeSite::default();
    let mut up = Uploader::new();
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), Instant::now());
    assert!(site.requests() >= 2);
    assert!(site.sizes.borrow().iter().all(|s| *s <= MAX_BODY));
    assert_eq!(site.stored_ids().len(), 60);
    assert_eq!(ids(&queue), ["aw-hugeaaaa"], "kept in the list");
    match up.status() {
        SyncStatus::Refused { records, .. } => {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].code, "too-large");
        }
        other => panic!("{other:?}"),
    }
    // Not retried.
    up.pass(
        &mut queue,
        &site,
        Site::Staging,
        &key(KEY),
        Instant::now() + IDLE,
    );
    assert!(!site.sent_ids().contains(&"aw-hugeaaaa".to_string()));
}

#[test]
fn no_more_than_30_requests_a_minute() {
    let ops: Vec<Value> = (0..31)
        .map(|n| award(&format!("aw-{n:04}aaaa"), "staging"))
        .collect();
    let (mut queue, _) = queue_with(&ops);
    let site = FakeSite::default();
    let mut up = Uploader::new();
    up.batch_limit = Some(1);
    let now = Instant::now();
    let wait = up.pass(&mut queue, &site, Site::Staging, &key(KEY), now);
    assert_eq!(site.requests(), 30);
    assert!(wait <= RATE_WINDOW && wait > RATE_WINDOW / 2, "{wait:?}");
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + wait / 2);
    assert_eq!(site.requests(), 30);
    let later = now + RATE_WINDOW + Duration::from_secs(1);
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), later);
    assert_eq!(site.requests(), 31);
    assert!(ids(&queue).is_empty());
}

#[test]
fn sending_names_the_raid_while_the_request_is_out() {
    struct Peek<'a>(&'a FakeSite, StatusWatch, RefCell<Vec<SyncStatus>>);
    impl Uplink for Peek<'_> {
        fn post_operations(
            &self,
            site: Site,
            key: &HelperKey,
            body: &str,
        ) -> Result<UploadReply, String> {
            self.2.borrow_mut().push(self.1.get());
            self.0.post_operations(site, key, body)
        }
    }
    let (mut queue, _) = queue_with(&[award("aw-1111aaaa", "staging")]);
    let site = FakeSite::default();
    let mut up = Uploader::new();
    let peek = Peek(&site, up.watch(), RefCell::default());
    up.pass(&mut queue, &peek, Site::Staging, &key(KEY), Instant::now());
    assert_eq!(
        peek.2.borrow().as_slice(),
        [SyncStatus::Sending {
            raid: "cm9xr2k4a0001".into()
        }]
    );
    assert_eq!(up.watch().get(), SyncStatus::Idle { other_site: 0 });
}

#[test]
fn an_answer_that_doesnt_match_the_upload_is_sent_again() {
    let (mut queue, _) = queue_with(&[award("aw-1111aaaa", "staging")]);
    let site = FakeSite::default();
    site.then(Script::Answer(200, None, json!({ "results": [] })));
    let mut up = Uploader::new();
    let now = Instant::now();
    let wait = up.pass(&mut queue, &site, Site::Staging, &key(KEY), now);
    assert_eq!(up.status(), SyncStatus::Offline { waiting: 1 });
    up.pass(&mut queue, &site, Site::Staging, &key(KEY), now + wait);
    assert!(ids(&queue).is_empty());
}

#[test]
fn the_status_is_plain_data_for_the_window() {
    let status = SyncStatus::Offline { waiting: 3 };
    assert_eq!(
        serde_json::to_value(status).unwrap(),
        json!({ "kind": "offline", "waiting": 3 })
    );
    assert_eq!(
        serde_json::to_value(SyncStatus::Idle { other_site: 2 }).unwrap(),
        json!({ "kind": "idle", "otherSite": 2 })
    );
    assert_eq!(
        serde_json::to_value(SyncStatus::KeyStopped).unwrap(),
        json!({ "kind": "keyStopped" })
    );
}

/// A site on this machine that answers one request with `response` and hands back what it
/// was sent, head and body.
fn one_answer(response: String) -> (String, std::thread::JoinHandle<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = Vec::new();
        let mut byte = [0u8];
        while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap() == 1 {
            request.push(byte[0]);
        }
        let head = String::from_utf8_lossy(&request).to_ascii_lowercase();
        let length: usize = head
            .lines()
            .find_map(|l| l.strip_prefix("content-length:"))
            .map_or(0, |v| v.trim().parse().unwrap());
        let mut body = vec![0u8; length];
        stream.read_exact(&mut body).unwrap();
        request.extend(body);
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });
    (address, handle)
}

#[test]
fn the_site_client_posts_the_upload_with_the_key_and_reads_retry_after() {
    let body = r#"{"error":"Slow.","code":"rate-limited"}"#;
    let (address, handle) = one_answer(format!(
        "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 17\r\n\
         Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    ));
    let client = SiteClient::with_addresses("http://127.0.0.1:9", &address, Duration::from_secs(5));
    let reply = client
        .post_operations(Site::Staging, &key(KEY), r#"{"operations":[]}"#)
        .unwrap();
    assert_eq!(
        reply,
        UploadReply {
            status: 429,
            retry_after: Some(17),
            body: Some(json!({ "error": "Slow.", "code": "rate-limited" })),
        }
    );
    let request = handle.join().unwrap();
    assert!(
        request.starts_with("POST /api/helper/operations "),
        "{request}"
    );
    assert!(
        request.contains(&format!("authorization: Bearer {KEY}")),
        "{request}"
    );
    assert!(request.ends_with(r#"{"operations":[]}"#), "{request}");
}

#[test]
fn the_site_client_reports_no_answer_without_the_key() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let client = SiteClient::with_addresses(&address, &address, Duration::from_secs(5));
    let problem = client
        .post_operations(Site::Production, &key(KEY), "{}")
        .unwrap_err();
    assert!(!problem.contains(KEY));
}
