//! The screen a save state was written with, kept beside it as a PNG.
//!
//! The state file already holds this screen, but only behind a full postcard
//! decode of the whole thing — far too much to do once per row of a list. A Game
//! Boy frame packs into a few KB of PNG, so the list reads these instead.

use crate::AppConfigFile;
use core::ppu::framebuffer::FrameBuffer;
use image::codecs::png::{PngDecoder, PngEncoder};
use image::{ColorType, ExtendedColorType, ImageDecoder, ImageEncoder};
use std::fs;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Seek, Write};
use std::path::{Path, PathBuf};

const SHOT_EXT: &str = "png";
const COLOR: ColorType = ColorType::Rgb8;

pub struct StateShot {
    /// Tightly packed RGB8, `width * height * 3` long.
    pub rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl StateShot {
    pub fn of(buffer: &FrameBuffer, width: usize, height: usize) -> Self {
        Self {
            rgb: buffer.rgb888(),
            width: width as u32,
            height: height as u32,
        }
    }

    pub fn save_file(&self, game: &str, suffix: &str) -> Result<(), String> {
        let path = Self::path(game, suffix);

        if let Some(parent) = Path::new(&path).parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let file = File::create(path).map_err(|e| e.to_string())?;

        self.encode(BufWriter::new(file))
    }

    pub fn load_file(game: &str, suffix: &str) -> Result<Self, String> {
        let file = File::open(Self::path(game, suffix)).map_err(|e| e.to_string())?;

        Self::decode(BufReader::new(file))
    }

    fn encode<W: Write>(&self, writer: W) -> Result<(), String> {
        PngEncoder::new(writer)
            .write_image(&self.rgb, self.width, self.height, ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())
    }

    fn decode<R: BufRead + Seek>(reader: R) -> Result<Self, String> {
        let decoder = PngDecoder::new(reader).map_err(|e| e.to_string())?;
        let (width, height) = decoder.dimensions();

        // The decoder hands the pixels over as they are stored, and everything
        // downstream of here assumes three bytes each.
        if decoder.color_type() != COLOR {
            return Err(format!("state shot is not {COLOR:?}"));
        }

        let mut rgb = vec![0; decoder.total_bytes() as usize];
        decoder.read_image(&mut rgb).map_err(|e| e.to_string())?;

        Ok(Self { rgb, width, height })
    }

    /// A slot with no shot is not an error — every state written before shots
    /// existed is one.
    pub fn delete_file(game: &str, suffix: &str) -> Result<(), String> {
        let path = Self::path(game, suffix);

        if !path.exists() {
            return Ok(());
        }

        fs::remove_file(path).map_err(|e| e.to_string())
    }

    /// Derived from the state's own path, so the two can never land apart.
    pub fn path(game: &str, suffix: &str) -> PathBuf {
        AppConfigFile::get_save_state_path(game, suffix).with_extension(SHOT_EXT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Two rows of three pixels, so a wrong stride would show up as shifted colour.
    fn shot() -> StateShot {
        StateShot {
            rgb: vec![
                0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff, 0x11, 0x22, 0x33, 0x44, 0x55,
                0x66, 0x77, 0x88, 0x99,
            ],
            width: 3,
            height: 2,
        }
    }

    #[test]
    fn a_shot_survives_the_round_trip() {
        let mut png = Vec::new();
        shot().encode(&mut png).unwrap();

        let read = StateShot::decode(Cursor::new(png)).unwrap();

        assert_eq!(read.width, 3);
        assert_eq!(read.height, 2);
        assert_eq!(read.rgb, shot().rgb);
    }

    #[test]
    fn a_screen_of_pixels_packs_into_a_fraction_of_its_state() {
        let mut png = Vec::new();
        StateShot {
            rgb: vec![0x80; 160 * 144 * 3],
            width: 160,
            height: 144,
        }
        .encode(&mut png)
        .unwrap();

        assert!(png.len() < 160 * 144 * 3, "PNG grew past the raw pixels");
    }

    #[test]
    fn a_shot_in_another_colour_type_is_refused() {
        let mut png = Vec::new();
        image::codecs::png::PngEncoder::new(&mut png)
            .write_image(&[0; 4], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();

        assert!(StateShot::decode(Cursor::new(png)).is_err());
    }
}
