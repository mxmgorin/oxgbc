//! Where the app's own files live, and the two ways it reads what is outside:
//! walking storage itself, and decoding the pictures it finds there.

pub mod browser;
pub mod image;
pub mod zip;

use std::path::PathBuf;

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

/// The one directory the app writes into: config, palettes, save states, the
/// library's sidecars. Everything else derives its path from here.
pub fn base_dir() -> PathBuf {
    let path = sdl2::filesystem::pref_path("mxmgorin", "oxGBC").unwrap();

    PathBuf::from(path)
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
}
