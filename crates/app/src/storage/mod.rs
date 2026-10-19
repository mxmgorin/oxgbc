//! Where the app's own files live, and the two ways it reads what is outside:
//! walking storage itself, and decoding the pictures it finds there.

pub mod browser;
pub mod image;
pub mod zip;

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// What counts as a game, for the shelf and both browsers — they disagreed once
/// and a folder of zips came out empty. [`zip::unzip_rom`] opens the zips.
pub const ROM_EXTENSIONS: &[&str] = &["gb", "gbc", "zip"];

/// By name rather than path: Android hands out `content://` URIs.
pub fn is_rom_file(file: &str) -> bool {
    let Some((_, extension)) = file.rsplit_once('.') else {
        return false;
    };

    ROM_EXTENSIONS
        .iter()
        .any(|rom| rom.eq_ignore_ascii_case(extension))
}

/// Where the app stores its files. Set once at startup via [`init`].
#[derive(Debug, Clone)]
pub struct Location {
    /// Directory for config, saves and other app data.
    pub base: PathBuf,
    /// Root that stored paths are made relative to, so the folder can be moved.
    /// `None` leaves paths unchanged.
    pub anchor: Option<PathBuf>,
}

impl Location {
    /// Uses the platform's per-user data directory.
    pub fn user() -> Self {
        let path = sdl2::filesystem::pref_path("mxmgorin", "oxGBC")
            .expect("SDL resolves a per-user directory on every supported platform");

        Self {
            base: PathBuf::from(path),
            anchor: None,
        }
    }

    /// Returns `path` relative to the anchor if it is inside it, otherwise unchanged.
    pub fn to_stored(&self, path: &Path) -> PathBuf {
        self.anchor
            .as_deref()
            .and_then(|anchor| path.strip_prefix(anchor).ok())
            .unwrap_or(path)
            .to_path_buf()
    }

    /// Resolves a stored path against the anchor; inverse of [`Self::to_stored`].
    pub fn from_stored(&self, path: &Path) -> PathBuf {
        match self.anchor.as_deref() {
            Some(anchor) if path.is_relative() => anchor.join(path),
            _ => path.to_path_buf(),
        }
    }
}

static LOCATION: OnceLock<Location> = OnceLock::new();

/// Sets the storage location; returns `false` if it is already set.
/// Must be called before any path is resolved, otherwise [`Location::user`] is used.
pub fn init(location: Location) -> bool {
    LOCATION.set(location).is_ok()
}

fn location() -> &'static Location {
    LOCATION.get_or_init(Location::user)
}

/// The one directory the app writes into: config, palettes, save states, the
/// library's sidecars. Everything else derives its path from here.
pub fn base_dir() -> &'static Path {
    &location().base
}

/// [`Location::to_stored`] for the current location.
pub fn to_stored(path: &Path) -> PathBuf {
    location().to_stored(path)
}

/// [`Location::from_stored`] for the current location.
pub fn from_stored(path: &Path) -> PathBuf {
    location().from_stored(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_game_is_recognized_however_its_extension_is_spelled() {
        assert!(is_rom_file("Aerostar.zip"));
        assert!(is_rom_file("/roms/gb/ZELDA.GB"));
        assert!(is_rom_file("content://tree/primary%3ARoms%2FWario.gbc"));

        assert!(!is_rom_file("Astro Rabby.7z"));
        assert!(!is_rom_file("gamelist"));
    }

    fn portable() -> Location {
        Location {
            base: PathBuf::from("/stick/oxgbc/portable"),
            anchor: Some(PathBuf::from("/stick/oxgbc")),
        }
    }

    #[test]
    fn a_path_inside_the_anchor_is_stored_relative_and_read_back_whole() {
        let location = portable();
        let rom = Path::new("/stick/oxgbc/roms/Tetris.gb");

        let stored = location.to_stored(rom);
        assert_eq!(stored, Path::new("roms/Tetris.gb"));
        assert_eq!(location.from_stored(&stored), rom);
    }

    #[test]
    fn a_path_outside_the_anchor_is_stored_as_it_came() {
        let rom = Path::new("/home/me/roms/Tetris.gb");

        assert_eq!(portable().to_stored(rom), rom);
        assert_eq!(portable().from_stored(rom), rom);
    }

    #[test]
    fn without_an_anchor_paths_pass_through() {
        let location = Location {
            base: PathBuf::from("/home/me/.local/share/oxGBC"),
            anchor: None,
        };
        let uri = Path::new("content://tree/primary%3ARoms%2FWario.gbc");

        assert_eq!(location.to_stored(uri), uri);
        assert_eq!(location.from_stored(uri), uri);
    }
}
