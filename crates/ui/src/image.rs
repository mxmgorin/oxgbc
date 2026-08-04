//! Pictures the platform hands over, and the textures they are uploaded as.
//!
//! The platform passes raw pixels rather than textures because only this side has
//! an egui context to upload through, and only during a frame.

use egui::{ColorImage, TextureFilter, TextureHandle, TextureOptions, Ui};
use std::collections::HashMap;

/// Tightly packed RGB8: `width * height * 3` bytes.
pub struct RgbImage {
    pub rgb: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// Uploaded pictures, kept between frames — uploading per frame would leave a
/// texture behind every frame. Dropped wholesale when the view they came from is
/// rebuilt, since that is when what they show can have changed.
#[derive(Default)]
pub struct TextureCache {
    version: u64,
    textures: HashMap<usize, TextureHandle>,
}

impl TextureCache {
    pub(crate) fn sync(&mut self, version: u64) {
        if self.version != version {
            self.version = version;
            self.textures.clear();
        }
    }

    pub(crate) fn texture(&mut self, ui: &Ui, key: usize, image: &RgbImage) -> &TextureHandle {
        self.textures.entry(key).or_insert_with(|| {
            let pixels = ColorImage::from_rgb([image.width, image.height], &image.rgb);

            // Crisp when blown up, smooth when shrunk: nearest-neighbour
            // minification of pixel art just drops pixels.
            let options = TextureOptions {
                magnification: TextureFilter::Nearest,
                minification: TextureFilter::Linear,
                ..Default::default()
            };

            ui.ctx()
                .load_texture(format!("ui-image-{key}"), pixels, options)
        })
    }
}
