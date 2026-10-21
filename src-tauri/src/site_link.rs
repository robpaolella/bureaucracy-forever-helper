//! Opens the loot log on the guild site from the window's "Open the loot log" button.

use crate::commands::{staging, Engine};
use crate::key::Site;

/// The loot log on a raid's page, or the calendar when no raid is known. None unless the raid
/// id is only letters, digits, `-` and `_`, so the window can't open any other address.
pub fn loot_log_url(site: Site, raid: Option<&str>) -> Option<String> {
    let base = site.address();
    match raid {
        None => Some(format!("{base}/members/calendar")),
        Some(id) => {
            let plain = !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
            plain.then(|| format!("{base}/members/calendar/{id}#raid-loot"))
        }
    }
}

/// Opens the loot log of `raid` (the calendar if none) on the site the helper is using.
#[tauri::command]
pub fn open_loot_log(
    app: tauri::AppHandle,
    engine: tauri::State<'_, Engine>,
    raid: Option<String>,
) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let site = if staging(&engine) {
        Site::Staging
    } else {
        Site::Production
    };
    let url = loot_log_url(site, raid.as_deref()).ok_or("That isn't a raid.")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_the_raid_page_loot_log_on_the_current_site() {
        assert_eq!(
            loot_log_url(Site::Production, Some("clx9_a-1")).as_deref(),
            Some("https://www.bureauguild.com/members/calendar/clx9_a-1#raid-loot")
        );
        assert_eq!(
            loot_log_url(Site::Staging, Some("r1")).as_deref(),
            Some("https://staging.bureauguild.com/members/calendar/r1#raid-loot")
        );
    }

    #[test]
    fn falls_back_to_the_calendar_without_a_raid() {
        assert_eq!(
            loot_log_url(Site::Production, None).as_deref(),
            Some("https://www.bureauguild.com/members/calendar")
        );
    }

    #[test]
    fn refuses_an_id_that_could_point_elsewhere() {
        for id in [
            "",
            "a/b",
            "..",
            "a?x=1",
            "a#b",
            "a b",
            "a@evil.test",
            "a\\b",
        ] {
            assert_eq!(loot_log_url(Site::Production, Some(id)), None, "{id:?}");
        }
    }
}
