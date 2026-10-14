use super::*;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const KEY: &str = "bfh_q7Xk2mP9vR4tW8yZ1aB3cD5eF6gH0iJ-kL_nO2pQ4sA";
const OTHER_KEY: &str = "bfh_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

const ME: &str = r#"{"officer":"Aldric","env":"staging","formatVersions":[1],
    "addon":{"version":"0.1.0","url":"https://github.com/robpaolella/bureaucracy-forever-addon/releases/latest"}}"#;
const LOGIN_PAGE: &str = "<!doctype html><title>Login – Vercel</title>";

/// A fresh, empty folder for one test.
fn scratch_dir() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-key-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// What a fake site answers.
#[derive(Clone)]
enum Answer {
    /// The status line's rest (`"200 OK"`), extra header lines, and the body.
    With(&'static str, &'static str, &'static str),
    /// A 500 whose body is the request it got, headers and all.
    EchoRequest,
    /// Takes the connection and never answers.
    Silent,
    /// Sends a status line and headers promising a body, then never sends it.
    Stall(&'static str, &'static str),
}

fn json(status: &'static str, body: &'static str) -> Answer {
    Answer::With(status, "Content-Type: application/json\r\n", body)
}

/// A site on this machine that gives every request the same answer and keeps each
/// request's head (request line and headers).
struct FakeSite {
    address: String,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeSite {
    fn start(answer: Answer) -> FakeSite {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&requests);
        std::thread::spawn(move || {
            let mut held = Vec::new();
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let head = read_head(&mut stream);
                seen.lock().unwrap().push(head.clone());
                let (status, headers, body) = match &answer {
                    Answer::With(s, h, b) => (*s, *h, b.to_string()),
                    Answer::EchoRequest => ("500 Internal Server Error", "", head),
                    Answer::Silent => {
                        held.push(stream);
                        continue;
                    }
                    Answer::Stall(status, headers) => {
                        let head =
                            format!("HTTP/1.1 {status}\r\n{headers}Content-Length: 100\r\n\r\n");
                        let _ = stream.write_all(head.as_bytes());
                        held.push(stream);
                        continue;
                    }
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        FakeSite { address, requests }
    }

    fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

fn read_head(stream: &mut TcpStream) -> String {
    let mut head = Vec::new();
    let mut byte = [0u8];
    while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
        head.push(byte[0]);
    }
    String::from_utf8_lossy(&head).into_owned()
}

/// An address where nothing answers.
fn closed_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

fn client(production: &str, staging: &str) -> SiteClient {
    SiteClient::with_addresses(production, staging, Duration::from_millis(500))
}

fn key(text: &str) -> HelperKey {
    HelperKey::parse(text).unwrap()
}

/// Asks a fake site giving `answer` about `KEY`, as `site`.
fn ask(site: Site, answer: Answer) -> Result<Connection, SiteError> {
    let fake = FakeSite::start(answer);
    let result = client(&fake.address, &fake.address).check(site, &key(KEY));
    assert_eq!(fake.requests().len(), 1);
    result
}

fn unreachable(result: Result<Connection, SiteError>) -> bool {
    matches!(result, Err(SiteError::Unreachable { .. }))
}

/// A store in memory that can be told to fail.
#[derive(Default)]
struct FakeStore {
    keys: Mutex<HashMap<Site, HelperKey>>,
    fail: Option<StoreProblem>,
}

impl FakeStore {
    fn failing(problem: StoreProblem) -> FakeStore {
        FakeStore {
            fail: Some(problem),
            ..FakeStore::default()
        }
    }

    fn fail(&self, action: StoreAction) -> Result<(), StoreError> {
        match self.fail {
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

struct Setup {
    dir: PathBuf,
    production: FakeSite,
    staging: FakeSite,
}

impl Setup {
    fn new(production: Answer, staging: Answer) -> Setup {
        Setup {
            dir: scratch_dir(),
            production: FakeSite::start(production),
            staging: FakeSite::start(staging),
        }
    }

    fn engine<S: SecretStore>(&self, store: S) -> KeyEngine<S> {
        KeyEngine::open(
            self.dir.join(SETTINGS_FILE),
            store,
            client(&self.production.address, &self.staging.address),
        )
    }
}

/// Every file's contents in `dir`.
fn all_files(dir: &Path) -> String {
    fs::read_dir(dir)
        .unwrap()
        .map(|e| fs::read_to_string(e.unwrap().path()).unwrap())
        .collect()
}

#[test]
fn key_shape_is_checked() {
    assert_eq!(HelperKey::parse(""), Err(ShapeError::Empty));
    assert_eq!(HelperKey::parse(" \n\t"), Err(ShapeError::Empty));
    for bad in [
        "my key from the site",
        &KEY[4..], // no prefix
        &KEY.replace("bfh_", "BFH_"),
        &KEY[..KEY.len() - 1],  // 42 characters
        &format!("{KEY}A"),     // 44 characters
        &KEY.replace('-', "+"), // base64, not base64url
        &format!("{KEY}="),     // padded
        &KEY.replace('q', " "),
    ] {
        assert_eq!(HelperKey::parse(bad), Err(ShapeError::Malformed), "{bad}");
    }
    assert_eq!(HelperKey::parse(&format!("  {KEY}\n")), Ok(key(KEY)));
    assert_eq!(key(KEY).secret(), KEY);
}

#[test]
fn a_badly_shaped_key_is_refused_without_asking_the_site() {
    let setup = Setup::new(json("200 OK", ME), json("200 OK", ME));
    let engine = setup.engine(FakeStore::default());
    for (pasted, error) in [
        ("", ShapeError::Empty),
        ("   ", ShapeError::Empty),
        ("bfh_short", ShapeError::Malformed),
    ] {
        assert_eq!(engine.connect(pasted), Err(ConnectError::Shape(error)));
    }
    assert_eq!(
        ShapeError::Malformed.to_string(),
        "That doesn’t look like a helper key. Copy it again from the officer area."
    );
    assert!(setup.production.requests().is_empty());
    assert!(engine.store.keys.lock().unwrap().is_empty());
}

#[test]
fn an_accepted_key_gives_the_officer_and_the_site_details() {
    let fake = FakeSite::start(json("200 OK", ME));
    let connection = client(&fake.address, &closed_address())
        .check(Site::Production, &key(KEY))
        .unwrap();
    assert_eq!(
        connection,
        Connection {
            officer: "Aldric".into(),
            env: "staging".into(),
            format_versions: vec![1],
            addon_version: "0.1.0".into(),
            addon_url: "https://github.com/robpaolella/bureaucracy-forever-addon/releases/latest"
                .into(),
        }
    );
    let requests = fake.requests();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].starts_with("GET /api/helper/me HTTP/1.1\r\n"),
        "{}",
        requests[0]
    );
    let head = requests[0].to_ascii_lowercase();
    assert!(head.contains(&format!(
        "authorization: bearer {}\r\n",
        KEY.to_ascii_lowercase()
    )));
}

#[test]
fn a_refused_key_is_told_apart_by_status() {
    let bad_key = json(
        "401 Unauthorized",
        r#"{"error":"That key isn't valid.","code":"bad-key"}"#,
    );
    let not_officer = json(
        "403 Forbidden",
        r#"{"error":"Officers only.","code":"not-officer"}"#,
    );
    for site in [Site::Production, Site::Staging] {
        assert_eq!(
            ask(site, bad_key.clone()),
            Err(SiteError::KeyRefused(Refusal::BadKey))
        );
        assert_eq!(
            ask(site, not_officer.clone()),
            Err(SiteError::KeyRefused(Refusal::NotOfficer))
        );
    }
    assert_eq!(
        SiteError::KeyRefused(Refusal::NotOfficer).to_string(),
        "This key has been revoked or no longer works. Create a new key in the officer area."
    );
}

#[test]
fn a_switched_off_site_answers_the_same_for_any_key() {
    let off = json(
        "503 Service Unavailable",
        r#"{"error":"The helper connection isn't switched on for this site yet.","code":"helper-off"}"#,
    );
    let setup = Setup::new(off.clone(), off);
    let engine = setup.engine(FakeStore::default());
    for pasted in [KEY, OTHER_KEY] {
        assert_eq!(
            engine.connect(pasted),
            Err(ConnectError::Site(SiteError::HelperOff))
        );
    }
    // Without a well-shaped key nothing is asked, so the shape is what's reported.
    assert_eq!(
        engine.connect("nope"),
        Err(ConnectError::Shape(ShapeError::Malformed))
    );
    assert_eq!(engine.check_stored(), Err(ConnectError::NoKey));
    assert_eq!(setup.production.requests().len(), 2);
    assert!(engine.store.keys.lock().unwrap().is_empty());
    assert_eq!(
        SiteError::HelperOff.to_string(),
        "The guild site hasn’t switched the helper on yet. Try again later."
    );
}

#[test]
fn anything_else_is_an_unreachable_site() {
    for answer in [
        json(
            "500 Internal Server Error",
            r#"{"error":"Oops.","code":"server-error"}"#,
        ),
        Answer::With(
            "502 Bad Gateway",
            "Content-Type: text/html\r\n",
            "<h1>Bad gateway</h1>",
        ),
        json(
            "503 Service Unavailable",
            r#"{"error":"Down for maintenance.","code":"maintenance"}"#,
        ),
        Answer::With("503 Service Unavailable", "", "Service Unavailable"),
        json(
            "429 Too Many Requests",
            r#"{"error":"Slow down.","code":"rate-limited"}"#,
        ),
        json(
            "413 Payload Too Large",
            r#"{"error":"Too large.","code":"too-large"}"#,
        ),
        json(
            "404 Not Found",
            r#"{"error":"Not found.","code":"not-found"}"#,
        ),
        json("200 OK", r#"{"officer":"Aldric"}"#),
        json("200 OK", "not json"),
        json("204 No Content", ""),
        Answer::Silent,
        // The production site's own answer before it has the helper API.
        json("401 Unauthorized", r#"{"error":"Unauthorised."}"#),
        Answer::Stall("401 Unauthorized", "Content-Type: text/html\r\n"),
        Answer::Stall("200 OK", "Content-Type: application/json\r\n"),
    ] {
        for site in [Site::Production, Site::Staging] {
            assert!(unreachable(ask(site, answer.clone())));
        }
    }
    let nobody = client(&closed_address(), &closed_address()).check(Site::Production, &key(KEY));
    assert!(unreachable(nobody));
    assert_eq!(
        SiteError::Unreachable {
            detail: String::new()
        }
        .to_string(),
        "Couldn’t reach the guild site. Check your internet connection and try again."
    );
}

#[test]
fn vercels_login_page_means_staging_isnt_open_yet() {
    let login = Answer::With(
        "401 Unauthorized",
        "Content-Type: text/html\r\n",
        LOGIN_PAGE,
    );
    let redirect = Answer::With(
        "302 Found",
        "Location: https://vercel.com/sso-api?url=https%3A%2F%2Fstaging.bureauguild.com\r\n",
        "",
    );
    for answer in [login, redirect] {
        assert_eq!(
            ask(Site::Staging, answer.clone()),
            Err(SiteError::StagingNotOpen)
        );
        assert!(unreachable(ask(Site::Production, answer)));
    }
    assert_eq!(
        SiteError::StagingNotOpen.to_string(),
        "The staging site isn't open yet."
    );
}

#[test]
fn the_staging_setting_picks_the_site_and_is_remembered() {
    let setup = Setup::new(json("200 OK", ME), json("200 OK", ME));
    let mut engine = setup.engine(FakeStore::default());
    assert_eq!(engine.site(), Site::Production);
    engine.connect(KEY).unwrap();
    assert_eq!(setup.production.requests().len(), 1);
    assert!(setup.staging.requests().is_empty());

    engine.set_staging(true).unwrap();
    assert_eq!(engine.site(), Site::Staging);
    let engine = setup.engine(FakeStore::default());
    assert_eq!(engine.site(), Site::Staging);
    engine.connect(OTHER_KEY).unwrap();
    assert_eq!(setup.staging.requests().len(), 1);
    assert_eq!(setup.production.requests().len(), 1);

    let mut engine = engine;
    engine.set_staging(false).unwrap();
    assert_eq!(setup.engine(FakeStore::default()).site(), Site::Production);
    assert_eq!(Site::Production.address(), "https://www.bureauguild.com");
    assert_eq!(Site::Staging.address(), "https://staging.bureauguild.com");
}

#[test]
fn each_site_has_its_own_key_and_never_gets_the_others() {
    let setup = Setup::new(json("200 OK", ME), json("200 OK", ME));
    let mut engine = setup.engine(FakeStore::default());
    engine.connect(KEY).unwrap();

    // Switching to staging carries nothing over.
    engine.set_staging(true).unwrap();
    assert_eq!(engine.check_stored(), Err(ConnectError::NoKey));
    assert!(setup.staging.requests().is_empty());
    engine.connect(OTHER_KEY).unwrap();
    engine.check_stored().unwrap();

    // Removing staging's key keeps production's.
    engine.remove_key().unwrap();
    assert_eq!(engine.check_stored(), Err(ConnectError::NoKey));
    engine.set_staging(false).unwrap();
    engine.check_stored().unwrap();

    let production = setup.production.requests().join("");
    let staging = setup.staging.requests().join("");
    assert_eq!(setup.production.requests().len(), 2);
    assert_eq!(setup.staging.requests().len(), 2);
    assert!(production.contains(KEY) && !production.contains(OTHER_KEY));
    assert!(staging.contains(OTHER_KEY) && !staging.contains(KEY));
}

#[test]
fn the_stored_key_is_kept_whatever_the_site_says() {
    let refused = json(
        "401 Unauthorized",
        r#"{"error":"That key isn't valid.","code":"bad-key"}"#,
    );
    for answer in [
        refused,
        json("500 Internal Server Error", "{}"),
        Answer::Silent,
    ] {
        let setup = Setup::new(answer, Answer::Silent);
        let engine = setup.engine(FakeStore::default());
        engine.store.save(Site::Production, &key(KEY)).unwrap();
        assert!(matches!(engine.check_stored(), Err(ConnectError::Site(_))));
        // A new key the site doesn't accept isn't stored, and the old one stays.
        assert!(matches!(
            engine.connect(OTHER_KEY),
            Err(ConnectError::Site(_))
        ));
        assert_eq!(engine.store.read(Site::Production), Ok(Some(key(KEY))));
    }
}

#[test]
fn the_key_never_appears_in_settings_or_messages() {
    let setup = Setup::new(Answer::EchoRequest, Answer::EchoRequest);
    let mut engine = setup.engine(FakeStore::default());
    engine.set_staging(true).unwrap();
    engine.store.save(Site::Staging, &key(KEY)).unwrap();

    // The fake site sends the request, key included, back in its body.
    let failed = engine.check_stored().unwrap_err();
    assert!(setup.staging.requests()[0].contains(KEY));
    let failed_new = engine.connect(OTHER_KEY).unwrap_err();
    let nobody = client(&closed_address(), &closed_address()).check(Site::Staging, &key(KEY));
    for output in [
        format!("{failed} {failed:?}"),
        format!("{failed_new} {failed_new:?}"),
        format!("{nobody:?}"),
        format!("{:?}", key(KEY)),
        format!("{:?}", engine.store.read(Site::Staging)),
    ] {
        assert!(
            !output.contains(&KEY[4..]) && !output.contains(&OTHER_KEY[4..]),
            "{output}"
        );
    }
    let files = all_files(&setup.dir);
    assert!(files.contains("\"staging\": true"), "{files}");
    assert!(
        !files.contains(&KEY[4..]) && !files.contains(&OTHER_KEY[4..]),
        "{files}"
    );
}

#[test]
fn a_failing_store_gives_plain_words_and_keeps_the_key_nowhere() {
    let setup = Setup::new(json("200 OK", ME), json("200 OK", ME));
    let engine = setup.engine(FakeStore::failing(StoreProblem::Refused));
    let error = engine.connect(KEY).unwrap_err();
    assert_eq!(
        error,
        ConnectError::Store(StoreError {
            action: StoreAction::Save,
            problem: StoreProblem::Refused,
        })
    );
    let message = error.to_string();
    assert!(
        message.starts_with("The helper couldn’t save your key in "),
        "{message}"
    );
    assert!(message.ends_with("then try again."), "{message}");
    assert!(!message.contains(&KEY[4..]) && !format!("{error:?}").contains(&KEY[4..]));
    assert!(engine.store.keys.lock().unwrap().is_empty());
    assert!(!all_files(&setup.dir).contains(&KEY[4..]));

    let unavailable = setup.engine(FakeStore::failing(StoreProblem::Unavailable));
    assert_eq!(
        unavailable.check_stored().unwrap_err().to_string(),
        format!(
            "The helper couldn’t read your key from {}. It isn’t available on this computer.",
            store_name()
        )
    );
    assert!(setup.production.requests().len() == 1);
}

fn store_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "Keychain"
    } else if cfg!(target_os = "windows") {
        "Credential Manager"
    } else {
        "the system’s secure store"
    }
}

#[test]
fn an_unreadable_settings_file_means_production_and_other_settings_are_kept() {
    let dir = scratch_dir();
    let path = dir.join(SETTINGS_FILE);
    for damaged in [
        "",
        "{",
        "[]",
        r#"{"formatVersion":2,"staging":true}"#,
        r#"{"staging":true}"#,
    ] {
        fs::write(&path, damaged).unwrap();
        assert!(!settings::read_staging(&path), "{damaged}");
    }
    fs::write(
        &path,
        r#"{"formatVersion":1,"staging":false,"gameFolder":"C:\\WoW"}"#,
    )
    .unwrap();
    settings::write_staging(&path, true).unwrap();
    assert!(settings::read_staging(&path));
    assert!(fs::read_to_string(&path)
        .unwrap()
        .contains(r#""gameFolder": "C:\\WoW""#));
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn without_a_secure_store_the_system_store_says_so() {
    let store = SystemStore::default();
    let error = store.save(Site::Production, &key(KEY)).unwrap_err();
    assert_eq!(error.problem, StoreProblem::Unavailable);
    assert_eq!(
        store.read(Site::Production).unwrap_err().problem,
        StoreProblem::Unavailable
    );
}

#[test]
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows")),
    ignore = "the real secure store is tested only on a Mac or Windows PC"
)]
fn the_real_store_saves_reads_and_removes_each_sites_key() {
    let store = SystemStore::with_service("com.robpaolella.bureaucracy-forever-helper.test");
    let _ = store.delete(Site::Production);
    let _ = store.delete(Site::Staging);
    store.save(Site::Production, &key(KEY)).unwrap();
    store.save(Site::Staging, &key(OTHER_KEY)).unwrap();
    assert_eq!(store.read(Site::Production), Ok(Some(key(KEY))));
    assert_eq!(store.read(Site::Staging), Ok(Some(key(OTHER_KEY))));
    store.save(Site::Production, &key(OTHER_KEY)).unwrap();
    assert_eq!(store.read(Site::Production), Ok(Some(key(OTHER_KEY))));
    store.delete(Site::Production).unwrap();
    store.delete(Site::Production).unwrap();
    assert_eq!(store.read(Site::Production), Ok(None));
    assert_eq!(store.read(Site::Staging), Ok(Some(key(OTHER_KEY))));
    store.delete(Site::Staging).unwrap();
}
