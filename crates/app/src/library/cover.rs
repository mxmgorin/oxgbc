//! Cover art for a cartridge, kept beside its metadata in `library/`.
//!
//! Stored as RGB8 PNG whatever it was imported from, and no larger than a shelf
//! ever needs: a scan at print resolution would otherwise be uploaded whole to
//! draw a tile an inch wide.

use crate::library::meta::RomMeta;
use crate::storage::image::{delete_file, RgbImage};
use std::path::{Path, PathBuf};

const COVER_EXT: &str = "png";
/// Longer side a stored cover is shrunk to.
const MAX_SIDE: u32 = 384;

pub fn import(game: &str, source: &Path) -> Result<(), String> {
    let cover = RgbImage::import(source, MAX_SIDE)?;

    cover.save_png(&path(game))
}

/// From pixels already in hand rather than a file. Callers pass a Game Boy screen,
/// which is well inside [`MAX_SIDE`], so nothing is shrunk here.
pub fn set(game: &str, cover: &RgbImage) -> Result<(), String> {
    cover.save_png(&path(game))
}

pub fn load(game: &str) -> Result<RgbImage, String> {
    RgbImage::load_png(&path(game))
}

/// A game with no cover is not an error — most have none.
pub fn delete(game: &str) -> Result<(), String> {
    delete_file(&path(game))
}

/// Beside the game's metadata, so the two travel together.
pub fn path(game: &str) -> PathBuf {
    RomMeta::path(game).with_extension(COVER_EXT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cover_sits_beside_the_metadata_it_belongs_to() {
        let meta = RomMeta::path("Zelda.gbc");
        let cover = path("Zelda.gbc");

        assert_eq!(meta.parent(), cover.parent());
        assert_eq!(cover.file_name().unwrap(), "Zelda.gbc.png");
    }
}
