use super::*;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

fn temp() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "bf-game-folder-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A game folder `root/<name>` with the given version subfolders, each with `Interface`;
/// those named in `with_addon` also have the add-on.
fn game(root: &Path, name: &str, versions: &[&str], with_addon: &[&str]) -> PathBuf {
    let game = root.join(name);
    for version in versions {
        fs::create_dir_all(game.join(version).join("Interface").join("AddOns")).unwrap();
    }
    for version in with_addon {
        fs::create_dir_all(
            game.join(version)
                .join("Interface/AddOns")
                .join(ADDON_FOLDER),
        )
        .unwrap();
    }
    game
}

fn kind(outcome: &FolderOutcome) -> String {
    serde_json::to_value(outcome).unwrap()["kind"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn a_game_folder_is_one_with_a_subfolder_holding_interface() {
    let root = temp();
    assert!(is_game_folder(&game(&root, "wow", &["_retail_"], &[])));
    let no_interface = root.join("other");
    fs::create_dir_all(no_interface.join("_retail_")).unwrap();
    assert!(!is_game_folder(&no_interface));
    // `Interface` directly inside, not in a subfolder, doesn't count.
    let version = root.join("wow").join("_retail_");
    assert!(!is_game_folder(&version));
    assert!(!is_game_folder(&root.join("missing")));
}

#[test]
fn a_picked_game_folder_with_the_addon_is_found_with_its_paths() {
    let root = temp();
    let folder = game(&root, "wow", &["_classic_", "_retail_"], &["_retail_"]);
    let FolderOutcome::Found(found) = check(&folder) else {
        panic!("expected found");
    };
    assert_eq!(found.version, "_retail_");
    assert_eq!(found.game_folder, text(&folder));
    assert_eq!(found.version_folder, text(&folder.join("_retail_")));
    assert!(found
        .addon_folder
        .ends_with("Interface/AddOns/BureaucracyForever"));
}

#[test]
fn several_versions_with_the_addon_are_listed_for_the_officer_to_pick() {
    let root = temp();
    let folder = game(
        &root,
        "wow",
        &["_retail_", "_classic_beta_", "_ptr_"],
        &["_retail_", "_classic_beta_"],
    );
    let FolderOutcome::Several { installs } = check(&folder) else {
        panic!("expected several");
    };
    let versions: Vec<_> = installs.iter().map(|i| i.version.as_str()).collect();
    assert_eq!(versions, ["_classic_beta_", "_retail_"]);
}

#[test]
fn a_game_folder_without_the_addon_says_so() {
    let root = temp();
    let folder = game(&root, "wow", &["_retail_"], &[]);
    assert_eq!(
        check(&folder),
        FolderOutcome::NoAddon {
            message: NO_ADDON,
            path: text(&folder)
        }
    );
}

#[test]
fn a_folder_that_is_not_a_game_folder_is_refused() {
    let root = temp();
    assert_eq!(
        check(&root),
        FolderOutcome::NotGame {
            message: NOT_A_GAME_FOLDER
        }
    );
    assert_eq!(kind(&check(&root.join("missing"))), "notGame");
}

#[test]
fn spaces_and_accented_characters_in_paths_work() {
    let root = temp();
    let folder = game(&root, "Wörld of Wárcraft ünï", &["_retail_"], &["_retail_"]);
    assert_eq!(kind(&check(&folder)), "found");
}

#[test]
fn mac_places_are_the_two_applications_folders() {
    let env = Environment {
        os: Os::Mac,
        home: Some("/Users/aldric".into()),
        program_files: vec![],
        drives: vec![],
    };
    assert_eq!(
        env.usual_places(),
        [
            PathBuf::from("/Applications/World of Warcraft"),
            PathBuf::from("/Users/aldric/Applications/World of Warcraft"),
        ]
    );
}

#[test]
fn windows_places_are_program_files_and_every_drive() {
    let env = Environment {
        os: Os::Windows,
        home: None,
        program_files: vec!["PF".into(), "PF86".into()],
        drives: vec!["C".into(), "D".into()],
    };
    let expected: Vec<PathBuf> = [
        "PF/World of Warcraft",
        "PF86/World of Warcraft",
        "C/World of Warcraft",
        "C/Games/World of Warcraft",
        "D/World of Warcraft",
        "D/Games/World of Warcraft",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    assert_eq!(env.usual_places(), expected);
}

#[test]
fn a_search_finds_a_game_on_a_drive_that_is_not_c() {
    let (c, d) = (temp(), temp());
    game(&d, "Games/World of Warcraft", &["_retail_"], &["_retail_"]);
    let env = Environment {
        os: Os::Windows,
        home: None,
        program_files: vec![c.join("Program Files")],
        drives: vec![c, d.clone()],
    };
    let FolderOutcome::Found(found) = search(&env.usual_places()) else {
        panic!("expected found");
    };
    assert_eq!(found.game_folder, text(&d.join("Games/World of Warcraft")));
}

#[test]
fn a_search_adds_up_versions_across_places_and_finds_a_game_without_the_addon() {
    let root = temp();
    let a = game(&root, "a", &["_retail_"], &["_retail_"]);
    let b = game(&root, "b", &["_classic_"], &["_classic_"]);
    let bare = game(&root, "bare", &["_retail_"], &[]);
    // The same place twice must not make two matches.
    let several = search(&[a.clone(), a.clone(), b.clone()]);
    let FolderOutcome::Several { installs } = several else {
        panic!("expected several");
    };
    assert_eq!(installs.len(), 2);
    assert_eq!(
        search(&[root.join("nope"), bare.clone()]),
        FolderOutcome::NoAddon {
            message: NO_ADDON,
            path: text(&bare)
        }
    );
    assert_eq!(kind(&search(&[bare, a])), "found");
    assert_eq!(
        search(&[root.join("nope")]),
        FolderOutcome::NotFound { message: NOT_FOUND }
    );
    assert_eq!(kind(&search(&[])), "notFound");
}

#[test]
fn the_remembered_folder_is_kept_and_checked_again() {
    let root = temp();
    let folder = game(&root, "wow", &["_retail_"], &["_retail_"]);
    let version = folder.join("_retail_");
    let settings = root.join("settings.json");
    let store = GameFolderStore::new(&settings);
    assert_eq!(store.remembered(), FolderOutcome::NothingRemembered);
    assert_eq!(kind(&store.remember(&version)), "found");
    assert_eq!(store.remembered(), recheck(&version));
    assert_eq!(kind(&store.remembered()), "found");
    // Gone: reported with its path, and a later search doesn't replace it.
    fs::remove_dir_all(&folder).unwrap();
    assert_eq!(
        store.remembered(),
        FolderOutcome::Gone {
            message: GONE,
            path: text(&version)
        }
    );
    assert_eq!(
        settings::read_game_folder(&settings).as_deref(),
        Some(text(&version).as_str())
    );
}

#[test]
fn only_a_folder_with_the_addon_is_remembered() {
    let root = temp();
    let folder = game(&root, "wow", &["_retail_"], &[]);
    let settings = root.join("settings.json");
    let store = GameFolderStore::new(&settings);
    assert_eq!(kind(&store.remember(&folder.join("_retail_"))), "noAddon");
    assert_eq!(kind(&store.remember(&root.join("missing"))), "gone");
    assert_eq!(store.remembered(), FolderOutcome::NothingRemembered);
}

#[test]
fn remembering_keeps_the_staging_setting_and_other_entries() {
    let root = temp();
    let folder = game(&root, "wow", &["_retail_"], &["_retail_"]);
    let settings = root.join("settings.json");
    fs::write(
        &settings,
        json!({"formatVersion": 1, "staging": true, "other": 7}).to_string(),
    )
    .unwrap();
    let store = GameFolderStore::new(&settings);
    store.remember(&folder.join("_retail_"));
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&settings).unwrap()).unwrap();
    assert_eq!(saved["staging"], true);
    assert_eq!(saved["other"], 7);
    assert!(settings::read_staging(&settings));
}

#[test]
fn a_settings_file_that_cannot_be_written_is_reported() {
    let root = temp();
    let folder = game(&root, "wow", &["_retail_"], &["_retail_"]);
    let store = GameFolderStore::new(root.join("missing-dir").join("settings.json"));
    assert_eq!(
        kind(&store.remember(&folder.join("_retail_"))),
        "settingsFailed"
    );
}

#[test]
fn results_serialize_with_a_kind_the_front_end_can_tell_apart() {
    let root = temp();
    let folder = game(&root, "wow", &["_retail_"], &["_retail_"]);
    let value = serde_json::to_value(check(&folder)).unwrap();
    assert_eq!(value["kind"], "found");
    assert_eq!(value["gameFolder"], text(&folder));
    assert_eq!(value["version"], "_retail_");
}
