//! Picks the desktop data directory. The first match wins:
//! - an explicit path from [`DATA_DIR_FLAG`] or [`DATA_DIR_ENV`];
//! - a [`PORTABLE_DIR`] folder next to the install, which enables portable mode;
//!   paths inside the install are then stored relative to it;
//! - the platform's per-user directory.
//!
//! The install is the directory containing the executable, the `.app` bundle on
//! macOS, or the AppImage on Linux. Candidates that aren't writable are skipped.

use app::storage::Location;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const DATA_DIR_FLAG: &str = "--data-dir";
const DATA_DIR_ENV: &str = "OXGBC_DATA_DIR";
const PORTABLE_DIR: &str = "portable";
const WRITE_PROBE: &str = ".write-probe";

/// Resolves the data location and strips [`DATA_DIR_FLAG`] and its value from `args`.
pub fn resolve(args: &mut Vec<String>) -> Location {
    let explicit = take_data_dir(args).or_else(|| env::var_os(DATA_DIR_ENV).map(PathBuf::from));

    if let Some(dir) = explicit.filter(|dir| !dir.as_os_str().is_empty()) {
        match prepare(&dir) {
            Ok(base) => return Location { base, anchor: None },
            Err(err) => log::error!("Data dir {dir:?} unusable: {err}"),
        }
    }

    if let Some(install) = install_dir() {
        let portable = install.join(PORTABLE_DIR);

        if portable.is_dir() {
            match prepare(&portable) {
                Ok(base) => {
                    return Location {
                        base,
                        anchor: Some(install),
                    }
                }
                Err(err) => log::warn!("Portable dir {portable:?} not writable: {err}"),
            }
        }
    }

    Location::user()
}

/// Removes [`DATA_DIR_FLAG`] from `args` and returns its value, if any.
/// Supports both `--data-dir <path>` and `--data-dir=<path>`.
fn take_data_dir(args: &mut Vec<String>) -> Option<PathBuf> {
    let prefix = format!("{DATA_DIR_FLAG}=");
    let index = args
        .iter()
        .position(|arg| arg == DATA_DIR_FLAG || arg.starts_with(&prefix))?;
    let arg = args.remove(index);

    if let Some(value) = arg.strip_prefix(&prefix) {
        return Some(PathBuf::from(value));
    }

    if index < args.len() {
        Some(PathBuf::from(args.remove(index)))
    } else {
        log::error!("{DATA_DIR_FLAG} needs a path");
        None
    }
}

/// Creates `dir` if needed and checks that it's writable; returns its canonical path.
fn prepare(dir: &Path) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let probe = dir.join(WRITE_PROBE);
    fs::write(&probe, [])?;
    fs::remove_file(&probe)?;

    dir.canonicalize()
}

/// Returns the install directory, or `None` if it can't be determined.
/// Canonicalized to match canonicalized ROM paths.
fn install_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(image) = env::var_os("APPIMAGE") {
        return Path::new(&image).parent()?.canonicalize().ok();
    }

    let exe = env::current_exe().ok()?.canonicalize().ok()?;
    let dir = exe.parent()?;

    #[cfg(target_os = "macos")]
    if let Some(bundle) = dir
        .ancestors()
        .find(|dir| dir.extension().is_some_and(|ext| ext == "app"))
    {
        return bundle.parent().map(Path::to_path_buf);
    }

    Some(dir.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn the_data_dir_flag_leaves_the_rom_where_the_app_expects_it() {
        let mut spaced = args(&["oxgbc", "--data-dir", "/d", "game.gb"]);
        assert_eq!(take_data_dir(&mut spaced), Some(PathBuf::from("/d")));
        assert_eq!(spaced, args(&["oxgbc", "game.gb"]));

        let mut joined = args(&["oxgbc", "game.gb", "--data-dir=/d"]);
        assert_eq!(take_data_dir(&mut joined), Some(PathBuf::from("/d")));
        assert_eq!(joined, args(&["oxgbc", "game.gb"]));
    }

    #[test]
    fn a_data_dir_flag_without_a_path_is_dropped() {
        let mut dangling = args(&["oxgbc", "--data-dir"]);
        assert_eq!(take_data_dir(&mut dangling), None);
        assert_eq!(dangling, args(&["oxgbc"]));
    }
}
