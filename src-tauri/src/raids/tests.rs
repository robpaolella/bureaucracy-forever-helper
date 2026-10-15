use super::*;
use crate::commands::set_staging;
use crate::key::{HelperKey, KeyEngine, StoreAction, StoreError, StoreProblem};
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

const KEY: &str = "bfh_q7Xk2mP9vR4tW8yZ1aB3cD5eF6gH0iJ-kL_nO2pQ4sA";
const STAGING_KEY: &str = "bfh_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

const TWO: &str = r#"[
  {"id":"r2","code":"mc-2","name":"Molten Core","startsAt":1792000000,"durationMin":180,"instance":409,"hasLootTable":true},
  {"id":"r1","code":"custom","name":"Mystery","startsAt":1792100000,"durationMin":90,"hasLootTable":false}]"#;

fn raid(id: &str, instance: Option<i64>) -> Raid {
    Raid {
        id: id.to_string(),
        code: "c".to_string(),
        name: "Name".to_string(),
        starts_at: 1792000000,
        duration_min: 60,
        instance,
        has_loot_table: true,
    }
}

/// A site on this machine whose answer a test can change; it keeps each request's head.
#[derive(Clone)]
struct FakeSite {
    address: String,
    answer: Arc<Mutex<(&'static str, String)>>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeSite {
    fn start(status: &'static str, body: &str) -> FakeSite {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let site = FakeSite {
            address: format!("http://{}", listener.local_addr().unwrap()),
            answer: Arc::new(Mutex::new((status, body.to_string()))),
            requests: Arc::default(),
        };
        let (answer, requests) = (Arc::clone(&site.answer), Arc::clone(&site.requests));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut head = Vec::new();
                let mut byte = [0u8];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    head.push(byte[0]);
                }
                requests
                    .lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&head).into_owned());
                let (status, body) = answer.lock().unwrap().clone();
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        site
    }

    fn say(&self, status: &'static str, body: &str) {
        *self.answer.lock().unwrap() = (status, body.to_string());
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

#[derive(Default)]
struct FakeStore {
    keys: Mutex<HashMap<Site, HelperKey>>,
    failing: bool,
}

impl SecretStore for FakeStore {
    fn save(&self, site: Site, key: &HelperKey) -> Result<(), StoreError> {
        self.keys.lock().unwrap().insert(site, key.clone());
        Ok(())
    }
    fn read(&self, site: Site) -> Result<Option<HelperKey>, StoreError> {
        if self.failing {
            return Err(StoreError {
                action: StoreAction::Read,
                problem: StoreProblem::Unavailable,
            });
        }
        Ok(self.keys.lock().unwrap().get(&site).cloned())
    }
    fn delete(&self, site: Site) -> Result<(), StoreError> {
        self.keys.lock().unwrap().remove(&site);
        Ok(())
    }
}

fn settings_path() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-raids-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("settings.json")
}

fn engine(production: &FakeSite, staging: &FakeSite, store: FakeStore) -> Engine<FakeStore> {
    let client = SiteClient::with_addresses(
        &production.address,
        &staging.address,
        Duration::from_secs(2),
    );
    Engine::new(KeyEngine::open(settings_path(), store, client))
}

fn with_keys() -> FakeStore {
    let store = FakeStore::default();
    {
        let mut keys = store.keys.lock().unwrap();
        keys.insert(Site::Production, HelperKey::parse(KEY).unwrap());
        keys.insert(Site::Staging, HelperKey::parse(STAGING_KEY).unwrap());
    }
    store
}

fn client(site: &FakeSite) -> SiteClient {
    SiteClient::with_addresses(&site.address, &site.address, Duration::from_secs(2))
}

fn fetch(site: &FakeSite) -> Result<Vec<Raid>, SiteError> {
    client(site).raids(Site::Production, &HelperKey::parse(KEY).unwrap())
}

/// Timing short enough for a test to watch several passes.
fn quick() -> Timing {
    Timing {
        tick: Duration::from_millis(40),
        slow: Duration::from_millis(40),
        stopped: Duration::from_secs(3600),
    }
}

fn start(site: &FakeSite, staging: &FakeSite, store: FakeStore) -> (Poller, Engine<FakeStore>) {
    let engine = engine(site, staging, store);
    let poller = Poller::start(engine.clone(), client(site), quick(), |_| {});
    (poller, engine)
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..200 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for {what}");
}

fn wait_state(poller: &Poller, state: RaidsState) {
    wait_for(&format!("{state:?}"), || poller.view().state == state);
}

#[test]
fn asks_for_the_raids_with_the_key_and_keeps_all_in_order() {
    let site = FakeSite::start("200 OK", TWO);
    let raids = fetch(&site).unwrap();
    let head = &site.requests()[0];
    assert!(head.starts_with("GET /api/helper/raids HTTP/1.1"), "{head}");
    assert!(head.contains(&format!("Bearer {KEY}")) || head.contains(&format!("bearer {KEY}")));
    assert_eq!(
        raids,
        vec![
            Raid {
                id: "r2".into(),
                code: "mc-2".into(),
                name: "Molten Core".into(),
                starts_at: 1792000000,
                duration_min: 180,
                instance: Some(409),
                has_loot_table: true,
            },
            Raid {
                id: "r1".into(),
                code: "custom".into(),
                name: "Mystery".into(),
                starts_at: 1792100000,
                duration_min: 90,
                instance: None,
                has_loot_table: false,
            },
        ]
    );
}

#[test]
fn an_empty_answer_is_an_empty_list() {
    let site = FakeSite::start("200 OK", "[]");
    assert_eq!(fetch(&site), Ok(vec![]));
}

#[test]
fn the_list_may_come_inside_a_raids_field() {
    let site = FakeSite::start("200 OK", &format!(r#"{{"raids":{TWO}}}"#));
    assert_eq!(fetch(&site).unwrap().len(), 2);
}

#[test]
fn an_unreadable_answer_is_a_failure() {
    let missing = |field: &str| {
        let mut entry = json!({"id":"r","name":"N","startsAt":1792000000});
        entry.as_object_mut().unwrap().remove(field);
        json!([entry]).to_string()
    };
    for body in [
        "not json".to_string(),
        r#"{"other":1}"#.to_string(),
        missing("id"),
        missing("name"),
        missing("startsAt"),
        r#"[{"id":"r","name":"N","startsAt":"soon"}]"#.to_string(),
        // One bad entry spoils the whole answer.
        r#"[{"id":"r","name":"N","startsAt":1},{"name":"N","startsAt":1}]"#.to_string(),
    ] {
        let site = FakeSite::start("200 OK", &body);
        assert!(
            matches!(fetch(&site), Err(SiteError::Unreachable { .. })),
            "{body}"
        );
    }
}

#[test]
fn failures_are_sorted_as_the_key_check_sorts_them() {
    let cases = [
        (
            "401 Unauthorized",
            r#"{"error":"x","code":"bad-key"}"#,
            "refused",
        ),
        (
            "403 Forbidden",
            r#"{"error":"x","code":"not-officer"}"#,
            "refused",
        ),
        (
            "503 Service Unavailable",
            r#"{"error":"x","code":"helper-off"}"#,
            "off",
        ),
        ("500 Internal Server Error", "{}", "unreachable"),
        (
            "429 Too Many Requests",
            r#"{"error":"x","code":"rate-limited"}"#,
            "unreachable",
        ),
        ("413 Payload Too Large", "{}", "unreachable"),
    ];
    for (status, body, want) in cases {
        let site = FakeSite::start(status, body);
        let got = match fetch(&site) {
            Err(SiteError::KeyRefused(_)) => "refused",
            Err(SiteError::HelperOff) => "off",
            Err(SiteError::Unreachable { .. }) => "unreachable",
            other => panic!("{status}: {other:?}"),
        };
        assert_eq!(got, want, "{status}");
    }
    // Nothing listening.
    let dead = FakeSite::start("200 OK", "[]");
    let gone = SiteClient::with_addresses(
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
        Duration::from_secs(1),
    );
    drop(dead);
    assert!(matches!(
        gone.raids(Site::Production, &HelperKey::parse(KEY).unwrap()),
        Err(SiteError::Unreachable { .. })
    ));
}

#[test]
fn views_follow_each_result_and_keep_the_last_good_list() {
    let fresh = RaidsView::loading().after(Ok(vec![raid("a", None)]));
    assert_eq!(fresh.state, RaidsState::Fresh);
    let failed = fresh.clone().after(Err(SiteError::Unreachable {
        detail: "it answered 500".into(),
    }));
    assert_eq!(failed.state, RaidsState::Unreachable);
    assert_eq!(failed.raids, fresh.raids);
    let refused = fresh
        .clone()
        .after(Err(SiteError::KeyRefused(Refusal::NotOfficer)));
    assert_eq!(
        (refused.state, refused.raids.len()),
        (RaidsState::KeyRefused, 1)
    );
    let off = fresh.clone().after(Err(SiteError::HelperOff));
    assert_eq!((off.state, off.raids.len()), (RaidsState::SiteOff, 1));
    // An empty answer is a good answer: it replaces the list.
    let empty = fresh.after(Ok(vec![]));
    assert_eq!((empty.state, empty.raids.len()), (RaidsState::Empty, 0));
}

#[test]
fn the_timer_fetches_on_its_own_and_keeps_the_list_through_a_failure() {
    let site = FakeSite::start("200 OK", TWO);
    let (poller, _) = start(&site, &site, with_keys());
    assert_eq!(poller.view().state, RaidsState::Loading);
    wait_state(&poller, RaidsState::Fresh);
    assert_eq!(poller.view().raids.len(), 2);
    site.say("500 Internal Server Error", "{}");
    wait_state(&poller, RaidsState::Unreachable);
    assert_eq!(poller.view().raids.len(), 2);
    site.say("200 OK", "[]");
    wait_state(&poller, RaidsState::Empty);
    assert!(poller.view().raids.is_empty());
    poller.stop();
    poller.join();
}

#[test]
fn with_no_key_nothing_is_requested() {
    let site = FakeSite::start("200 OK", TWO);
    let (poller, _) = start(&site, &site, FakeStore::default());
    wait_state(&poller, RaidsState::NoKey);
    std::thread::sleep(Duration::from_millis(150));
    assert!(site.requests().is_empty());
    poller.stop();
    poller.join();
}

#[test]
fn an_unreadable_store_is_reported_and_nothing_is_requested() {
    let site = FakeSite::start("200 OK", TWO);
    let store = FakeStore {
        failing: true,
        ..FakeStore::default()
    };
    let (poller, _) = start(&site, &site, store);
    wait_for("the store problem", || {
        matches!(poller.view().state, RaidsState::KeyUnreadable { .. })
    });
    assert!(site.requests().is_empty());
    poller.stop();
    poller.join();
}

#[test]
fn a_refused_key_stops_fetching_but_keeps_the_list_until_the_key_changes() {
    let site = FakeSite::start("200 OK", TWO);
    let (poller, _) = start(&site, &site, with_keys());
    wait_state(&poller, RaidsState::Fresh);
    site.say("401 Unauthorized", r#"{"error":"x","code":"bad-key"}"#);
    wait_state(&poller, RaidsState::KeyRefused);
    assert_eq!(poller.view().raids.len(), 2);
    let asked = site.requests().len();
    std::thread::sleep(Duration::from_millis(200));
    assert_eq!(site.requests().len(), asked);
    // A new key restarts it.
    site.say("200 OK", TWO);
    poller.restart();
    assert_eq!(poller.view().state, RaidsState::Loading);
    wait_state(&poller, RaidsState::Fresh);
    poller.stop();
    poller.join();
}

#[test]
fn a_switched_off_site_keeps_the_list_and_is_asked_again() {
    let site = FakeSite::start("200 OK", TWO);
    let (poller, _) = start(&site, &site, with_keys());
    wait_state(&poller, RaidsState::Fresh);
    site.say(
        "503 Service Unavailable",
        r#"{"error":"x","code":"helper-off"}"#,
    );
    wait_state(&poller, RaidsState::SiteOff);
    assert_eq!(poller.view().raids.len(), 2);
    site.say("200 OK", TWO);
    wait_state(&poller, RaidsState::Fresh);
    poller.stop();
    poller.join();
}

#[test]
fn switching_site_drops_the_list_and_starts_a_fresh_one() {
    let production = FakeSite::start("200 OK", TWO);
    let staging = FakeSite::start(
        "200 OK",
        r#"[{"id":"s1","name":"Test","startsAt":1792000000}]"#,
    );
    let engine = engine(&production, &staging, with_keys());
    // The poller's own client addresses the same two sites.
    let poller = Poller::start(
        engine.clone(),
        SiteClient::with_addresses(
            &production.address,
            &staging.address,
            Duration::from_secs(2),
        ),
        quick(),
        |_| {},
    );
    wait_state(&poller, RaidsState::Fresh);
    assert_eq!(poller.view().raids.len(), 2);
    assert!(staging.requests().is_empty());
    set_staging(&engine, true);
    poller.restart();
    assert_eq!(poller.view(), RaidsView::loading());
    wait_for("the staging list", || {
        poller.view().raids.iter().map(|r| r.id.as_str()).eq(["s1"])
    });
    assert!(staging.requests()[0].contains(STAGING_KEY));
    poller.stop();
    poller.join();
}

#[test]
fn changes_are_announced_once_each() {
    let site = FakeSite::start("200 OK", TWO);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let engine = engine(&site, &site, with_keys());
    let log = Arc::clone(&seen);
    let poller = Poller::start(engine, client(&site), quick(), move |view| {
        log.lock().unwrap().push(view.state.clone())
    });
    wait_state(&poller, RaidsState::Fresh);
    std::thread::sleep(Duration::from_millis(200));
    poller.stop();
    poller.join();
    assert_eq!(*seen.lock().unwrap(), vec![RaidsState::Fresh]);
}

#[test]
fn the_view_serialises_for_the_window() {
    let view = RaidsView {
        state: RaidsState::Fresh,
        raids: vec![raid("a", Some(409)), raid("b", None)],
    };
    assert_eq!(
        serde_json::to_value(&view).unwrap(),
        json!({"state": {"kind": "fresh"}, "raids": [
            {"id":"a","code":"c","name":"Name","startsAt":1792000000,"durationMin":60,"instance":409,"hasLootTable":true},
            {"id":"b","code":"c","name":"Name","startsAt":1792000000,"durationMin":60,"instance":null,"hasLootTable":true}]})
    );
    let unreadable = RaidsState::KeyUnreadable {
        message: "m".into(),
    };
    assert_eq!(
        serde_json::to_value(unreadable).unwrap(),
        json!({"kind": "keyUnreadable", "message": "m"})
    );
}
