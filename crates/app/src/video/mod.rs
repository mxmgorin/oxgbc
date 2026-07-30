use crate::config::ScaleMode;
use crate::config::VideoConfig;
use crate::video::gl_backend::GlBackend;
use crate::video::sdl2_backend::Sdl2Backend;
use core::ppu::framebuffer::FrameBuffer;
use core::ppu::tile::PixelColor;
use core::ppu::tile::TileData;
use core::ppu::LCD_X_RES;
use core::ppu::LCD_Y_RES;
use sdl2::rect::Rect;

mod font;
pub mod frame_blend;
mod sdl2_filters;
pub mod text;
mod video;
pub use video::*;

mod gl_backend;
mod overlay;
mod sdl2_backend;
pub mod sdl2_tiles;
pub mod shader;

pub fn calc_win_height(scale: u32) -> u32 {
    LCD_Y_RES as u32 * scale
}

pub fn calc_win_width(scale: u32) -> u32 {
    LCD_X_RES as u32 * scale
}

pub fn new_scaled_rect(mode: ScaleMode, window_width: u32, window_height: u32) -> Rect {
    let (new_width, new_height) = match mode {
        ScaleMode::Integer => scale_integer(window_width, window_height),
        ScaleMode::Fit => scale_fit(window_width, window_height),
        ScaleMode::Stretch => (window_width, window_height),
    };

    // Center the image in the screen
    let x = ((window_width - new_width) / 2) as i32;
    let y = ((window_height - new_height) / 2) as i32;

    Rect::new(x, y, new_width, new_height)
}

pub fn scale_fit(window_width: u32, window_height: u32) -> (u32, u32) {
    let screen_aspect = window_width as f32 / window_height as f32;
    let game_aspect = LCD_X_RES as f32 / LCD_Y_RES as f32;

    let (new_width, new_height) = if screen_aspect > game_aspect {
        // Screen is wider than game: Fit height, adjust width
        let height = window_height;
        let width = ((height as f32) * game_aspect) as u32;
        (width, height)
    } else {
        // Screen is taller than game: Fit width, adjust height
        let width = window_width;
        let height = ((width as f32) / game_aspect) as u32;
        (width, height)
    };

    (new_width, new_height)
}

pub fn scale_integer(window_width: u32, window_height: u32) -> (u32, u32) {
    let scale_x = window_width / LCD_X_RES as u32;
    let scale_y = window_height / LCD_Y_RES as u32;

    // Largest integer scale that fits
    let scale = scale_x.min(scale_y).max(1);

    let new_width = LCD_X_RES as u32 * scale;
    let new_height = LCD_Y_RES as u32 * scale;

    (new_width, new_height)
}

pub struct VideoTexture {
    pub pitch: usize,
    pub buffer: Box<[u8]>,
    pub rect: Rect,
    pub bytes_per_pixel: usize,
}

impl VideoTexture {
    pub fn new(rect: Rect, bytes_per_pixel: usize) -> Self {
        let pitch = rect.w as usize * bytes_per_pixel;

        Self {
            pitch,
            buffer: vec![0; pitch * rect.h as usize].into_boxed_slice(),
            rect,
            bytes_per_pixel,
        }
    }

    pub fn clear(&mut self) {
        for i in (0..self.buffer.len()).step_by(self.bytes_per_pixel) {
            self.buffer[i] = 0;
            self.buffer[i + 1] = 0;
            self.buffer[i + 2] = 0;

            if self.bytes_per_pixel == 4 {
                self.buffer[i + 3] = 0;
            }
        }
    }
}

#[inline]
pub fn fill_buffer(fb: &mut FrameBuffer, color: PixelColor) {
    for i in (0..fb.len()).step_by(FrameBuffer::BYTES_PER_PIXEL) {
        draw_color(fb, i, color);
    }
}

#[inline]
pub fn draw_color(fb: &mut FrameBuffer, index: usize, color: PixelColor) {
    // Use const generic indirectly via const
    const BPP: usize = FrameBuffer::BYTES_PER_PIXEL;

    let bytes: &[u8] = match BPP {
        2 => &color.as_rgb565_bytes(),
        3 => &color.as_rgb_bytes(),
        4 => &color.as_rgba_bytes(),
        _ => panic!("Unsupported pixel size"),
    };

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), fb.as_mut_ptr().add(index), BPP);
    }
}

pub fn truncate_text(s: &str, max_chars: usize) -> String {
    let max_len = s.len().min(max_chars + 2);
    let mut truncated = String::with_capacity(max_len);

    for (i, ch) in s.chars().enumerate() {
        if i == max_chars {
            let ends_with_paren = s.ends_with(')');
            let total_chars = s.chars().count();

            if total_chars > max_chars + 1 || !ends_with_paren {
                truncated.push('…');
            }

            if ends_with_paren {
                truncated.push(')');
            }

            break;
        }

        truncated.push(ch);
    }

    truncated
}

pub enum VideoBackend {
    Sdl2(Box<Sdl2Backend>),
    Gl(GlBackend),
}

impl VideoBackend {
    #[inline]
    pub fn draw_buffer(&mut self, buffer: &[u8], config: &VideoConfig) {
        match self {
            VideoBackend::Sdl2(x) => x.draw_buffer(buffer, config),
            VideoBackend::Gl(x) => x.draw_buffer(buffer),
        }
    }

    #[inline]
    pub fn draw_menu(&mut self, buffer: &[u8], config: &VideoConfig) {
        match self {
            VideoBackend::Sdl2(x) => x.draw_menu(buffer, config),
            VideoBackend::Gl(x) => x.draw_menu(buffer),
        }
    }

    #[inline]
    pub fn show(&mut self) {
        match self {
            VideoBackend::Sdl2(x) => x.show(),
            VideoBackend::Gl(x) => x.show(),
        }
    }

    pub fn set_scale(&mut self, scale: u32, mode: ScaleMode) -> Result<(), String> {
        match self {
            VideoBackend::Sdl2(x) => x.set_scale(scale, mode),
            VideoBackend::Gl(x) => x.set_scale(scale, mode),
        }
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, scale_mode: ScaleMode) {
        match self {
            VideoBackend::Sdl2(x) => x.set_fullscreen(fullscreen, scale_mode),
            VideoBackend::Gl(x) => x.set_fullscreen(fullscreen, scale_mode),
        }
    }

    pub fn handle_resize(&mut self, mode: ScaleMode) {
        match self {
            VideoBackend::Sdl2(x) => x.handle_resize(mode),
            VideoBackend::Gl(x) => x.handle_resize(mode),
        }
    }

    pub fn update_config(&mut self, config: &VideoConfig) {
        match self {
            VideoBackend::Sdl2(x) => x.update_config(config),
            VideoBackend::Gl(x) => x.update_config(&config.render),
        }
    }
    pub fn draw_tiles(&mut self, tiles: impl Iterator<Item = TileData>) {
        match self {
            VideoBackend::Sdl2(x) => x.draw_tiles(tiles),
            VideoBackend::Gl(_) => {}
        }
    }

    pub fn close_window(&mut self, id: u32) -> bool {
        match self {
            VideoBackend::Sdl2(x) => x.close_window(id),
            VideoBackend::Gl(x) => x.close_window(id),
        }
    }

    /// Returns whether egui consumed the event.
    #[cfg(feature = "frontend-modern")]
    pub fn egui_on_event(&mut self, event: &sdl2::event::Event) -> bool {
        match self {
            VideoBackend::Sdl2(x) => x.egui_on_event(event),
            VideoBackend::Gl(x) => x.egui_on_event(event),
        }
    }

    #[cfg(feature = "frontend-modern")]
    pub fn render_egui(&mut self, run_ui: &mut dyn FnMut(&egui_sdl2::egui::Context)) {
        match self {
            VideoBackend::Sdl2(x) => x.render_egui(run_ui),
            VideoBackend::Gl(x) => x.render_egui(run_ui),
        }
    }

    #[cfg(feature = "frontend-modern")]
    pub fn egui_repaint_delay(&self) -> std::time::Duration {
        match self {
            VideoBackend::Sdl2(x) => x.egui_repaint_delay(),
            VideoBackend::Gl(x) => x.egui_repaint_delay(),
        }
    }

    #[cfg(feature = "frontend-modern")]
    pub fn destroy_egui(&mut self) {
        match self {
            VideoBackend::Sdl2(x) => x.destroy_egui(),
            VideoBackend::Gl(x) => x.destroy_egui(),
        }
    }
}
