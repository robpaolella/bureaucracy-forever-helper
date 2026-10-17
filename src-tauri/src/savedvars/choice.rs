//! How the raid was chosen (ADDON-SPEC 2.1), as the window needs it: the newest saved file's
//! `activeBy`, `activeRaid` and `chosenBy` with the time the game saved it. With several game
//! accounts the reading with the newest save time decides (#60). Values pass on as saved;
//! the window decides what an unknown `activeBy` means.

use super::Reading;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The event that carries each new choice to the window.
pub const RAID_CHOICE_EVENT: &str = "raid-choice";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RaidChoice {
    pub active_by: Option<String>,
    pub active_raid: Option<String>,
    pub chosen_by: Option<String>,
    /// When the game saved the file, Unix seconds.
    pub saved_at: i64,
}

/// Each account's latest choice.
#[derive(Default)]
pub struct Choices(Mutex<BTreeMap<String, RaidChoice>>);

impl Choices {
    /// Notes an account's new reading; returns the choice now in force.
    pub fn record(&self, reading: &Reading) -> Option<RaidChoice> {
        let choice = RaidChoice {
            active_by: reading.selection.active_by.clone(),
            active_raid: reading.selection.active_raid.clone(),
            chosen_by: reading.selection.chosen_by.clone(),
            saved_at: reading.saved_at,
        };
        let mut all = self.0.lock().unwrap_or_else(|e| e.into_inner());
        all.insert(reading.account.clone(), choice);
        newest(&all)
    }

    /// The choice in force, `None` before any file is read.
    pub fn current(&self) -> Option<RaidChoice> {
        newest(&self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// The newest save time wins; a tie goes to the account that sorts first.
fn newest(all: &BTreeMap<String, RaidChoice>) -> Option<RaidChoice> {
    all.values()
        .fold(None::<&RaidChoice>, |best, c| match best {
            Some(b) if b.saved_at >= c.saved_at => Some(b),
            _ => Some(c),
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::savedvars::Selection;

    fn reading(account: &str, saved_at: i64, by: Option<&str>, name: Option<&str>) -> Reading {
        Reading {
            account: account.into(),
            saved_at,
            selection: Selection {
                active_raid: Some("r1".into()),
                active_by: by.map(Into::into),
                chosen_by: name.map(Into::into),
            },
            operations: BTreeMap::new(),
            awards: BTreeMap::new(),
            no_raid: vec![],
            no_raid_ops: vec![],
        }
    }

    #[test]
    fn nothing_is_read_yet() {
        assert_eq!(Choices::default().current(), None);
    }

    #[test]
    fn a_reading_gives_its_fields_as_saved() {
        let c = Choices::default();
        let got = c
            .record(&reading("A", 100, Some("auto"), Some("Aldric")))
            .unwrap();
        assert_eq!(got.active_by.as_deref(), Some("auto"));
        assert_eq!(got.active_raid.as_deref(), Some("r1"));
        assert_eq!(got.chosen_by.as_deref(), Some("Aldric"));
        assert_eq!(got.saved_at, 100);
        assert_eq!(c.current(), Some(got));
    }

    #[test]
    fn with_two_accounts_the_newer_save_decides() {
        let c = Choices::default();
        c.record(&reading("A", 200, Some("chosen"), None));
        let got = c.record(&reading("B", 100, Some("off"), None)).unwrap();
        assert_eq!(got.active_by.as_deref(), Some("chosen"));
        let got = c.record(&reading("B", 300, Some("off"), None)).unwrap();
        assert_eq!(got.active_by.as_deref(), Some("off"));
        assert_eq!(c.current(), Some(got));
    }

    #[test]
    fn a_tie_goes_to_the_first_account() {
        let c = Choices::default();
        c.record(&reading("B", 100, Some("off"), None));
        let got = c.record(&reading("A", 100, Some("none"), None)).unwrap();
        assert_eq!(got.active_by.as_deref(), Some("none"));
    }
}
