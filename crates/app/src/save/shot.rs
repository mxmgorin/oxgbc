//! The screen a save state was written with, kept beside it as a PNG.
//!
//! The state file already holds this screen, but only behind a full postcard
//! decode of the whole thing — far too much to do once per row of a list. A Game
//! Boy frame packs into a few KB of PNG, so the list reads these instead.

use crate::config::RenderConfig;
use crate::storage::image::{delete_file, RgbImage};
use core::ppu::framebuffer::FrameBuffer;
use std::path::PathBuf;

const SHOT_EXT: &str = "png";

pub fn save(
    game: &str,
    suffix: &str,
    buffer: &FrameBuffer,
    width: usize,
    height: usize,
) -> Result<(), String> {
    let shot = RgbImage {
        rgb: buffer.rgb888(),
        width: width as u32,
        height: height as u32,
    };

    shot.save_png(&path(game, suffix))
}

pub fn load(game: &str, suffix: &str) -> Result<RgbImage, String> {
    RgbImage::load_png(&path(game, suffix))
}

/// The screen out of the state file itself, for states written before shots were
/// saved beside them: `Lcd::buffer` is serialized with the rest of the PPU, but
/// only reachable behind a decode of the whole state.
pub fn load_from_state(game: &str, suffix: &str) -> Result<RgbImage, String> {
    let state = super::read_state(game, suffix)?;

    Ok(RgbImage {
        rgb: state.cpu.clock.bus.io.ppu.lcd.buffer.rgb888(),
        width: RenderConfig::WIDTH as u32,
        height: RenderConfig::HEIGHT as u32,
    })
}

/// A slot with no shot is not an error — every state written before shots existed
/// is one.
pub fn delete(game: &str, suffix: &str) -> Result<(), String> {
    delete_file(&path(game, suffix))
}

/// Derived from the state's own path, so the two can never land apart.
pub fn path(game: &str, suffix: &str) -> PathBuf {
    super::state_path(game, suffix).with_extension(SHOT_EXT)
}
