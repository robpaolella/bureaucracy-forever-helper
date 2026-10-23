use super::*;

fn process(exe: Option<&str>, started: i64) -> Process {
    Process {
        exe: exe.map(PathBuf::from),
        started,
    }
}

const FOLDER: &str = "/games/World of Warcraft/_retail_";

#[test]
fn a_process_inside_the_game_folder_is_the_game() {
    let list = [
        process(Some("/usr/bin/bash"), 1),
        process(Some("/games/World of Warcraft/_retail_/Wow.exe"), 500),
    ];
    assert_eq!(
        classify(Path::new(FOLDER), &list),
        GameStatus::Running { started: 500 }
    );
}

#[test]
fn the_latest_start_counts_when_several_run() {
    let list = [
        process(Some("/games/World of Warcraft/_retail_/Wow.exe"), 500),
        process(Some("/games/World of Warcraft/_retail_/Wow.exe"), 900),
    ];
    assert_eq!(
        classify(Path::new(FOLDER), &list),
        GameStatus::Running { started: 900 }
    );
}

#[test]
fn a_look_alike_folder_is_not_the_game() {
    let list = [process(
        Some("/games/World of Warcraft/_retail_2/Wow.exe"),
        5,
    )];
    assert_eq!(classify(Path::new(FOLDER), &list), GameStatus::NotRunning);
}

#[test]
fn no_matching_process_means_not_running() {
    let list = [process(Some("/usr/bin/bash"), 1)];
    assert_eq!(classify(Path::new(FOLDER), &list), GameStatus::NotRunning);
}

#[test]
fn an_unreadable_path_with_no_match_is_unknown() {
    let list = [process(Some("/usr/bin/bash"), 1), process(None, 2)];
    assert_eq!(classify(Path::new(FOLDER), &list), GameStatus::Unknown);
}

#[test]
fn an_unreadable_path_does_not_hide_a_found_game() {
    let list = [
        process(None, 2),
        process(Some("/games/World of Warcraft/_retail_/Wow.exe"), 7),
    ];
    assert_eq!(
        classify(Path::new(FOLDER), &list),
        GameStatus::Running { started: 7 }
    );
}

#[test]
fn an_empty_list_is_unknown() {
    assert_eq!(classify(Path::new(FOLDER), &[]), GameStatus::Unknown);
}

#[test]
fn the_real_lookup_finds_no_game_in_an_empty_folder() {
    let dir = std::env::temp_dir().join(format!("bf-game-running-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(!matches!(check(&dir), GameStatus::Running { .. }));
    let _ = std::fs::remove_dir_all(&dir);
}
