//! Where the app's own files live, and the two ways it reads what is outside:
//! walking storage itself, and decoding the pictures it finds there.

pub mod browser;
pub mod image;

use std::path::PathBuf;

/// The one directory the app writes into: config, palettes, save states, the
/// library's sidecars. Everything else derives its path from here.
pub fn get_base_dir() -> PathBuf {
    let path = sdl2::filesystem::pref_path("mxmgorin", "oxGBC").unwrap();

    PathBuf::from(path)
}
