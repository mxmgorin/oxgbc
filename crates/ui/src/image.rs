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

/// How many pictures stay uploaded. Far above what a screen draws at once, so a
/// frame never evicts what it is using, and far below a long library, which would
/// otherwise end up in video memory in its entirety on the way past.
const KEPT: usize = 64;

/// Uploaded pictures, kept between frames — uploading per frame would leave a
/// texture behind every frame. Dropped wholesale when the view they came from is
/// rebuilt, since that is when what they show can have changed, and otherwise
/// oldest-drawn-first once there are more than [`KEPT`].
#[derive(Default)]
pub struct TextureCache {
    version: u64,
    frame: u64,
    textures: HashMap<usize, Cached>,
}

struct Cached {
    texture: TextureHandle,
    /// Last frame it was drawn in, which is what decides who goes first.
    drawn: u64,
}

impl TextureCache {
    pub(crate) fn sync(&mut self, version: u64) {
        self.frame += 1;

        if self.version != version {
            self.version = version;
            self.textures.clear();
        }

        self.evict_oldest();
    }

    /// Drops what is uploaded, for pictures that will not be drawn again.
    pub(crate) fn clear(&mut self) {
        self.textures.clear();
    }

    pub(crate) fn texture(&mut self, ui: &Ui, key: usize, image: &RgbImage) -> &TextureHandle {
        let frame = self.frame;
        let cached = self.textures.entry(key).or_insert_with(|| Cached {
            texture: upload(ui, key, image),
            drawn: frame,
        });
        cached.drawn = frame;

        &cached.texture
    }

    /// Everything drawn in one frame shares a stamp, so a frame drawing more than
    /// [`KEPT`] pictures keeps all of them rather than dropping one it is about to use.
    fn evict_oldest(&mut self) {
        if self.textures.len() <= KEPT {
            return;
        }

        let mut drawn: Vec<u64> = self.textures.values().map(|cached| cached.drawn).collect();
        let cut = *drawn.select_nth_unstable(self.textures.len() - KEPT).1;
        self.textures.retain(|_, cached| cached.drawn >= cut);
    }
}

fn upload(ui: &Ui, key: usize, image: &RgbImage) -> TextureHandle {
    let pixels = ColorImage::from_rgb([image.width, image.height], &image.rgb);

    // Crisp when blown up, smooth when shrunk: nearest-neighbour minification of
    // pixel art just drops pixels.
    let options = TextureOptions {
        magnification: TextureFilter::Nearest,
        minification: TextureFilter::Linear,
        ..Default::default()
    };

    ui.ctx()
        .load_texture(format!("ui-image-{key}"), pixels, options)
}
