//! Every test runs the real updater against a local server, with a throwaway key pair made
//! for the test. The helper's real key and GitHub are never used.

use super::*;
use crate::commands::Engine;
use crate::key::{HelperKey, KeyEngine, SecretStore, Site, SiteClient, StoreError, UploadReply};
use crate::queue::{Queue, FILE_NAME};
use crate::upload::{Runner, Uplink};
use base64::Engine as _;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use tauri::test::{mock_builder, mock_context, noop_assets, MockRuntime};
use tauri::App;
use tauri_plugin_updater::UpdaterExt;

/// The mock app's version is 0.1.0.
const NEWER: &str = "0.2.0";
const FILE: &[u8] = b"the new helper, packed for the updater";

fn base64(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text)
}

struct Keys(minisign::KeyPair);

impl Keys {
    fn new() -> Keys {
        Keys(minisign::KeyPair::generate_unencrypted_keypair().unwrap())
    }

    /// The public key as the updater config holds it.
    fn public(&self) -> String {
        base64(&self.0.pk.to_box().unwrap().to_string())
    }

    /// A `.sig` as the Tauri bundler writes it, naming `version` when given.
    fn sign(&self, data: &[u8], version: Option<&str>) -> String {
        let comment = match version {
            Some(v) => format!("timestamp:1792001200\tfile:helper.tar.gz\tversion:{v}"),
            None => "timestamp:1792001200\tfile:helper.tar.gz".to_string(),
        };
        let signature =
            minisign::sign(Some(&self.0.pk), &self.0.sk, data, Some(&comment), None).unwrap();
        base64(&signature.to_string())
    }
}

/// How the local server answers one path.
#[derive(Clone)]
enum Answer {
    Json(Value),
    Bytes(Vec<u8>),
    Status(&'static str),
    /// Promises the whole file, sends part of it and hangs up.
    CutOff,
    /// Sends half the file, then the rest once the test says so.
    Held(Arc<Mutex<Receiver<()>>>),
}

/// A server on this machine; counts the requests for each path.
struct Server {
    address: String,
    hits: Arc<Mutex<HashMap<String, usize>>>,
}

impl Server {
    fn start(answers: HashMap<&'static str, Answer>) -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let hits = Arc::new(Mutex::new(HashMap::new()));
        let counted = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (answers, hits) = (answers.clone(), Arc::clone(&counted));
                std::thread::spawn(move || serve(stream, &answers, &hits));
            }
        });
        Server { address, hits }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.address)
    }

    fn hits(&self, path: &str) -> usize {
        self.hits.lock().unwrap().get(path).copied().unwrap_or(0)
    }
}

fn serve(
    mut stream: TcpStream,
    answers: &HashMap<&'static str, Answer>,
    hits: &Mutex<HashMap<String, usize>>,
) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let path = line.split(' ').nth(1).unwrap_or("").to_string();
    loop {
        let mut header = String::new();
        if reader.read_line(&mut header).unwrap() <= 2 {
            break;
        }
    }
    *hits.lock().unwrap().entry(path.clone()).or_default() += 1;
    let head = |status: &str, length: usize| {
        format!("HTTP/1.1 {status}\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n")
    };
    let _ = match answers.get(path.as_str()) {
        None => stream.write_all(head("404 Not Found", 0).as_bytes()),
        Some(Answer::Status(status)) => stream.write_all(head(status, 0).as_bytes()),
        Some(Answer::Json(body)) => {
            let body = body.to_string();
            stream.write_all(format!("{}{body}", head("200 OK", body.len())).as_bytes())
        }
        Some(Answer::Bytes(body)) => stream
            .write_all(head("200 OK", body.len()).as_bytes())
            .and_then(|()| stream.write_all(body)),
        Some(Answer::CutOff) => stream
            .write_all(head("200 OK", FILE.len()).as_bytes())
            .and_then(|()| stream.write_all(&FILE[..10])),
        Some(Answer::Held(go)) => {
            let half = FILE.len() / 2;
            let _ = stream.write_all(head("200 OK", FILE.len()).as_bytes());
            let _ = stream.write_all(&FILE[..half]);
            let _ = stream.flush();
            let _ = go.lock().unwrap().recv();
            stream.write_all(&FILE[half..])
        }
    };
}

/// The update manifest for `version`, its file at `url` with `signature`.
fn manifest(version: &str, url: &str, signature: &str) -> Value {
    json!({ "version": version, "notes": "What's new", "pub_date": "2026-10-15T00:00:00Z",
        "platforms": { "test": { "url": url, "signature": signature } } })
}

/// A server with a manifest for a newer version, and its file signed by `keys`.
fn release(keys: &Keys, file: Answer) -> Server {
    release_signed(&keys.sign(FILE, Some(NEWER)), file)
}

fn release_signed(signature: &str, file: Answer) -> Server {
    // The manifest names the file's address, which is only known once the server runs.
    let listener = Server::start(HashMap::from([("/helper.tar.gz", file)]));
    let manifest = manifest(NEWER, &listener.url("/helper.tar.gz"), signature);
    let server = Server::start(HashMap::from([("/latest.json", Answer::Json(manifest))]));
    Server {
        address: server.address,
        hits: listener.hits,
    }
}

/// The app at version 0.1.0 with the updater set up to trust `public_key` and read `endpoint`.
fn app(public_key: &str, endpoint: &str) -> App<MockRuntime> {
    let mut context = mock_context(noop_assets());
    context.config_mut().plugins.0.insert(
        "updater".into(),
        json!({ "pubkey": public_key, "endpoints": [endpoint], "requireSignedVersion": true }),
    );
    mock_builder()
        .plugin(tauri_plugin_updater::Builder::new().target("test").build())
        .build(context)
        .unwrap()
}

fn updater(app: &App<MockRuntime>) -> Result<Updater, Error> {
    configure(app.handle().updater_builder()).build()
}

/// What the install step was handed, so a test can see nothing was installed.
#[derive(Clone, Default)]
struct Installs(Arc<Mutex<Vec<Vec<u8>>>>);

impl Installs {
    fn record(&self) -> impl FnOnce(&Update, &[u8]) -> Result<(), Error> {
        let installs = self.clone();
        move |_, bytes| {
            installs.0.lock().unwrap().push(bytes.to_vec());
            Ok(())
        }
    }

    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

/// Checks, then installs, recording progress and what reached the install step.
fn check_and_install(keys: &Keys, server: &Server) -> (InstallOutcome, Vec<Progress>, Installs) {
    let app = app(&keys.public(), &server.url("/latest.json"));
    let updates = Updates::default();
    let installs = Installs::default();
    let mut heard = Vec::new();
    let outcome = tauri::async_runtime::block_on(async {
        updates.check(updater(&app)).await;
        updates.install(|p| heard.push(p), installs.record()).await
    });
    (outcome, heard, installs)
}

fn failed_for(reason: Reason) -> InstallOutcome {
    InstallOutcome::Failed {
        reason,
        message: reason.message().to_string(),
    }
}

#[test]
fn a_newer_release_is_found_with_its_version_and_nothing_is_downloaded_or_installed() {
    let keys = Keys::new();
    let server = release(&keys, Answer::Bytes(FILE.to_vec()));
    let app = app(&keys.public(), &server.url("/latest.json"));
    let updates = Updates::default();
    let restarts = AtomicUsize::new(0);
    let outcome = tauri::async_runtime::block_on(updates.check(updater(&app)));
    assert_eq!(
        outcome,
        CheckOutcome::Available {
            version: NEWER.into(),
            notes_url:
                "https://github.com/robpaolella/bureaucracy-forever-helper/releases/tag/v0.2.0"
                    .into()
        }
    );
    assert_eq!(updates.found_notes_url(), Some(notes_url(NEWER)));
    assert_eq!(server.hits("/helper.tar.gz"), 0);
    // Nothing installed means nothing to restart into.
    assert_eq!(
        updates.restart(|_, _| {
            restarts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }),
        RestartOutcome::NotReady
    );
    assert_eq!(restarts.load(Ordering::SeqCst), 0);
}

#[test]
fn the_same_or_an_older_version_or_an_empty_answer_is_up_to_date() {
    let keys = Keys::new();
    for version in ["0.1.0", "0.0.9"] {
        let body = manifest(
            version,
            "http://127.0.0.1:9/x",
            &keys.sign(FILE, Some(version)),
        );
        let server = Server::start(HashMap::from([("/latest.json", Answer::Json(body))]));
        let app = app(&keys.public(), &server.url("/latest.json"));
        let outcome = tauri::async_runtime::block_on(Updates::default().check(updater(&app)));
        assert_eq!(outcome, CheckOutcome::UpToDate, "{version}");
    }
    let server = Server::start(HashMap::from([(
        "/latest.json",
        Answer::Status("204 No Content"),
    )]));
    let app = app(&keys.public(), &server.url("/latest.json"));
    let updates = Updates::default();
    let outcome = tauri::async_runtime::block_on(updates.check(updater(&app)));
    assert_eq!(outcome, CheckOutcome::UpToDate);
    // Nothing found means no release notes to open.
    assert_eq!(updates.found_notes_url(), None);
}

#[test]
fn installing_downloads_the_signed_file_reports_progress_and_installs_it_once() {
    let keys = Keys::new();
    let server = release(&keys, Answer::Bytes(FILE.to_vec()));
    let (outcome, heard, installs) = check_and_install(&keys, &server);
    assert_eq!(
        outcome,
        InstallOutcome::Ready {
            version: NEWER.into()
        }
    );
    assert_eq!(*installs.0.lock().unwrap(), vec![FILE.to_vec()]);
    let total = Some(FILE.len() as u64);
    assert_eq!(
        heard.last(),
        Some(&Progress {
            downloaded: FILE.len() as u64,
            total
        })
    );
    assert!(heard.iter().all(|p| p.total == total));
    assert!(heard.windows(2).all(|w| w[0].downloaded < w[1].downloaded));
}

#[test]
fn installing_without_a_found_update_does_nothing() {
    let keys = Keys::new();
    let updates = Updates::default();
    let installs = Installs::default();
    let outcome = tauri::async_runtime::block_on(updates.install(|_| {}, installs.record()));
    assert_eq!(outcome, InstallOutcome::NothingFound);

    let body = manifest(
        "0.1.0",
        "http://127.0.0.1:9/x",
        &keys.sign(FILE, Some("0.1.0")),
    );
    let server = Server::start(HashMap::from([("/latest.json", Answer::Json(body))]));
    let app = app(&keys.public(), &server.url("/latest.json"));
    let outcome = tauri::async_runtime::block_on(async {
        updates.check(updater(&app)).await;
        updates.install(|_| {}, installs.record()).await
    });
    assert_eq!(outcome, InstallOutcome::NothingFound);
    assert_eq!(installs.count(), 0);
}

#[test]
fn a_file_signed_with_another_key_is_refused_and_nothing_is_installed() {
    let keys = Keys::new();
    let server = release(&Keys::new(), Answer::Bytes(FILE.to_vec()));
    let (outcome, _, installs) = check_and_install(&keys, &server);
    assert_eq!(outcome, failed_for(Reason::BadSignature));
    assert_eq!(installs.count(), 0);
}

#[test]
fn a_file_changed_after_signing_is_refused() {
    let keys = Keys::new();
    let server = release(&keys, Answer::Bytes(b"something else entirely".to_vec()));
    let (outcome, _, installs) = check_and_install(&keys, &server);
    assert_eq!(outcome, failed_for(Reason::BadSignature));
    assert_eq!(installs.count(), 0);
}

#[test]
fn a_signature_for_another_version_or_none_is_refused() {
    let keys = Keys::new();
    for signature in [
        keys.sign(FILE, Some("0.1.5")),
        keys.sign(FILE, None),
        "not a signature".to_string(),
    ] {
        let server = release_signed(&signature, Answer::Bytes(FILE.to_vec()));
        let (outcome, _, installs) = check_and_install(&keys, &server);
        assert_eq!(outcome, failed_for(Reason::BadSignature), "{signature}");
        assert_eq!(installs.count(), 0);
    }
}

/// An address nothing listens on.
fn nowhere() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    format!("http://{}", listener.local_addr().unwrap())
}

#[test]
fn no_network_fails_the_check_and_the_download_as_offline() {
    let keys = Keys::new();
    let app = app(&keys.public(), &format!("{}/latest.json", nowhere()));
    let outcome = tauri::async_runtime::block_on(Updates::default().check(updater(&app)));
    assert_eq!(
        outcome,
        CheckOutcome::Failed {
            reason: Reason::Offline,
            message: Reason::Offline.message().into()
        }
    );

    let file = format!("{}/helper.tar.gz", nowhere());
    let body = manifest(NEWER, &file, &keys.sign(FILE, Some(NEWER)));
    let server = Server::start(HashMap::from([("/latest.json", Answer::Json(body))]));
    let (outcome, _, installs) = check_and_install(&keys, &server);
    assert_eq!(outcome, failed_for(Reason::Offline));
    assert_eq!(installs.count(), 0);
}

#[test]
fn a_missing_manifest_or_update_is_missing() {
    let keys = Keys::new();
    let missing = CheckOutcome::Failed {
        reason: Reason::Missing,
        message: Reason::Missing.message().into(),
    };
    // No manifest (a release published without one), and one with nothing for this computer.
    let elsewhere = json!({ "version": NEWER, "platforms": { "other": { "url": "http://127.0.0.1:9/x",
        "signature": "x" } } });
    let server = Server::start(HashMap::from([("/other.json", Answer::Json(elsewhere))]));
    for path in ["/latest.json", "/other.json"] {
        let app = app(&keys.public(), &server.url(path));
        let outcome = tauri::async_runtime::block_on(Updates::default().check(updater(&app)));
        assert_eq!(outcome, missing, "{path}");
    }

    let server = release(&keys, Answer::Status("404 Not Found"));
    let (outcome, _, installs) = check_and_install(&keys, &server);
    assert_eq!(outcome, failed_for(Reason::Missing));
    assert_eq!(installs.count(), 0);
}

#[test]
fn a_download_cut_off_part_way_is_interrupted_and_nothing_is_installed() {
    let keys = Keys::new();
    let server = release(&keys, Answer::CutOff);
    let (outcome, _, installs) = check_and_install(&keys, &server);
    assert_eq!(outcome, failed_for(Reason::Interrupted));
    assert_eq!(installs.count(), 0);
}

#[test]
fn a_failed_install_leaves_nothing_to_restart_into() {
    let keys = Keys::new();
    let server = release(&keys, Answer::Bytes(FILE.to_vec()));
    let app = app(&keys.public(), &server.url("/latest.json"));
    let updates = Updates::default();
    let outcome = tauri::async_runtime::block_on(async {
        updates.check(updater(&app)).await;
        updates
            .install(|_| {}, |_, _| Err(Error::InvalidUpdaterFormat))
            .await
    });
    assert_eq!(outcome, failed_for(Reason::InstallFailed));
    let restarted = AtomicBool::new(false);
    assert_eq!(
        updates.restart(|_, _| {
            restarted.store(true, Ordering::SeqCst);
            Ok(())
        }),
        RestartOutcome::NotReady
    );
    assert!(!restarted.load(Ordering::SeqCst));
}

#[test]
fn only_the_restart_command_restarts_and_only_after_an_install() {
    let keys = Keys::new();
    let server = release(&keys, Answer::Bytes(FILE.to_vec()));
    let app = app(&keys.public(), &server.url("/latest.json"));
    let updates = Updates::default();
    let restarts = Mutex::new(Vec::new());
    let restart = |update: &Update, bytes: &[u8]| {
        restarts
            .lock()
            .unwrap()
            .push((update.version.clone(), bytes.to_vec()));
        Ok(())
    };
    let outcome = tauri::async_runtime::block_on(async {
        updates.check(updater(&app)).await;
        updates.install(|_| {}, Installs::default().record()).await
    });
    assert!(matches!(outcome, InstallOutcome::Ready { .. }));
    assert!(restarts.lock().unwrap().is_empty());
    assert_eq!(updates.restart(restart), RestartOutcome::Restarting);
    // It gets the checked file, which Windows' installer needs.
    assert_eq!(
        *restarts.lock().unwrap(),
        vec![(NEWER.to_string(), FILE.to_vec())]
    );
}

#[test]
fn a_restart_that_fails_leaves_the_update_ready_to_try_again() {
    let keys = Keys::new();
    let server = release(&keys, Answer::Bytes(FILE.to_vec()));
    let app = app(&keys.public(), &server.url("/latest.json"));
    let updates = Updates::default();
    tauri::async_runtime::block_on(async {
        updates.check(updater(&app)).await;
        updates.install(|_| {}, Installs::default().record()).await
    });
    let refused = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
    assert_eq!(
        updates.restart(|_, _| Err(Error::Io(refused))),
        RestartOutcome::Failed {
            reason: Reason::InstallFailed,
            message: Reason::InstallFailed.message().into()
        }
    );
    assert_eq!(updates.restart(|_, _| Ok(())), RestartOutcome::Restarting);
}

/// A store holding one production key.
struct OneKey(HelperKey);

impl SecretStore for OneKey {
    fn save(&self, _: Site, _: &HelperKey) -> Result<(), StoreError> {
        Ok(())
    }
    fn read(&self, site: Site) -> Result<Option<HelperKey>, StoreError> {
        Ok((site == Site::Production).then(|| self.0.clone()))
    }
    fn delete(&self, _: Site) -> Result<(), StoreError> {
        Ok(())
    }
}

/// A site that tells the test each time the uploader sends.
struct Told(Mutex<Sender<()>>);

impl Uplink for Told {
    fn post_operations(&self, _: Site, _: &HelperKey, _: &str) -> Result<UploadReply, String> {
        let _ = self.0.lock().unwrap().send(());
        Err("not answering".into())
    }
}

#[test]
fn awards_keep_sending_while_an_update_downloads_and_a_second_install_waits() {
    let keys = Keys::new();
    let (go, held) = channel();
    let server = release(&keys, Answer::Held(Arc::new(Mutex::new(held))));
    let app = app(&keys.public(), &server.url("/latest.json"));
    let updates = Arc::new(Updates::default());
    tauri::async_runtime::block_on(updates.check(updater(&app)));
    let installs = Installs::default();
    let (started, downloading) = channel();
    let download = {
        let (updates, record) = (Arc::clone(&updates), installs.record());
        tauri::async_runtime::spawn(async move {
            updates
                .install(
                    move |_| {
                        let _ = started.send(());
                    },
                    record,
                )
                .await
        })
    };
    // Half the file has arrived and the rest is held back.
    downloading.recv_timeout(Duration::from_secs(10)).unwrap();
    let second = tauri::async_runtime::block_on(updates.install(|_| {}, |_, _| Ok(())));
    assert_eq!(second, InstallOutcome::Busy);

    let dir = std::env::temp_dir().join(format!("bf-updater-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let engine = Engine::new(KeyEngine::open(
        dir.join("settings.json"),
        OneKey(HelperKey::parse("bfh_q7Xk2mP9vR4tW8yZ1aB3cD5eF6gH0iJ-kL_nO2pQ4sA").unwrap()),
        SiteClient::default(),
    ));
    let mut queue = Queue::open(dir.join(FILE_NAME));
    queue
        .add(
            json!({ "type": "award", "id": "aw-update-0000", "formatVersion": 1,
            "raid": "cm9xr2k4a0001", "env": "production", "boss": "lb_lucifron", "item": 16805,
            "char": "Ana Vale", "method": "HR", "roll": 91, "sender": "Rae Quill",
            "at": 1792001200 }),
        )
        .unwrap();
    let (told, sent) = channel();
    let runner = Runner::start(
        engine,
        Arc::new(Mutex::new(queue)),
        Told(Mutex::new(told)),
        |_| {},
    );
    sent.recv_timeout(Duration::from_secs(10))
        .expect("the award was sent while the update downloaded");
    assert!(!download.inner().is_finished());
    runner.stop();
    runner.join();

    go.send(()).unwrap();
    let outcome = tauri::async_runtime::block_on(download).unwrap();
    assert_eq!(
        outcome,
        InstallOutcome::Ready {
            version: NEWER.into()
        }
    );
    assert_eq!(installs.count(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}
