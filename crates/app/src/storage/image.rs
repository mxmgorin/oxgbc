//! Reading and writing the app's own images — save-state screens and cartridge
//! covers — as plain RGB8 PNGs, whatever they were imported from.

use image::codecs::png::{PngDecoder, PngEncoder};
use image::{ColorType, ExtendedColorType, ImageDecoder, ImageEncoder, ImageReader};
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

const COLOR: ColorType = ColorType::Rgb8;

pub struct RgbImage {
    /// Tightly packed RGB8, `width * height * 3` long.
    pub rgb: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl RgbImage {
    pub fn save_png(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let file = File::create(path).map_err(|e| e.to_string())?;

        self.encode(BufWriter::new(file))
    }

    pub fn load_png(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        let decoder = PngDecoder::new(BufReader::new(file)).map_err(|e| e.to_string())?;
        let (width, height) = decoder.dimensions();

        // The decoder hands the pixels over as they are stored, and everything
        // downstream of here assumes three bytes each.
        if decoder.color_type() != COLOR {
            return Err(format!("image is not {COLOR:?}"));
        }

        let mut rgb = vec![0; decoder.total_bytes() as usize];
        decoder.read_image(&mut rgb).map_err(|e| e.to_string())?;

        Ok(Self { rgb, width, height })
    }

    /// Anything the `image` crate can read, brought in as RGB8 and shrunk so its
    /// longer side is at most `max_side` — a cover scanned at print resolution
    /// would otherwise be uploaded whole to draw a thumbnail from.
    pub fn import(path: &Path, max_side: u32) -> Result<Self, String> {
        let read = ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?
            .decode()
            .map_err(|e| e.to_string())?;
        let side = read.width().max(read.height());

        let read = if side > max_side {
            let scale = max_side as f32 / side as f32;
            let width = (read.width() as f32 * scale).round().max(1.0) as u32;
            let height = (read.height() as f32 * scale).round().max(1.0) as u32;

            read.resize(width, height, image::imageops::FilterType::Triangle)
        } else {
            read
        };
        let rgb = read.to_rgb8();

        Ok(Self {
            width: rgb.width(),
            height: rgb.height(),
            rgb: rgb.into_raw(),
        })
    }

    fn encode<W: Write>(&self, writer: W) -> Result<(), String> {
        PngEncoder::new(writer)
            .write_image(&self.rgb, self.width, self.height, ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())
    }
}

/// A missing file is not an error — nothing was there to remove.
pub fn delete_file(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }

    fs::remove_file(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Two rows of three pixels, so a wrong stride would show up as shifted colour.
    fn image() -> RgbImage {
        RgbImage {
            rgb: vec![
                0xff, 0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff, 0x11, 0x22, 0x33, 0x44, 0x55,
                0x66, 0x77, 0x88, 0x99,
            ],
            width: 3,
            height: 2,
        }
    }

    fn decode(png: Vec<u8>) -> Result<RgbImage, String> {
        let decoder = PngDecoder::new(Cursor::new(png)).map_err(|e| e.to_string())?;

        if decoder.color_type() != COLOR {
            return Err("not rgb8".to_owned());
        }

        let (width, height) = decoder.dimensions();
        let mut rgb = vec![0; decoder.total_bytes() as usize];
        decoder.read_image(&mut rgb).map_err(|e| e.to_string())?;

        Ok(RgbImage { rgb, width, height })
    }

    #[test]
    fn an_image_survives_the_round_trip() {
        let mut png = Vec::new();
        image().encode(&mut png).unwrap();

        let read = decode(png).unwrap();

        assert_eq!((read.width, read.height), (3, 2));
        assert_eq!(read.rgb, image().rgb);
    }

    #[test]
    fn a_screen_of_pixels_packs_into_a_fraction_of_a_save_state() {
        let mut png = Vec::new();
        RgbImage {
            rgb: vec![0x80; 160 * 144 * 3],
            width: 160,
            height: 144,
        }
        .encode(&mut png)
        .unwrap();

        assert!(png.len() < 160 * 144 * 3, "PNG grew past the raw pixels");
    }

    #[test]
    fn an_image_in_another_colour_type_is_refused() {
        let mut png = Vec::new();
        PngEncoder::new(&mut png)
            .write_image(&[0; 4], 1, 1, ExtendedColorType::Rgba8)
            .unwrap();

        assert!(decode(png).is_err());
    }
}
