use super::*;
use crate::key::{KeyEngine, StoreError};
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write as _};
use std::net::TcpListener;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

const KEY: &str = "bfh_q7Xk2mP9vR4tW8yZ1aB3cD5eF6gH0iJ-kL_nO2pQ4sA";
const WRITTEN: i64 = 1_000;

#[derive(Default)]
struct FakeStore {
    keys: Mutex<HashMap<Site, HelperKey>>,
}

impl SecretStore for FakeStore {
    fn save(&self, site: Site, key: &HelperKey) -> Result<(), StoreError> {
        self.keys.lock().unwrap().insert(site, key.clone());
        Ok(())
    }
    fn read(&self, site: Site) -> Result<Option<HelperKey>, StoreError> {
        Ok(self.keys.lock().unwrap().get(&site).cloned())
    }
    fn delete(&self, site: Site) -> Result<(), StoreError> {
        self.keys.lock().unwrap().remove(&site);
        Ok(())
    }
}

fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-reserves-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn engine(with_key: bool) -> Engine<FakeStore> {
    let store = FakeStore::default();
    if with_key {
        store
            .keys
            .lock()
            .unwrap()
            .insert(Site::Production, HelperKey::parse(KEY).unwrap());
    }
    let client = SiteClient::with_addresses(
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
        Duration::from_secs(1),
    );
    Engine::new(KeyEngine::open(
        scratch_dir().join("settings.json"),
        store,
        client,
    ))
}

fn raid(id: &str, name: &str) -> Raid {
    Raid {
        id: id.to_string(),
        code: format!("code-{id}"),
        name: name.to_string(),
        starts_at: 1_792_000_000,
        duration_min: 180,
        instance: Some(409),
        has_loot_table: true,
    }
}

fn view_of(state: RaidsState, raids: Vec<Raid>) -> RaidsView {
    RaidsView { state, raids }
}

fn snapshot(id: &str, name: &str, reserve: &str) -> Value {
    json!({ "formatVersion": 1, "raid": id, "name": name, "generated": 5,
        "reserves": { "16805": [ { "user": "u", "char": reserve, "kind": "SR", "trial": false } ] } })
}

fn catchup(id: &str) -> Value {
    json!({ "formatVersion": 1, "raid": id, "awards": [], "voids": [] })
}

type Answer = Result<Value, RaidsFailure>;

/// A site whose documents a test can change; it records every request with its time.
#[derive(Clone, Default)]
struct FakeSource {
    answers: Arc<Mutex<HashMap<(String, Part), Answer>>>,
    calls: Arc<Mutex<Vec<(String, Part, Instant)>>>,
    /// Failures handed out first, one per request, before the answers.
    failures: Arc<Mutex<Vec<RaidsFailure>>>,
    /// How long a catch-up takes to arrive.
    slow_catchup: Arc<Mutex<Duration>>,
}

impl FakeSource {
    fn set(&self, id: &str, part: Part, answer: Answer) {
        self.answers
            .lock()
            .unwrap()
            .insert((id.to_string(), part), answer);
    }

    fn serve(&self, id: &str, name: &str, reserve: &str) {
        self.set(id, Part::Snapshot, Ok(snapshot(id, name, reserve)));
        self.set(id, Part::Catchup, Ok(catchup(id)));
    }

    fn calls(&self) -> Vec<(String, Part)> {
        let calls = self.calls.lock().unwrap();
        calls
            .iter()
            .map(|(id, part, _)| (id.clone(), *part))
            .collect()
    }
}

impl ReservesSource for FakeSource {
    fn fetch(&self, _: Site, _: &HelperKey, raid_id: &str, part: Part) -> Answer {
        self.calls
            .lock()
            .unwrap()
            .push((raid_id.to_string(), part, Instant::now()));
        if let Some(failure) = self.failures.lock().unwrap().pop() {
            return Err(failure);
        }
        let answer = self
            .answers
            .lock()
            .unwrap()
            .get(&(raid_id.to_string(), part))
            .cloned()
            .unwrap_or_else(|| {
                Err(RaidsFailure::from(SiteError::Unreachable {
                    detail: "it answered 404".into(),
                }))
            });
        if part == Part::Catchup {
            std::thread::sleep(*self.slow_catchup.lock().unwrap());
        }
        answer
    }
}

fn quick() -> Timing {
    Timing {
        tick: Duration::from_millis(30),
        spacing: Duration::from_millis(1),
        pending: Duration::from_millis(10),
        slow: Duration::from_millis(30),
        stopped: Duration::from_secs(3600),
        max_retry_after: Duration::from_secs(3600),
    }
}

struct Rig {
    reserves: Reserves,
    source: FakeSource,
    raids: Arc<Mutex<RaidsView>>,
    inbox: PathBuf,
    folder: Arc<Mutex<PathBuf>>,
    now: Arc<AtomicI64>,
    events: Arc<Mutex<Vec<ReservesView>>>,
}

impl Rig {
    fn start(raids: RaidsView) -> Rig {
        Rig::with(raids, quick(), true, true)
    }

    fn with(raids: RaidsView, timing: Timing, key: bool, known: bool) -> Rig {
        let source = FakeSource::default();
        let shared_raids = Arc::new(Mutex::new(raids));
        let inbox = scratch_dir().join("Inbox.lua");
        let folder = Arc::new(Mutex::new(inbox.clone()));
        let now = Arc::new(AtomicI64::new(WRITTEN));
        let events: Arc<Mutex<Vec<ReservesView>>> = Arc::default();
        let reserves = Reserves::start(
            engine(key),
            Inputs {
                source: source.clone(),
                raids: {
                    let raids = Arc::clone(&shared_raids);
                    move || raids.lock().unwrap().clone()
                },
                target: {
                    let target = Arc::clone(&folder);
                    move || known.then(|| target.lock().unwrap().clone())
                },
                now: {
                    let now = Arc::clone(&now);
                    move || now.load(Ordering::SeqCst)
                },
                timing,
            },
            {
                let events = Arc::clone(&events);
                move |view| events.lock().unwrap().push(view.clone())
            },
        );
        Rig {
            reserves,
            source,
            raids: shared_raids,
            inbox,
            folder,
            now,
            events,
        }
    }

    fn file(&self) -> Option<Vec<u8>> {
        fs::read(&self.inbox).ok()
    }

    fn wait_for(&self, what: &str, mut done: impl FnMut(&ReservesView) -> bool) {
        for _ in 0..200 {
            if done(&self.reserves.view()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("timed out waiting for {what}: {:?}", self.reserves.view());
    }

    /// Lets several passes go by.
    fn settle(&self) {
        std::thread::sleep(Duration::from_millis(300));
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.reserves.stop();
        self.reserves.join();
    }
}

fn two_raids() -> RaidsView {
    view_of(
        RaidsState::Fresh,
        vec![raid("r1", "Molten Core"), raid("r2", "Blackwing Lair")],
    )
}

fn expected(rig: &Rig, ids: &[(&str, &str, &str)], written: i64) -> Vec<u8> {
    let data: Vec<RaidData> = ids
        .iter()
        .map(|(id, name, reserve)| RaidData {
            snapshot: snapshot(id, name, reserve),
            catchup: Some(catchup(id)),
        })
        .collect();
    let _ = rig;
    inbox::render_inbox(&data, written).unwrap()
}

#[test]
fn several_raids_reach_the_writer_in_the_lists_order() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    assert_eq!(
        rig.file().unwrap(),
        expected(
            &rig,
            &[("r1", "Molten Core", "Ana"), ("r2", "Blackwing Lair", "Bo")],
            WRITTEN
        )
    );
    let calls = rig.source.calls();
    assert_eq!(
        calls[..4],
        [
            ("r1".to_string(), Part::Snapshot),
            ("r1".to_string(), Part::Catchup),
            ("r2".to_string(), Part::Snapshot),
            ("r2".to_string(), Part::Catchup),
        ]
    );
}

#[test]
fn no_raids_writes_an_inbox_with_none() {
    let rig = Rig::start(view_of(RaidsState::Empty, vec![]));
    rig.wait_for("a file", |_| rig.file().is_some());
    assert_eq!(
        rig.file().unwrap(),
        inbox::render_inbox(&[], WRITTEN).unwrap()
    );
    assert!(rig.source.calls().is_empty());
    rig.settle();
    // Nothing to load, so nothing to prompt for.
    assert_eq!(rig.reserves.view(), ReservesView::waiting());
}

#[test]
fn the_first_write_is_in_the_game_and_a_change_asks_for_a_reload() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("the first write", |v| v.changes == 1);
    let view = rig.reserves.view();
    assert_eq!(
        view.state,
        ReservesState::InGame {
            raids: vec!["Molten Core".into(), "Blackwing Lair".into()]
        }
    );
    assert!(view.first);

    rig.now.store(2_000, Ordering::SeqCst);
    rig.source.serve("r1", "Molten Core", "Cy");
    rig.wait_for("the change", |v| v.changes == 2);
    let view = rig.reserves.view();
    assert_eq!(
        view.state,
        ReservesState::Reload {
            raids: vec!["Molten Core".into(), "Blackwing Lair".into()],
            changed: vec!["Molten Core".into()],
        }
    );
    assert!(!view.first);
    assert_eq!(
        rig.file().unwrap(),
        expected(
            &rig,
            &[("r1", "Molten Core", "Cy"), ("r2", "Blackwing Lair", "Bo")],
            2_000
        )
    );
}

#[test]
fn an_unchanged_result_writes_nothing_and_says_nothing() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    let first = rig.file().unwrap();
    let events = rig.events.lock().unwrap().len();
    let calls = rig.source.calls().len();
    // Only the time differs on the next passes.
    rig.now.store(9_999, Ordering::SeqCst);
    rig.settle();
    assert!(rig.source.calls().len() > calls, "it kept polling");
    assert_eq!(rig.file().unwrap(), first, "the file was not rewritten");
    assert_eq!(rig.events.lock().unwrap().len(), events, "no event");
    assert_eq!(rig.reserves.view().changes, 1);
}

#[test]
fn a_file_from_an_earlier_run_that_is_still_right_is_left_alone_and_in_the_game() {
    let rig = Rig::start(view_of(RaidsState::Fresh, vec![raid("r1", "Molten Core")]));
    // Written at another time, as a run before this one would have left it.
    let old = inbox::render_inbox(
        &[RaidData {
            snapshot: snapshot("r1", "Molten Core", "Ana"),
            catchup: Some(catchup("r1")),
        }],
        5,
    )
    .unwrap();
    fs::write(&rig.inbox, &old).unwrap();
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.wait_for("in the game", |v| {
        matches!(v.state, ReservesState::InGame { .. })
    });
    rig.settle();
    assert_eq!(rig.file().unwrap(), old);
    assert_eq!(
        rig.reserves.view().changes,
        0,
        "no notice for a file that was right"
    );
}

#[test]
fn a_file_from_an_earlier_run_that_differs_asks_for_a_reload() {
    let rig = Rig::start(view_of(RaidsState::Fresh, vec![raid("r1", "Molten Core")]));
    fs::write(
        &rig.inbox,
        "BureaucracyLootInbox = {formatVersion=1,written=5,raids={}}\n",
    )
    .unwrap();
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.wait_for("a reload prompt", |v| {
        matches!(v.state, ReservesState::Reload { .. })
    });
    assert_eq!(rig.reserves.view().changes, 1);
}

#[test]
fn a_failed_fetch_leaves_the_old_file_and_says_nothing() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    let first = rig.file().unwrap();
    let events = rig.events.lock().unwrap().len();

    // The first raid changed, but the second can't be fetched: nothing is written.
    rig.now.store(2_000, Ordering::SeqCst);
    rig.source.serve("r1", "Molten Core", "Cy");
    rig.source.set(
        "r2",
        Part::Catchup,
        Err(RaidsFailure::from(SiteError::Unreachable {
            detail: "it didn't answer in time".into(),
        })),
    );
    rig.settle();
    assert_eq!(rig.file().unwrap(), first);
    assert_eq!(rig.events.lock().unwrap().len(), events);

    // Once the site answers, the change goes through.
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("the change", |v| v.changes == 2);
}

#[test]
fn a_snapshot_the_writer_refuses_leaves_the_old_file_and_shows_the_error() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    let first = rig.file().unwrap();

    rig.now.store(2_000, Ordering::SeqCst);
    rig.source.serve("r2", "Blackwing Lair", &"x".repeat(300));
    rig.wait_for("the error", |v| v.problem.is_some());
    let problem = rig.reserves.view().problem.unwrap();
    assert!(
        problem.contains("Blackwing Lair"),
        "names the raid: {problem}"
    );
    assert_eq!(rig.file().unwrap(), first);
    assert_eq!(rig.reserves.view().changes, 1);
    assert!(matches!(
        rig.reserves.view().state,
        ReservesState::InGame { .. }
    ));

    // A good snapshot clears it.
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("it clearing", |v| v.problem.is_none());
}

#[test]
fn a_rate_limit_is_waited_out_as_the_site_asks() {
    let rig = Rig::start(view_of(RaidsState::Fresh, vec![raid("r1", "Molten Core")]));
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.failures.lock().unwrap().push(RaidsFailure {
        error: SiteError::Unreachable {
            detail: "it answered 429".into(),
        },
        retry_after: Some(1),
    });
    rig.wait_for("a write", |v| v.changes == 1);
    let calls = rig.source.calls.lock().unwrap();
    assert_eq!(calls.len(), 3, "the refused request, then both documents");
    let waited = calls[1].2.duration_since(calls[0].2);
    assert!(
        waited >= Duration::from_millis(950),
        "waited only {waited:?}"
    );
}

#[test]
fn the_wait_after_a_failure_follows_retry_after_within_limits() {
    let timing = Timing::default();
    let failure = |retry_after| RaidsFailure {
        error: SiteError::Unreachable {
            detail: String::new(),
        },
        retry_after,
    };
    assert_eq!(timing.after_failure(&failure(None)), timing.tick);
    assert_eq!(timing.after_failure(&failure(Some(5))), timing.tick);
    assert_eq!(
        timing.after_failure(&failure(Some(90))),
        Duration::from_secs(90)
    );
    assert_eq!(
        timing.after_failure(&failure(Some(999_999))),
        timing.max_retry_after
    );
    assert_eq!(
        timing.after_failure(&RaidsFailure::from(SiteError::HelperOff)),
        timing.slow
    );
}

#[test]
fn requests_stay_within_the_sites_limit_for_three_raids() {
    // The default timing: never closer than `spacing`, so at most 20 a minute, plus the raids
    // list's one a minute, under the 30 the site allows (7.6).
    let timing = Timing::default();
    assert!(
        timing.most_per_minute() < 30 - 5,
        "room is left for uploads"
    );
    // Three raids is six requests a pass, and a pass is followed by the tick.
    let pass = timing.spacing * 5 + timing.tick;
    assert!(6.0 / pass.as_secs_f64() * 60.0 <= timing.most_per_minute() as f64);

    // And the timer really spaces them.
    let mut quick = quick();
    quick.spacing = Duration::from_millis(40);
    let raids = view_of(
        RaidsState::Fresh,
        vec![raid("r1", "A"), raid("r2", "B"), raid("r3", "C")],
    );
    let rig = Rig::with(raids, quick, true, true);
    for id in ["r1", "r2", "r3"] {
        rig.source.serve(id, id, "Ana");
    }
    rig.wait_for("a write", |v| v.changes == 1);
    let calls = rig.source.calls.lock().unwrap();
    assert!(calls.len() >= 6);
    for pair in calls.windows(2) {
        let gap = pair[1].2.duration_since(pair[0].2);
        assert!(
            gap >= Duration::from_millis(35),
            "two requests {gap:?} apart"
        );
    }
}

#[test]
fn nothing_is_requested_without_a_key_a_folder_or_a_known_list() {
    let none = Rig::with(two_raids(), quick(), false, true);
    let no_folder = Rig::with(two_raids(), quick(), true, false);
    let loading = Rig::with(view_of(RaidsState::Loading, vec![]), quick(), true, true);
    let stopped = Rig::with(
        view_of(RaidsState::KeyRefused, vec![raid("r1", "A")]),
        quick(),
        true,
        true,
    );
    for rig in [&none, &no_folder, &loading, &stopped] {
        rig.settle();
        assert!(rig.source.calls().is_empty());
        assert!(rig.file().is_none());
    }
    // The list becoming known starts it.
    loading.source.serve("r1", "A", "Ana");
    *loading.raids.lock().unwrap() = view_of(RaidsState::Fresh, vec![raid("r1", "A")]);
    loading.wait_for("a write", |v| v.changes == 1);
}

#[test]
fn the_reload_prompt_ends_when_the_game_saves_after_the_write() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    rig.now.store(2_000, Ordering::SeqCst);
    rig.source.serve("r1", "Molten Core", "Cy");
    rig.wait_for("the change", |v| v.changes == 2);

    rig.reserves.game_saved(1_999);
    assert!(matches!(
        rig.reserves.view().state,
        ReservesState::Reload { .. }
    ));
    rig.reserves.game_saved(2_001);
    assert_eq!(
        rig.reserves.view().state,
        ReservesState::InGame {
            raids: vec!["Molten Core".into(), "Blackwing Lair".into()]
        }
    );
    assert_eq!(rig.reserves.view().changes, 2, "a reload is not a change");
}

#[test]
fn a_restart_forgets_what_was_written_but_not_how_many_notices_were_counted() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    rig.reserves.restart();
    assert_eq!(rig.reserves.view().state, ReservesState::Waiting);
    assert_eq!(rig.reserves.view().changes, 1);
    // The same data on disk is right for the new key too: in the game, no new notice.
    rig.wait_for("in the game again", |v| {
        matches!(v.state, ReservesState::InGame { .. })
    });
    assert_eq!(rig.reserves.view().changes, 1);
}

#[test]
fn the_last_snapshots_raids_and_bosses_are_known_until_a_restart() {
    let rig = Rig::start(view_of(RaidsState::Fresh, vec![raid("r1", "Molten Core")]));
    assert!(rig.reserves.known_raids().is_none());
    let mut snap = snapshot("r1", "Molten Core", "Ana");
    snap["env"] = json!("staging");
    snap["bosses"] = json!([
        { "id": "lb_lucifron", "name": "Lucifron", "trash": false, "items": [16805] },
        { "id": "lb_trash", "name": "Trash", "trash": true, "items": [] }
    ]);
    rig.source.set("r1", Part::Snapshot, Ok(snap));
    rig.source.set("r1", Part::Catchup, Ok(catchup("r1")));
    rig.wait_for("a write", |v| v.changes == 1);
    let known = rig.reserves.known_raids().unwrap();
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].raid.id, "r1");
    assert_eq!(known[0].raid.env, Env::Staging);
    assert_eq!(known[0].raid.bosses, ["lb_lucifron", "lb_trash"]);
    assert_eq!(known[0].name, "Molten Core");
    assert_eq!(known[0].boss_names["lb_lucifron"], "Lucifron");
    // A failed fetch keeps them; a new key or site forgets them.
    rig.source.answers.lock().unwrap().clear();
    rig.settle();
    assert_eq!(rig.reserves.known_raids().unwrap().len(), 1);
    rig.reserves.restart();
    rig.settle();
    assert!(rig.reserves.known_raids().is_none());
}

#[test]
fn a_snapshot_without_an_environment_or_bosses_gives_no_known_raid() {
    let data = [RaidData {
        snapshot: snapshot("r1", "Molten Core", "Ana"),
        catchup: None,
    }];
    assert!(named_raids(&data).is_empty());
}

#[test]
fn the_view_serialises_for_the_window() {
    let view = ReservesView {
        state: ReservesState::Reload {
            raids: vec!["Molten Core".into()],
            changed: vec!["Molten Core".into()],
        },
        changes: 2,
        first: false,
        problem: None,
    };
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        json!({ "state": { "kind": "reload", "raids": ["Molten Core"], "changed": ["Molten Core"] },
            "changes": 2, "first": false, "problem": null })
    );
    assert_eq!(
        serde_json::to_value(ReservesState::InGame { raids: vec![] }).unwrap(),
        json!({ "kind": "inGame", "raids": [] })
    );
}

/// A site on this machine for the real client: it answers each path with a canned reply.
fn site(replies: Vec<(&'static str, &'static str, String)>) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let heads: Arc<Mutex<Vec<String>>> = Arc::default();
    let seen = Arc::clone(&heads);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut head = Vec::new();
            let mut byte = [0u8];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
            seen.lock().unwrap().push(head);
            let (status, extra, body) = replies
                .iter()
                .find(|(p, _, _)| *p == path)
                .map(|(_, status, body)| (*status, "", body.clone()))
                .unwrap_or(("404 Not Found", "", "{}".into()));
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (address, heads)
}

#[test]
fn the_site_client_asks_for_each_document_with_the_key_and_reads_big_ones() {
    let big = json!({ "filler": "x".repeat(300_000) }).to_string();
    let (address, heads) = site(vec![
        (
            "/api/helper/raids/cm9x%2F1/snapshot",
            "200 OK",
            snapshot("cm9x/1", "Molten Core", "Ana").to_string(),
        ),
        ("/api/helper/raids/cm9x%2F1/catchup", "200 OK", big),
        ("/api/helper/raids/r2/snapshot", "200 OK", "[1]".into()),
        (
            "/api/helper/raids/r3/snapshot",
            "429 Too Many Requests",
            "{}".into(),
        ),
    ]);
    let client = SiteClient::with_addresses(&address, &address, Duration::from_secs(2));
    let key = HelperKey::parse(KEY).unwrap();

    let got = client.fetch(Site::Production, &key, "cm9x/1", Part::Snapshot);
    assert_eq!(got.unwrap()["name"], "Molten Core");
    let got = client.fetch(Site::Production, &key, "cm9x/1", Part::Catchup);
    assert_eq!(got.unwrap()["filler"].as_str().unwrap().len(), 300_000);
    assert!(heads.lock().unwrap()[0].contains(&format!("Bearer {KEY}")));

    // Not an object, a limit and a missing raid are failures, never data.
    let failure = client
        .fetch(Site::Production, &key, "r2", Part::Snapshot)
        .unwrap_err();
    assert!(matches!(failure.error, SiteError::Unreachable { .. }));
    let failure = client
        .fetch(Site::Production, &key, "r3", Part::Snapshot)
        .unwrap_err();
    assert!(matches!(failure.error, SiteError::Unreachable { .. }));
    assert!(client
        .fetch(Site::Production, &key, "gone", Part::Catchup)
        .is_err());
}

#[test]
fn a_new_generated_time_alone_is_not_a_change() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    let first = rig.file().unwrap();
    let events = rig.events.lock().unwrap().len();
    // The site stamps every answer with the time it was built.
    for id in ["r1", "r2"] {
        let mut snapshot = snapshot(id, id, "x");
        snapshot["generated"] = json!(99);
        let mut catchup = catchup(id);
        catchup["generated"] = json!(99);
        let name = if id == "r1" {
            "Molten Core"
        } else {
            "Blackwing Lair"
        };
        let reserve = if id == "r1" { "Ana" } else { "Bo" };
        snapshot["name"] = json!(name);
        snapshot["reserves"] = self::snapshot(id, name, reserve)["reserves"].clone();
        rig.source.set(id, Part::Snapshot, Ok(snapshot));
        rig.source.set(id, Part::Catchup, Ok(catchup));
    }
    let calls = rig.source.calls().len();
    rig.settle();
    assert!(rig.source.calls().len() > calls);
    assert_eq!(rig.file().unwrap(), first);
    assert_eq!(rig.events.lock().unwrap().len(), events);
    assert_eq!(rig.reserves.view().changes, 1);
}

#[test]
fn a_file_from_an_earlier_run_differing_only_in_generated_is_still_right() {
    let rig = Rig::start(view_of(RaidsState::Fresh, vec![raid("r1", "Molten Core")]));
    let mut older = snapshot("r1", "Molten Core", "Ana");
    older["generated"] = json!(1);
    let old = inbox::render_inbox(
        &[RaidData {
            snapshot: older,
            catchup: Some(catchup("r1")),
        }],
        5,
    )
    .unwrap();
    fs::write(&rig.inbox, &old).unwrap();
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.wait_for("in the game", |v| {
        matches!(v.state, ReservesState::InGame { .. })
    });
    rig.settle();
    assert_eq!(rig.file().unwrap(), old);
    assert_eq!(rig.reserves.view().changes, 0);
}

#[test]
fn an_answer_that_arrives_after_a_restart_is_never_written() {
    let rig = Rig::start(view_of(RaidsState::Fresh, vec![raid("r1", "Molten Core")]));
    rig.source.serve("r1", "Molten Core", "Ana");
    *rig.source.slow_catchup.lock().unwrap() = Duration::from_millis(400);
    rig.wait_for("the catch-up request", |_| rig.source.calls().len() >= 2);
    // The key or site changes while the last answer is on its way; the new key's passes find
    // nothing to fetch.
    rig.source.answers.lock().unwrap().clear();
    rig.reserves.restart();
    std::thread::sleep(Duration::from_millis(900));
    assert!(rig.file().is_none(), "the old key's answer was written");
    assert_eq!(rig.reserves.view().state, ReservesState::Waiting);
}

#[test]
fn a_new_game_folder_gets_the_reserves_even_when_nothing_changed() {
    let rig = Rig::start(two_raids());
    rig.source.serve("r1", "Molten Core", "Ana");
    rig.source.serve("r2", "Blackwing Lair", "Bo");
    rig.wait_for("a write", |v| v.changes == 1);
    let other = scratch_dir().join("Inbox.lua");
    *rig.folder.lock().unwrap() = other.clone();
    rig.wait_for("the other folder's file", |_| other.exists());
    assert_eq!(fs::read(&other).unwrap(), rig.file().unwrap());
}
