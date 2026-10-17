use super::install::{check, put_in_place, put_in_place_with};
use super::*;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{Cursor, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use zip::write::SimpleFileOptions;

const NOTES: &str = "## Before you update\r\nType /reload in the game after updating. No restart needed.\r\n- Saved data changed.\r\n\r\n## New\r\n- Officers get a new window.\r\n";

fn temp() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-addon-update-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn zip_of(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(vec![]));
    for (name, text) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// The add-on's zip at `version`, as the add-on's release builds it.
fn addon_zip(version: &str) -> Vec<u8> {
    zip_of(&[
        ("BureaucracyForever/BureaucracyForever.toc", &format!("## Title: Bureaucracy Forever\n## Version: {version}\n\nCore.lua\nModules\\UI.lua\nInbox.lua\n")),
        ("BureaucracyForever/Core.lua", "-- new core"),
        ("BureaucracyForever/Modules/UI.lua", "-- new ui"),
        ("BureaucracyForever/Inbox.lua", "-- placeholder"),
    ])
}

/// A game version folder; with `addon`, it holds the add-on at that version and an inbox.
fn version_folder(addon: Option<&str>) -> PathBuf {
    let folder = temp().join("_retail_");
    let addons = folder.join("Interface/AddOns");
    fs::create_dir_all(addons.join("OtherAddon")).unwrap();
    fs::write(addons.join("OtherAddon/Other.toc"), "other").unwrap();
    if let Some(version) = addon {
        let live = addons.join("BureaucracyForever");
        fs::create_dir_all(&live).unwrap();
        fs::write(
            live.join("BureaucracyForever.toc"),
            format!("## Version: {version}\n"),
        )
        .unwrap();
        fs::write(live.join("Core.lua"), "-- old core").unwrap();
        fs::write(live.join("Inbox.lua"), "-- the officers' reserves").unwrap();
    }
    folder
}

/// Every file under the add-ons folder, with its contents.
fn files(version_folder: &Path) -> BTreeMap<String, String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, String>) {
        for entry in fs::read_dir(dir).unwrap().map(Result::unwrap) {
            let path = entry.path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(name, fs::read_to_string(&path).unwrap());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(
        &version_folder.join("Interface/AddOns"),
        &version_folder.join("Interface/AddOns"),
        &mut out,
    );
    out
}

/// GitHub on this machine: answers each path with a status and body, and counts requests.
struct FakeGitHub {
    address: String,
    answers: Arc<Mutex<HashMap<String, Vec<u8>>>>,
    requests: Arc<Mutex<Vec<String>>>,
}

impl FakeGitHub {
    fn start() -> FakeGitHub {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let fake = FakeGitHub {
            address: format!("http://{}", listener.local_addr().unwrap()),
            answers: Arc::default(),
            requests: Arc::default(),
        };
        let (answers, requests) = (Arc::clone(&fake.answers), Arc::clone(&fake.requests));
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut head = Vec::new();
                let mut byte = [0u8];
                while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    head.push(byte[0]);
                }
                let head = String::from_utf8_lossy(&head).into_owned();
                let path = head.split(' ').nth(1).unwrap_or("").to_string();
                requests.lock().unwrap().push(path.clone());
                let (status, body) = answers
                    .lock()
                    .unwrap()
                    .get(&path)
                    .map_or(("404 Not Found", vec![]), |body| ("200 OK", body.clone()));
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        fake
    }

    fn answer(&self, path: &str, body: Vec<u8>) {
        self.answers.lock().unwrap().insert(path.to_string(), body);
    }

    /// A release of `version` whose zip is `zip`.
    fn release(&self, version: &str, zip: Vec<u8>) {
        let answer = json!({
            "tag_name": format!("v{version}"),
            "body": NOTES,
            "assets": [
                { "name": "notes.txt", "browser_download_url": format!("{}/notes", self.address) },
                { "name": format!("BureaucracyForever-{version}.zip"), "browser_download_url": format!("{}/zip/{version}", self.address) },
            ],
        });
        self.answer(
            &format!("/releases/v{version}"),
            answer.to_string().into_bytes(),
        );
        self.answer(&format!("/zip/{version}"), zip);
    }

    fn downloads(&self) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.starts_with("/zip/"))
            .count()
    }

    fn engine(&self) -> AddonUpdates {
        AddonUpdates::new(GitHub::with_address(
            &format!("{}/releases/v", self.address),
            Duration::from_secs(5),
        ))
    }
}

/// Checks for `version` and installs it into `folder`, giving the install's result.
fn update(github: &FakeGitHub, folder: &Path, version: &str) -> AddonInstall {
    let engine = github.engine();
    assert!(matches!(
        engine.check(Some(version.into()), folder),
        AddonCheck::Checked { .. }
    ));
    engine.install(folder, |_| {})
}

fn not_installed(reason: Reason, kept: Option<&str>) -> AddonInstall {
    AddonInstall::NotInstalled {
        reason,
        message: reason.message(),
        kept: kept.map(str::to_string),
    }
}

#[test]
fn reads_the_zip_and_note_from_the_release() {
    let github = FakeGitHub::start();
    github.release("1.4.0", addon_zip("1.4.0"));
    let release = github.engine().github.release("v1.4.0").unwrap();
    assert_eq!(release.zip_url, format!("{}/zip/1.4.0", github.address));
    assert_eq!(
        release.note.as_deref(),
        Some("Type /reload in the game after updating. No restart needed.\n- Saved data changed.")
    );
}

#[test]
fn a_release_without_the_section_gives_no_note() {
    let asset = json!([{ "name": "BureaucracyForever-1.0.0.zip", "browser_download_url": "u" }]);
    for body in [
        json!("## New\n- A window."),
        json!("## Before you update\n\n## New\n- x"),
        json!(null),
    ] {
        let release = release_from(&json!({ "body": body, "assets": asset }), "1.0.0").unwrap();
        assert_eq!(release.note, None);
    }
}

#[test]
fn a_release_without_its_zip_is_no_release() {
    let other = json!({ "assets": [{ "name": "BureaucracyForever-0.9.0.zip", "browser_download_url": "u" }] });
    assert_eq!(release_from(&other, "1.0.0"), None);
    assert_eq!(release_from(&json!({ "assets": [] }), "1.0.0"), None);
}

#[test]
fn check_compares_the_site_version_with_the_installed_one() {
    let github = FakeGitHub::start();
    github.release("1.4.0", addon_zip("1.4.0"));
    let engine = github.engine();
    let checked = |installed: Option<&str>, up_to_date| AddonCheck::Checked {
        latest: "1.4.0".into(),
        note: Some(
            "Type /reload in the game after updating. No restart needed.\n- Saved data changed."
                .into(),
        ),
        installed: installed.map(str::to_string),
        up_to_date,
    };
    assert_eq!(
        engine.check(Some("1.4.0".into()), &version_folder(Some("1.3.2"))),
        checked(Some("1.3.2"), false)
    );
    assert_eq!(
        engine.check(Some("1.4.0".into()), &version_folder(Some("1.4.0"))),
        checked(Some("1.4.0"), true)
    );
    assert_eq!(
        engine.check(Some("1.4.0".into()), &version_folder(None)),
        checked(None, false)
    );
}

#[test]
fn a_check_that_finds_no_release_keeps_the_old_version_and_installs_nothing() {
    let github = FakeGitHub::start();
    github.answer(
        "/releases/v1.5.0",
        json!({ "assets": [] }).to_string().into_bytes(),
    );
    let engine = github.engine();
    let folder = version_folder(Some("1.3.2"));
    let before = files(&folder);
    let failed = |reason: Reason| AddonCheck::Failed {
        reason,
        message: reason.message(),
        installed: Some("1.3.2".into()),
    };
    // No release with that tag, one without its zip, a version that isn't one, and no site.
    assert_eq!(
        engine.check(Some("1.4.0".into()), &folder),
        failed(Reason::NoRelease)
    );
    assert_eq!(
        engine.check(Some("1.5.0".into()), &folder),
        failed(Reason::NoRelease)
    );
    assert_eq!(
        engine.check(Some("1.4/../x".into()), &folder),
        failed(Reason::NoRelease)
    );
    assert_eq!(engine.check(None, &folder), failed(Reason::Site));
    assert_eq!(engine.install(&folder, |_| {}), AddonInstall::NothingFound);
    assert_eq!(files(&folder), before);
}

#[test]
fn install_over_an_existing_addon_keeps_the_inbox() {
    let github = FakeGitHub::start();
    github.release("1.4.0", addon_zip("1.4.0"));
    let folder = version_folder(Some("1.3.2"));
    assert_eq!(
        update(&github, &folder, "1.4.0"),
        AddonInstall::Installed {
            version: "1.4.0".into()
        }
    );
    let after = files(&folder);
    assert_eq!(
        after["BureaucracyForever/Inbox.lua"],
        "-- the officers' reserves"
    );
    assert_eq!(after["BureaucracyForever/Core.lua"], "-- new core");
    assert_eq!(after["BureaucracyForever/Modules/UI.lua"], "-- new ui");
    assert_eq!(after["OtherAddon/Other.toc"], "other");
    // Nothing else: no temporary folders are left behind.
    assert_eq!(after.len(), 5);
    assert_eq!(installed_version(&folder).as_deref(), Some("1.4.0"));
}

#[test]
fn installs_into_a_game_folder_with_no_addon() {
    let github = FakeGitHub::start();
    github.release("1.4.0", addon_zip("1.4.0"));
    let folder = version_folder(None);
    fs::remove_dir_all(folder.join("Interface/AddOns")).unwrap();
    assert_eq!(
        update(&github, &folder, "1.4.0"),
        AddonInstall::Installed {
            version: "1.4.0".into()
        }
    );
    assert_eq!(
        files(&folder)["BureaucracyForever/Inbox.lua"],
        "-- placeholder"
    );
    assert_eq!(files(&folder).len(), 4);
}

#[test]
fn a_folder_that_isnt_a_game_folder_gets_nothing() {
    let github = FakeGitHub::start();
    github.release("1.4.0", addon_zip("1.4.0"));
    let folder = temp();
    assert_eq!(
        update(&github, &folder, "1.4.0"),
        not_installed(Reason::NotGameFolder, None)
    );
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
}

#[test]
fn a_bad_download_leaves_the_old_version_untouched() {
    let toc = |v: &str| format!("## Version: {v}\n");
    let bad = [
        b"not a zip".to_vec(),
        zip_of(&[("Other/BureaucracyForever.toc", &toc("1.4.0"))]),
        zip_of(&[("BureaucracyForever/Core.lua", "-- no toc")]),
        zip_of(&[("BureaucracyForever/BureaucracyForever.toc", &toc("1.3.9"))]),
        zip_of(&[(
            "BureaucracyForever/BureaucracyForever.toc",
            "## Title: no version\n",
        )]),
        zip_of(&[
            ("BureaucracyForever/BureaucracyForever.toc", &toc("1.4.0")),
            ("Extra/x.lua", ""),
        ]),
        zip_of(&[
            ("BureaucracyForever/BureaucracyForever.toc", &toc("1.4.0")),
            ("BureaucracyForever/../../x.lua", ""),
        ]),
    ];
    for zip in bad {
        let github = FakeGitHub::start();
        github.release("1.4.0", zip);
        let folder = version_folder(Some("1.3.2"));
        let before = files(&folder);
        assert_eq!(
            update(&github, &folder, "1.4.0"),
            not_installed(Reason::BadDownload, Some("1.3.2"))
        );
        assert_eq!(files(&folder), before);
    }
}

#[test]
fn a_failed_swap_rolls_back_to_the_old_version() {
    let folder = version_folder(Some("1.3.2"));
    let before = files(&folder);
    let zip = addon_zip("1.4.0");
    // The swap's first rename (live to old), then its second (new to live), fails.
    for failing in [1, 2] {
        let calls = AtomicUsize::new(0);
        let rename = |from: &Path, to: &Path| match calls.fetch_add(1, Ordering::SeqCst) + 1 {
            n if n == failing => Err(std::io::Error::other("refused")),
            _ => fs::rename(from, to),
        };
        let placed = put_in_place_with(&folder, check(&zip, "1.4.0").unwrap(), rename);
        assert_eq!(placed, Err(install::PlaceError::Failed));
        assert_eq!(files(&folder), before);
        assert_eq!(
            fs::read_dir(folder.join("Interface/AddOns"))
                .unwrap()
                .count(),
            2
        );
    }
}

#[test]
fn an_addon_that_couldnt_be_moved_back_is_put_back_at_the_next_check() {
    let github = FakeGitHub::start();
    let folder = version_folder(Some("1.3.2"));
    let before = files(&folder);
    let zip = addon_zip("1.4.0");
    // The swap (call 2) and the move back (call 3) both fail.
    let calls = AtomicUsize::new(0);
    let rename = |from: &Path, to: &Path| match calls.fetch_add(1, Ordering::SeqCst) + 1 {
        2 | 3 => Err(std::io::Error::other("refused")),
        _ => fs::rename(from, to),
    };
    let placed = put_in_place_with(&folder, check(&zip, "1.4.0").unwrap(), rename);
    assert_eq!(placed, Err(install::PlaceError::NotRestored));
    assert_eq!(installed_version(&folder), None);
    // The next check moves it back, with no download.
    let checked = github.engine().check(None, &folder);
    assert!(matches!(checked, AddonCheck::Failed { installed: Some(v), .. } if v == "1.3.2"));
    assert_eq!(files(&folder), before);
    assert_eq!(github.downloads(), 0);
}

#[cfg(unix)]
#[test]
fn a_leftover_that_cant_be_removed_stops_the_install() {
    use std::os::unix::fs::PermissionsExt;
    let folder = version_folder(Some("1.3.2"));
    let locked = folder.join("Interface/AddOns/.BureaucracyForever-new/locked");
    fs::create_dir_all(&locked).unwrap();
    fs::write(locked.join("stale.lua"), "-- stale").unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let before = files(&folder);
    let zip = addon_zip("1.4.0");
    let placed = put_in_place(&folder, check(&zip, "1.4.0").unwrap());
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(placed, Err(install::PlaceError::Failed));
    assert_eq!(files(&folder), before);
}

#[test]
fn a_failed_install_is_not_tried_again_by_itself() {
    let github = FakeGitHub::start();
    github.release("1.4.0", b"broken".to_vec());
    let engine = github.engine();
    let folder = version_folder(Some("1.3.2"));
    engine.check(Some("1.4.0".into()), &folder);
    assert_eq!(
        engine.install(&folder, |_| {}),
        not_installed(Reason::BadDownload, Some("1.3.2"))
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(github.downloads(), 1);
    // "Try again" is another call, and the fixed release then installs.
    github.release("1.4.0", addon_zip("1.4.0"));
    assert_eq!(
        engine.install(&folder, |_| {}),
        AddonInstall::Installed {
            version: "1.4.0".into()
        }
    );
    assert_eq!(github.downloads(), 2);
}

#[test]
fn a_swap_cut_short_is_finished_before_the_next_install() {
    let github = FakeGitHub::start();
    github.release("1.4.0", addon_zip("1.4.0"));
    let folder = version_folder(Some("1.3.2"));
    let addons = folder.join("Interface/AddOns");
    fs::rename(
        addons.join("BureaucracyForever"),
        addons.join(".BureaucracyForever-old"),
    )
    .unwrap();
    assert_eq!(
        update(&github, &folder, "1.4.0"),
        AddonInstall::Installed {
            version: "1.4.0".into()
        }
    );
    assert_eq!(
        files(&folder)["BureaucracyForever/Inbox.lua"],
        "-- the officers' reserves"
    );
}

#[test]
fn reports_progress_while_installing() {
    let github = FakeGitHub::start();
    let zip = addon_zip("1.4.0");
    let size = zip.len() as u64;
    github.release("1.4.0", zip);
    let engine = github.engine();
    let folder = version_folder(Some("1.3.2"));
    engine.check(Some("1.4.0".into()), &folder);
    let mut heard = vec![];
    engine.install(&folder, |p| heard.push(p));
    assert_eq!(
        heard.last(),
        Some(&Progress::Installing {
            version: "1.4.0".into()
        })
    );
    assert_eq!(
        heard[heard.len() - 2],
        Progress::Downloading {
            version: "1.4.0".into(),
            downloaded: size,
            total: Some(size)
        }
    );
    assert_eq!(
        heard.len() - 1,
        heard
            .iter()
            .filter(|p| matches!(p, Progress::Downloading { .. }))
            .count()
    );
    let progress = serde_json::to_value(&heard[0]).unwrap();
    assert_eq!(progress["step"], "downloading");
}

#[test]
fn the_swap_waits_for_an_inbox_write_in_progress() {
    let folder = version_folder(Some("1.3.2"));
    let guard = crate::inbox::ADDON_FOLDER_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let swap = {
        let folder = folder.clone();
        std::thread::spawn(move || {
            let zip = addon_zip("1.4.0");
            put_in_place(&folder, check(&zip, "1.4.0").unwrap())
        })
    };
    std::thread::sleep(Duration::from_millis(200));
    assert!(
        files(&folder)["BureaucracyForever/Core.lua"].contains("old"),
        "it swapped while an inbox write held the lock"
    );
    drop(guard);
    assert_eq!(swap.join().unwrap(), Ok(()));
    assert!(files(&folder)["BureaucracyForever/Core.lua"].contains("new"));
}

#[test]
fn the_notes_page_is_the_release_of_that_version_and_nothing_else() {
    assert_eq!(
        release_page("1.4.0").as_deref(),
        Some("https://github.com/robpaolella/bureaucracy-forever-addon/releases/tag/v1.4.0")
    );
    assert_eq!(release_page(""), None);
    assert_eq!(release_page("1.4.0/../../x"), None);
    assert_eq!(release_page("1.4.0?x=y"), None);
}
