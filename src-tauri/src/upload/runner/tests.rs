use super::*;
use crate::commands::{remove_key, set_staging, ChangeOutcome};
use crate::key::{KeyEngine, SiteClient, StoreAction, StoreError, StoreProblem};
use crate::queue::FILE_NAME;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const KEY: &str = "bfh_q7Xk2mP9vR4tW8yZ1aB3cD5eF6gH0iJ-kL_nO2pQ4sA";
const STAGING_KEY: &str = "bfh_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
const NEW_KEY: &str = "bfh_BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB";

fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-runner-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn key(text: &str) -> HelperKey {
    HelperKey::parse(text).unwrap()
}

/// An award with an id made from `name`, for `env`.
fn award(name: &str, env: &str) -> Value {
    json!({ "type": "award", "id": id(name), "formatVersion": 1, "raid": "cm9xr2k4a0001",
        "env": env, "boss": "lb_lucifron", "item": 16805, "char": "Ana Vale",
        "method": "HR", "roll": 91, "sender": "Rae Quill", "at": 1792001200 })
}

fn id(name: &str) -> String {
    format!("aw-{name}-0000")
}

/// A store in memory, shared with the test so it can change a key or fail.
#[derive(Clone, Default)]
struct FakeStore {
    keys: Arc<Mutex<HashMap<Site, HelperKey>>>,
    failing: Arc<AtomicBool>,
}

impl FakeStore {
    fn put(&self, site: Site, text: &str) {
        self.keys.lock().unwrap().insert(site, key(text));
    }

    fn fail(&self, action: StoreAction) -> Result<(), StoreError> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(StoreError {
                action,
                problem: StoreProblem::Refused,
            });
        }
        Ok(())
    }
}

impl SecretStore for FakeStore {
    fn save(&self, site: Site, key: &HelperKey) -> Result<(), StoreError> {
        self.fail(StoreAction::Save)?;
        self.keys.lock().unwrap().insert(site, key.clone());
        Ok(())
    }
    fn read(&self, site: Site) -> Result<Option<HelperKey>, StoreError> {
        self.fail(StoreAction::Read)?;
        Ok(self.keys.lock().unwrap().get(&site).cloned())
    }
    fn delete(&self, site: Site) -> Result<(), StoreError> {
        self.fail(StoreAction::Delete)?;
        self.keys.lock().unwrap().remove(&site);
        Ok(())
    }
}

/// An upload as the site saw it: the site, the key and the ids.
type Upload = (Site, HelperKey, Vec<String>);

/// A site that records every upload (site, key and ids) and records each operation.
#[derive(Clone, Default)]
struct FakeSite {
    uploads: Arc<Mutex<Vec<Upload>>>,
}

impl FakeSite {
    fn uploads(&self) -> Vec<Upload> {
        self.uploads.lock().unwrap().clone()
    }
}

impl Uplink for FakeSite {
    fn post_operations(
        &self,
        site: Site,
        key: &HelperKey,
        body: &str,
    ) -> Result<UploadReply, String> {
        let body: Value = serde_json::from_str(body).unwrap();
        let ids: Vec<String> = body["operations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|op| op["id"].as_str().unwrap().to_string())
            .collect();
        let results: Vec<Value> = ids
            .iter()
            .map(|id| json!({ "id": id, "status": "recorded" }))
            .collect();
        self.uploads.lock().unwrap().push((site, key.clone(), ids));
        Ok(UploadReply {
            status: 200,
            retry_after: None,
            body: Some(json!({ "results": results })),
        })
    }
}

struct App {
    runner: Runner,
    engine: Engine<FakeStore>,
    store: FakeStore,
    site: FakeSite,
    queue: Arc<Mutex<Queue>>,
    heard: Arc<Mutex<Vec<SyncStatus>>>,
}

impl App {
    /// Starts the runner with `store`'s keys and a queue holding `ops`.
    fn start(store: FakeStore, ops: &[Value]) -> App {
        let dir = scratch_dir();
        let engine = Engine::new(KeyEngine::open(
            dir.join("settings.json"),
            store.clone(),
            SiteClient::default(),
        ));
        let mut queue = Queue::open(dir.join(FILE_NAME));
        for op in ops {
            queue.add(op.clone()).unwrap();
        }
        App::start_with(engine, store, Arc::new(Mutex::new(queue)))
    }

    fn start_with(engine: Engine<FakeStore>, store: FakeStore, queue: Arc<Mutex<Queue>>) -> App {
        let site = FakeSite::default();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let runner = {
            let heard = Arc::clone(&heard);
            Runner::start(engine.clone(), Arc::clone(&queue), site.clone(), move |s| {
                heard.lock().unwrap().push(s.clone())
            })
        };
        App {
            runner,
            engine,
            store,
            site,
            queue,
            heard,
        }
    }

    fn add(&self, op: Value) {
        self.queue.lock().unwrap().add(op).unwrap();
    }

    fn waiting(&self) -> usize {
        self.queue.lock().unwrap().operations().unwrap().len()
    }

    /// Waits until the status is `status`, failing after a few seconds.
    fn until(&self, status: SyncStatus) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.runner.status() != status {
            assert!(
                Instant::now() < deadline,
                "still {:?}",
                self.runner.status()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl App {
    fn until_sent(&self, uploads: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.site.uploads().len() < uploads {
            assert!(Instant::now() < deadline, "only {:?}", self.site.uploads());
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.runner.stop();
    }
}

const IDLE_STATUS: SyncStatus = SyncStatus::Idle { other_site: 0 };

fn sent(text: &str, site: Site, ids: &[&str]) -> Upload {
    (site, key(text), ids.iter().map(|name| id(name)).collect())
}

#[test]
fn with_a_stored_key_the_queue_drains_to_the_site() {
    let store = FakeStore::default();
    store.put(Site::Production, KEY);
    let app = App::start(
        store,
        &[award("a1", "production"), award("a2", "production")],
    );
    app.until_sent(1);
    app.until(IDLE_STATUS);
    assert_eq!(
        app.site.uploads(),
        vec![sent(KEY, Site::Production, &["a1", "a2"])]
    );
    assert_eq!(app.waiting(), 0);
}

#[test]
fn without_a_key_nothing_is_sent_and_the_status_says_so_until_one_is_stored() {
    let app = App::start(FakeStore::default(), &[award("a1", "production")]);
    app.until(SyncStatus::NoKey);
    std::thread::sleep(Duration::from_millis(50));
    assert!(app.site.uploads().is_empty());
    assert_eq!(app.waiting(), 1);

    app.store.put(Site::Production, KEY);
    app.runner.restart();
    app.until(IDLE_STATUS);
    assert_eq!(
        app.site.uploads(),
        vec![sent(KEY, Site::Production, &["a1"])]
    );
}

#[test]
fn switching_site_or_key_restarts_with_the_new_ones_and_never_mixes_them() {
    let store = FakeStore::default();
    store.put(Site::Production, KEY);
    store.put(Site::Staging, STAGING_KEY);
    let app = App::start(store, &[award("p1", "production"), award("s1", "staging")]);
    app.until(SyncStatus::Idle { other_site: 1 });

    assert_eq!(set_staging(&app.engine, true), ChangeOutcome::Done);
    app.runner.restart();
    app.until(IDLE_STATUS);

    app.store.put(Site::Staging, NEW_KEY);
    app.add(award("s2", "staging"));
    app.runner.restart();
    app.until_sent(3);
    app.until(IDLE_STATUS);

    // Removing the staging key stops sending there; production's key stays put.
    assert_eq!(remove_key(&app.engine), ChangeOutcome::Done);
    app.runner.restart();
    app.until(SyncStatus::NoKey);
    app.add(award("s3", "staging"));
    std::thread::sleep(Duration::from_millis(50));

    assert_eq!(
        app.site.uploads(),
        vec![
            sent(KEY, Site::Production, &["p1"]),
            sent(STAGING_KEY, Site::Staging, &["s1"]),
            sent(NEW_KEY, Site::Staging, &["s2"]),
        ]
    );
    assert_eq!(app.waiting(), 1);
}

#[test]
fn a_secure_store_failure_sends_nothing_and_is_reported() {
    let store = FakeStore::default();
    store.put(Site::Production, KEY);
    store.failing.store(true, Ordering::SeqCst);
    let app = App::start(store, &[award("a1", "production")]);
    let message = StoreError {
        action: StoreAction::Read,
        problem: StoreProblem::Refused,
    }
    .to_string();
    app.until(SyncStatus::KeyUnreadable { message });
    assert!(app.site.uploads().is_empty());

    app.store.failing.store(false, Ordering::SeqCst);
    app.runner.restart();
    app.until(IDLE_STATUS);
    assert_eq!(app.waiting(), 0);
}

#[test]
fn a_damaged_queue_is_reported_and_picked_up_once_fixed() {
    let dir = scratch_dir();
    let path = dir.join(FILE_NAME);
    std::fs::write(&path, "not a list").unwrap();
    let store = FakeStore::default();
    store.put(Site::Production, KEY);
    let engine = Engine::new(KeyEngine::open(
        dir.join("settings.json"),
        store.clone(),
        SiteClient::default(),
    ));
    let queue = Arc::new(Mutex::new(Queue::open(&path)));
    let app = App::start_with(engine, store, queue);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !matches!(app.runner.status(), SyncStatus::QueueDamaged { .. }) {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "not a list");

    let file = json!({ "formatVersion": 1, "operations": [award("a1", "production")] });
    std::fs::write(&path, file.to_string()).unwrap();
    app.runner.restart();
    app.until(IDLE_STATUS);
    assert_eq!(
        app.site.uploads(),
        vec![sent(KEY, Site::Production, &["a1"])]
    );
}

#[test]
fn stopping_ends_it_promptly_while_it_waits_and_nothing_is_sent_after() {
    let store = FakeStore::default();
    store.put(Site::Production, KEY);
    let app = App::start(store, &[]);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.heard.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }

    // The first pass is done, and it now waits 30 seconds before the next pass.
    let asked = Instant::now();
    app.runner.stop();
    app.runner.join();
    assert!(asked.elapsed() < Duration::from_secs(1));

    app.add(award("a1", "production"));
    app.runner.restart();
    std::thread::sleep(Duration::from_millis(50));
    assert!(app.site.uploads().is_empty());
    assert_eq!(app.waiting(), 1);
}

#[test]
fn a_pass_under_way_sends_nothing_more_once_restarted_or_stopped() {
    let shared = Shared::default();
    let site = FakeSite::default();
    let guarded = Guarded {
        uplink: &site,
        shared: &shared,
        generation: 0,
    };
    let body = json!({ "operations": [award("a1", "production")] }).to_string();
    assert!(guarded
        .post_operations(Site::Production, &key(KEY), &body)
        .is_ok());

    shared.control().generation += 1;
    assert!(guarded
        .post_operations(Site::Production, &key(KEY), &body)
        .is_err());
    let current = Guarded {
        generation: 1,
        ..guarded
    };
    shared.control().stopped = true;
    assert!(current
        .post_operations(Site::Production, &key(KEY), &body)
        .is_err());
    assert_eq!(site.uploads().len(), 1);
}

/// A site that holds each answer until the test lets it go.
struct HeldSite {
    site: FakeSite,
    asked: Arc<Mutex<bool>>,
    release: Arc<(Mutex<bool>, Condvar)>,
}

impl Uplink for HeldSite {
    fn post_operations(
        &self,
        site: Site,
        key: &HelperKey,
        body: &str,
    ) -> Result<UploadReply, String> {
        *self.asked.lock().unwrap() = true;
        let (released, go) = &*self.release;
        let mut released = released.lock().unwrap();
        while !*released {
            released = go.wait(released).unwrap();
        }
        self.site.post_operations(site, key, body)
    }
}

#[test]
fn stopping_while_a_request_is_out_keeps_its_record_for_next_time() {
    let dir = scratch_dir();
    let store = FakeStore::default();
    store.put(Site::Production, KEY);
    let engine = Engine::new(KeyEngine::open(
        dir.join("settings.json"),
        store,
        SiteClient::default(),
    ));
    let mut queue = Queue::open(dir.join(FILE_NAME));
    queue.add(award("a1", "production")).unwrap();
    let queue = Arc::new(Mutex::new(queue));
    let site = HeldSite {
        site: FakeSite::default(),
        asked: Arc::default(),
        release: Arc::default(),
    };
    let (asked, release, uploads) = (
        Arc::clone(&site.asked),
        Arc::clone(&site.release),
        site.site.clone(),
    );
    let runner = Runner::start(engine, Arc::clone(&queue), site, |_| {});
    let deadline = Instant::now() + Duration::from_secs(5);
    while !*asked.lock().unwrap() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }

    runner.stop();
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    runner.join();

    // The site recorded it, but the answer came after Quit: it stays to be sent again.
    assert_eq!(
        uploads.uploads(),
        vec![sent(KEY, Site::Production, &["a1"])]
    );
    assert_eq!(queue.lock().unwrap().operations().unwrap().len(), 1);
}

#[test]
fn every_status_change_is_passed_on_once() {
    let app = App::start(FakeStore::default(), &[award("a1", "production")]);
    app.until(SyncStatus::NoKey);
    app.store.put(Site::Production, KEY);
    app.runner.restart();
    app.until(IDLE_STATUS);
    app.runner.restart();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        *app.heard.lock().unwrap(),
        vec![
            SyncStatus::NoKey,
            SyncStatus::Sending {
                raid: "cm9xr2k4a0001".into()
            },
            IDLE_STATUS,
        ]
    );
}

#[test]
fn the_new_statuses_are_plain_data_for_the_window() {
    assert_eq!(
        serde_json::to_value(SyncStatus::NoKey).unwrap(),
        json!({ "kind": "noKey" })
    );
    assert_eq!(
        serde_json::to_value(SyncStatus::KeyUnreadable {
            message: "m".into()
        })
        .unwrap(),
        json!({ "kind": "keyUnreadable", "message": "m" })
    );
}
