//! Drives the storage walk behind the UI's browse screen. The walking itself is
//! [`FileBrowser`]'s, shared with the text menu; this only starts it, turns it into
//! a view and moves it a row at a time.

use crate::frontend::BrowseTarget;
use crate::storage::browser::{FileBrowser, FILE_BROWSER_BACK_ITEM};
use std::path::{Path, PathBuf};

/// A walk shows only what it can pick: games, or pictures, or nothing but folders.
const ROM_EXTENSIONS: &[&str] = &["gb", "gbc", "zip"];
const COVER_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg"];
const FOLDERS_ONLY: &[&str] = &[];
/// The walk keeps its own selection for the text menu's paging; the modern screen
/// keeps its own focus and only needs the whole listing, so one page is all of it.
const ALL_ENTRIES: usize = usize::MAX;
const UP: &str = "..";

pub fn start(target: &BrowseTarget, from: Option<&Path>) -> Option<FileBrowser> {
    let extensions: &'static [&'static str] = match target {
        BrowseTarget::Rom => ROM_EXTENSIONS,
        BrowseTarget::Cover(_) => COVER_EXTENSIONS,
        BrowseTarget::Dir => FOLDERS_ONLY,
    };
    // Where the last walk left off, and the working directory the first time.
    let from = from
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));

    match FileBrowser::new(&from, ALL_ENTRIES, extensions) {
        Ok(walk) => Some(walk),
        Err(err) => {
            log::warn!("Failed open browse at {from:?}: {err}");
            FileBrowser::new(".", ALL_ENTRIES, extensions).ok()
        }
    }
}

pub fn view(walk: Option<&FileBrowser>, target: &BrowseTarget) -> ui::BrowseView {
    let Some(walk) = walk else {
        return ui::BrowseView::default();
    };
    let entries = walk
        .entries()
        .iter()
        // The walk leads with a sentinel for the way up, which is a row like any
        // other here — entering it goes up.
        .filter(|path| path.as_path() != Path::new(FILE_BROWSER_BACK_ITEM) || walk.has_parent())
        .map(|path| ui::BrowseEntry {
            name: if path.as_path() == Path::new(FILE_BROWSER_BACK_ITEM) {
                UP.to_owned()
            } else {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            },
            is_dir: path.is_dir() || path.as_path() == Path::new(FILE_BROWSER_BACK_ITEM),
        })
        .collect();

    ui::BrowseView {
        target: into_ui_target(target),
        path: walk.current_dir.to_string_lossy().into_owned(),
        entries,
    }
}

/// `Some` once a game has been picked; a folder only moves the walk along.
pub fn enter(walk: &mut FileBrowser, index: usize) -> Option<PathBuf> {
    // At the root there is no way up to show, so the view's rows start one along
    // from the walk's own.
    let index = index + usize::from(!walk.has_parent());
    walk.select(index);

    match walk.enter() {
        Ok(picked) => picked,
        Err(err) => {
            log::warn!("Failed browse enter: {err}");
            None
        }
    }
}

/// All the screen needs is whether a folder can be taken.
fn into_ui_target(target: &BrowseTarget) -> ui::BrowseTarget {
    match target {
        BrowseTarget::Dir => ui::BrowseTarget::Dir,
        BrowseTarget::Rom | BrowseTarget::Cover(_) => ui::BrowseTarget::File,
    }
}
