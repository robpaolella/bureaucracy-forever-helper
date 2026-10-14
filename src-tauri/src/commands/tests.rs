use super::*;
use crate::key::{HelperKey, Site, SiteClient, StoreAction, StoreError, StoreProblem};
use serde_json::json;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const KEY: &str = "bfh_q7Xk2mP9vR4tW8yZ1aB3cD5eF6gH0iJ-kL_nO2pQ4sA";
const ME: &str = r#"{"officer":"Aldric","env":"production","formatVersions":[1],
    "addon":{"version":"0.1.0","url":"https://example.test/addon"}}"#;

/// An in-memory store that can be made to fail.
#[derive(Default)]
struct FakeStore {
    keys: Mutex<HashMap<Site, HelperKey>>,
    failing: Option<StoreProblem>,
}

impl FakeStore {
    fn with_key() -> Self {
        let store = FakeStore::default();
        store
            .keys
            .lock()
            .unwrap()
            .insert(Site::Production, HelperKey::parse(KEY).unwrap());
        store
    }

    fn failing() -> Self {
        FakeStore {
            failing: Some(StoreProblem::Unavailable),
            ..FakeStore::default()
        }
    }

    fn fail(&self, action: StoreAction) -> Result<(), StoreError> {
        match self.failing {
            Some(problem) => Err(StoreError { action, problem }),
            None => Ok(()),
        }
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

/// A site on this machine that answers every request the same way.
fn site(status: &'static str, headers: &'static str, body: &'static str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let response = format!(
                "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    address
}

fn json_site(status: &'static str, body: &'static str) -> String {
    site(status, "Content-Type: application/json\r\n", body)
}

fn settings_path() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-commands-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("settings.json")
}

fn engine(production: &str, staging: &str, store: FakeStore) -> Engine<FakeStore> {
    let client = SiteClient::with_addresses(production, staging, Duration::from_secs(5));
    Engine::new(KeyEngine::open(settings_path(), store, client))
}

fn production(address: &str, store: FakeStore) -> Engine<FakeStore> {
    engine(address, "http://127.0.0.1:1", store)
}

fn kind(outcome: &KeyOutcome) -> String {
    serde_json::to_value(outcome).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn connecting_with_an_accepted_key_gives_the_officer_and_stores_the_key() {
    let engine = production(&json_site("200 OK", ME), FakeStore::default());
    let outcome = connect(&engine, KEY);
    assert_eq!(
        serde_json::to_value(&outcome).unwrap(),
        json!({
            "kind": "accepted",
            "officer": "Aldric",
            "env": "production",
            "formatVersions": [1],
            "addonVersion": "0.1.0",
            "addonUrl": "https://example.test/addon",
        })
    );
    // The key is now stored, so a check of the stored key is accepted too.
    assert_eq!(kind(&check_stored(&engine)), "accepted");
}

#[test]
fn each_refusal_is_told_apart() {
    let bad = production(
        &json_site("401 Unauthorized", r#"{"error":"no","code":"bad-key"}"#),
        FakeStore::default(),
    );
    let outcome = connect(&bad, KEY);
    assert_eq!(kind(&outcome), "refused");
    assert!(matches!(
        outcome,
        KeyOutcome::Refused {
            reason: "badKey",
            ..
        }
    ));

    let not_officer = production(
        &json_site("403 Forbidden", r#"{"error":"no","code":"not-officer"}"#),
        FakeStore::default(),
    );
    assert!(matches!(
        connect(&not_officer, KEY),
        KeyOutcome::Refused {
            reason: "notOfficer",
            ..
        }
    ));
}

#[test]
fn a_switched_off_site_is_site_off() {
    let engine = production(
        &json_site(
            "503 Service Unavailable",
            r#"{"error":"off","code":"helper-off"}"#,
        ),
        FakeStore::default(),
    );
    assert_eq!(kind(&connect(&engine, KEY)), "siteOff");
}

#[test]
fn a_staging_site_behind_a_login_page_is_not_open() {
    let staging = site(
        "401 Unauthorized",
        "Content-Type: text/html\r\n",
        "<title>Login</title>",
    );
    let engine = engine("http://127.0.0.1:1", &staging, FakeStore::default());
    engine.write().set_staging(true).unwrap();
    assert_eq!(kind(&connect(&engine, KEY)), "stagingNotOpen");
}

#[test]
fn a_site_that_cannot_be_reached_is_unreachable_and_keeps_the_stored_key() {
    let engine = production("http://127.0.0.1:1", FakeStore::with_key());
    let outcome = check_stored(&engine);
    assert_eq!(kind(&outcome), "unreachable");
    // Nothing was removed.
    assert_eq!(remove_key(&engine), ChangeOutcome::Done);
}

#[test]
fn a_wrongly_shaped_key_is_malformed_and_the_site_is_not_asked() {
    // Nothing listens at this address: asking the site would say unreachable.
    let engine = production("http://127.0.0.1:1", FakeStore::default());
    assert!(matches!(
        connect(&engine, "   "),
        KeyOutcome::Malformed {
            reason: "empty",
            ..
        }
    ));
    assert!(matches!(
        connect(&engine, "my key from the site"),
        KeyOutcome::Malformed {
            reason: "malformed",
            ..
        }
    ));
}

#[test]
fn a_failing_store_is_store_failed_on_connect_and_on_check() {
    let engine = production(&json_site("200 OK", ME), FakeStore::failing());
    assert_eq!(kind(&connect(&engine, KEY)), "storeFailed");
    assert_eq!(kind(&check_stored(&engine)), "storeFailed");
}

#[test]
fn checking_with_no_stored_key_is_no_key() {
    let engine = production("http://127.0.0.1:1", FakeStore::default());
    assert_eq!(check_stored(&engine), KeyOutcome::NoKey);
}

#[test]
fn removing_a_key_removes_it_and_reports_a_store_failure() {
    let engine = production(&json_site("200 OK", ME), FakeStore::with_key());
    assert_eq!(remove_key(&engine), ChangeOutcome::Done);
    assert_eq!(check_stored(&engine), KeyOutcome::NoKey);

    let failing = production("http://127.0.0.1:1", FakeStore::failing());
    assert!(matches!(
        remove_key(&failing),
        ChangeOutcome::StoreFailed { .. }
    ));
}

#[test]
fn the_staging_setting_is_read_and_changed() {
    let engine = production("http://127.0.0.1:1", FakeStore::default());
    assert!(!staging(&engine));
    assert_eq!(set_staging(&engine, true), ChangeOutcome::Done);
    assert!(staging(&engine));
    assert_eq!(set_staging(&engine, false), ChangeOutcome::Done);
    assert!(!staging(&engine));
}

#[test]
fn a_settings_file_that_cannot_be_written_is_reported_and_the_site_unchanged() {
    let client = SiteClient::with_addresses(
        "http://127.0.0.1:1",
        "http://127.0.0.1:1",
        Duration::from_secs(1),
    );
    let missing = settings_path()
        .parent()
        .unwrap()
        .join("gone")
        .join("settings.json");
    let engine = Engine::new(KeyEngine::open(missing, FakeStore::default(), client));
    assert!(matches!(
        set_staging(&engine, true),
        ChangeOutcome::SettingsFailed { .. }
    ));
    assert!(!staging(&engine));
}

#[test]
fn results_never_hold_the_key() {
    let engine = production(&json_site("200 OK", ME), FakeStore::failing());
    let text = serde_json::to_string(&connect(&engine, KEY)).unwrap();
    assert!(!text.contains(KEY));
}
