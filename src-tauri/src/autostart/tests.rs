use super::*;
use std::cell::Cell;
use std::fs;
use std::path::PathBuf;

/// A login item that can be told to refuse.
struct Fake {
    on: Cell<bool>,
    refuse: Cell<bool>,
}

impl Fake {
    fn new(on: bool) -> Fake {
        Fake {
            on: Cell::new(on),
            refuse: Cell::new(false),
        }
    }
}

impl LoginItem for Fake {
    fn is_enabled(&self) -> Result<bool, String> {
        Ok(self.on.get())
    }

    fn set(&self, on: bool) -> Result<(), String> {
        if self.refuse.get() {
            return Err("not allowed".into());
        }
        self.on.set(on);
        Ok(())
    }
}

fn settings_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bf-autostart-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.join("settings.json")
}

#[test]
fn only_the_login_item_hides_the_window() {
    let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(started_at_login(args(&["helper", LOGIN_ARG])));
    assert!(!started_at_login(args(&["helper"])));
    assert!(!started_at_login(args(&["helper", "--other"])));
}

#[test]
fn first_setup_turns_it_on_once() {
    let (item, file) = (Fake::new(false), settings_file("first"));
    assert_eq!(first_setup(&item, &file), LoginOutcome::Done { on: true });
    assert!(item.on.get());
    assert_eq!(settings::read_start_at_login(&file), Some(true));
}

#[test]
fn an_off_choice_stays_off_after_setup_runs_again() {
    let (item, file) = (Fake::new(false), settings_file("stays-off"));
    first_setup(&item, &file);
    assert_eq!(
        change(&item, &file, false),
        LoginOutcome::Done { on: false }
    );
    assert_eq!(first_setup(&item, &file), LoginOutcome::Done { on: false });
    assert!(!item.on.get());
}

#[test]
fn turning_it_on_again_adds_the_item_back() {
    let (item, file) = (Fake::new(false), settings_file("back-on"));
    change(&item, &file, false);
    assert_eq!(change(&item, &file, true), LoginOutcome::Done { on: true });
    assert!(item.on.get());
}

#[test]
fn a_refused_change_says_so_and_reports_the_real_state() {
    let (item, file) = (Fake::new(false), settings_file("refused"));
    item.refuse.set(true);
    assert_eq!(
        change(&item, &file, true),
        LoginOutcome::Refused {
            message: REFUSED,
            on: false
        }
    );
    let item = Fake::new(true);
    item.refuse.set(true);
    assert_eq!(
        change(&item, &file, false),
        LoginOutcome::Refused {
            message: REFUSED,
            on: true
        }
    );
}

#[test]
fn a_refused_default_is_not_tried_again_by_setup() {
    let (item, file) = (Fake::new(false), settings_file("refused-default"));
    item.refuse.set(true);
    assert!(matches!(
        first_setup(&item, &file),
        LoginOutcome::Refused { .. }
    ));
    item.refuse.set(false);
    assert_eq!(first_setup(&item, &file), LoginOutcome::Done { on: false });
}

#[test]
fn the_choice_keeps_the_other_settings() {
    let (item, file) = (Fake::new(false), settings_file("keeps"));
    settings::write_game_folder(&file, "/games/wow/_retail_").unwrap();
    change(&item, &file, true);
    assert_eq!(
        settings::read_game_folder(&file).as_deref(),
        Some("/games/wow/_retail_")
    );
}
