//! Puts a checked add-on zip in place in a game version folder.
//!
//! The zip is read whole before the game folder is touched: every entry must sit inside one
//! `BureaucracyForever` folder, and its `.toc` must name the release's version. It is then
//! unpacked beside the live add-on, takes the officer's `Inbox.lua` from the live folder,
//! and is swapped in with two renames. If the second rename fails, the first is undone, so
//! the old add-on, inbox included, stays exactly as it was; if even that fails, `recover`
//! puts it back at the next check or install. The temporary folders start
//! with a dot and their `.toc` doesn't match their name, so the game never loads them.

use crate::game_folder::ADDON_FOLDER;
use crate::inbox::ADDON_FOLDER_LOCK;
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::{Component, Path, PathBuf};
use zip::ZipArchive;

const INBOX: &str = "Inbox.lua";
const NEW: &str = ".BureaucracyForever-new";
const OLD: &str = ".BureaucracyForever-old";
/// The most the unpacked add-on may hold; it is well under 1 MB.
const MAX_UNPACKED: u64 = 50 * 1024 * 1024;

/// The `## Version:` line of a `.toc`.
fn toc_version(toc: &str) -> Option<String> {
    toc.lines()
        .map(|line| line.trim_start_matches('\u{feff}').trim())
        .find_map(|line| line.strip_prefix("## Version:"))
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// The installed add-on's version, from its `.toc`; None when there is no add-on.
pub fn installed_version(version_folder: &Path) -> Option<String> {
    let toc = addons(version_folder)
        .join(ADDON_FOLDER)
        .join(format!("{ADDON_FOLDER}.toc"));
    toc_version(&fs::read_to_string(toc).ok()?)
}

fn addons(version_folder: &Path) -> PathBuf {
    version_folder.join("Interface").join("AddOns")
}

/// A zip that passed the checks: each file's path inside the add-on folder, with its index.
pub struct Checked<'a> {
    archive: ZipArchive<Cursor<&'a [u8]>>,
    files: Vec<(usize, PathBuf)>,
}

/// Checks the zip without unpacking it. Any problem gives None.
pub fn check<'a>(bytes: &'a [u8], version: &str) -> Option<Checked<'a>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut files = vec![];
    let mut total = 0u64;
    for index in 0..archive.len() {
        let entry = archive.by_index(index).ok()?;
        let path = entry.enclosed_name()?;
        let mut parts = path.components();
        if parts.next() != Some(Component::Normal(ADDON_FOLDER.as_ref())) || entry.is_symlink() {
            return None;
        }
        total += entry.size();
        let inside: PathBuf = parts.collect();
        if !entry.is_dir() && !inside.as_os_str().is_empty() {
            files.push((index, inside));
        }
    }
    let mut toc = String::new();
    archive
        .by_name(&format!("{ADDON_FOLDER}/{ADDON_FOLDER}.toc"))
        .ok()?
        .take(MAX_UNPACKED)
        .read_to_string(&mut toc)
        .ok()?;
    let matches = toc_version(&toc).as_deref() == Some(version);
    (matches && total <= MAX_UNPACKED).then_some(Checked { archive, files })
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The folder isn't a game version folder (it has no `Interface` folder).
    NotGameFolder,
    /// Unpacking or the swap failed; the old add-on is as it was.
    Failed,
    /// The swap failed and the old add-on couldn't be moved back; `recover` moves it later.
    NotRestored,
}

/// Puts back an add-on a swap left aside (the helper closed between the renames, or the
/// move back failed). Does nothing when the add-on is in place.
pub fn recover(version_folder: &Path) {
    let _folder = ADDON_FOLDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = restore(&addons(version_folder), |from, to| fs::rename(from, to));
}

fn restore(addons: &Path, rename: impl Fn(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    let (live, old) = (addons.join(ADDON_FOLDER), addons.join(OLD));
    match !live.exists() && old.is_dir() {
        true => rename(&old, &live),
        false => Ok(()),
    }
}

/// Unpacks `zip` into `version_folder`'s add-ons and swaps it in, keeping the live inbox.
pub fn put_in_place(version_folder: &Path, zip: Checked) -> Result<(), PlaceError> {
    put_in_place_with(version_folder, zip, |from, to| fs::rename(from, to))
}

/// `put_in_place` with the rename it swaps with, so tests can make the swap fail.
pub fn put_in_place_with(
    version_folder: &Path,
    zip: Checked,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> Result<(), PlaceError> {
    if !version_folder.join("Interface").is_dir() {
        return Err(PlaceError::NotGameFolder);
    }
    // No inbox write may land between the copy of the inbox and the swap.
    let _folder = ADDON_FOLDER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let addons = addons(version_folder);
    let (live, new, old) = (
        addons.join(ADDON_FOLDER),
        addons.join(NEW),
        addons.join(OLD),
    );
    restore(&addons, &rename).map_err(|_| PlaceError::Failed)?;
    let _ = fs::remove_dir_all(&new);
    let _ = fs::remove_dir_all(&old);
    // A leftover that can't be removed would mix its files into the new add-on.
    if new.exists() || old.exists() {
        return Err(PlaceError::Failed);
    }
    let unpacked = fs::create_dir_all(&new)
        .and_then(|()| unpack(zip, &new))
        .and_then(|()| match live.join(INBOX).is_file() {
            true => fs::copy(live.join(INBOX), new.join(INBOX)).map(|_| ()),
            false => Ok(()),
        });
    if unpacked.is_err() {
        let _ = fs::remove_dir_all(&new);
        return Err(PlaceError::Failed);
    }
    let had_live = live.exists();
    if had_live && rename(&live, &old).is_err() {
        let _ = fs::remove_dir_all(&new);
        return Err(PlaceError::Failed);
    }
    if rename(&new, &live).is_err() {
        let _ = fs::remove_dir_all(&new);
        return match had_live && rename(&old, &live).is_err() {
            true => Err(PlaceError::NotRestored),
            false => Err(PlaceError::Failed),
        };
    }
    // A copy that can't be removed now is removed by the next install.
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

fn unpack(mut zip: Checked, into: &Path) -> io::Result<()> {
    for (index, inside) in &zip.files {
        let target = into.join(inside);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let entry = zip.archive.by_index(*index).map_err(io::Error::other)?;
        io::copy(
            &mut entry.take(MAX_UNPACKED),
            &mut fs::File::create(target)?,
        )?;
    }
    Ok(())
}
