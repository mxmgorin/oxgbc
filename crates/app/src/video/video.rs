use crate::config::{ScaleMode, VideoBackendType, VideoConfig};
use crate::video::frame_blend::FrameBlend;
use crate::video::gl_backend::GlBackend;
use crate::video::overlay::Overlay;
use crate::video::sdl2_backend::Sdl2Backend;
#[cfg(feature = "frontend-modern")]
use crate::video::DrawUi;
use crate::video::{calc_win_height, calc_win_width, new_scaled_rect, VideoBackend};
use core::ppu::framebuffer::FrameBuffer;
use core::ppu::tile::PixelColor;
use core::ppu::tile::TileData;
use sdl2::Sdl;
use std::time::{Duration, Instant};

pub struct AppVideo {
    frame_blend: Option<FrameBlend>,
    backend: VideoBackend,
    config: VideoConfig,
    last_render_time: Instant,
    /// Whether the backend's frame texture still holds what [`Self::draw_backdrop`]
    /// would upload. A menu frame draws the same paused picture as the one before it,
    /// so only something writing into the framebuffer — or a backend that rebuilt its
    /// texture — makes the upload worth doing again.
    backdrop_uploaded: bool,
    pub min_render_interval: Duration,
    /// Private, so the text it draws into the framebuffer cannot be written behind
    /// [`Self::backdrop_uploaded`]'s back.
    overlay: Overlay,
}

impl AppVideo {
    pub fn new(
        sdl: &Sdl,
        text_color: PixelColor,
        bg_color: PixelColor,
        config: &VideoConfig,
    ) -> Result<Self, String> {
        let scale = config.interface.scale as u32;
        let win_width = calc_win_width(scale);
        let win_height = calc_win_height(scale);
        let game_rect = new_scaled_rect(config.interface.scale_mode, win_width, win_height);

        let mut backend = match config.render.backend {
            VideoBackendType::Sdl2 => {
                let backend = Sdl2Backend::new(sdl, config, game_rect);
                VideoBackend::Sdl2(Box::new(backend))
            }
            VideoBackendType::Gl => {
                let backend = GlBackend::new(sdl, game_rect, config)?;
                VideoBackend::Gl(backend)
            }
        };
        backend.set_fullscreen(config.interface.is_fullscreen, config.interface.scale_mode);
        let overlay = Overlay::new(text_color, bg_color);

        Ok(Self {
            frame_blend: FrameBlend::new(&config.render.frame_blend_mode),
            config: config.clone(),
            last_render_time: Instant::now(),
            min_render_interval: config.render.calc_min_frame_interval(),
            backdrop_uploaded: false,
            backend,
            overlay,
        })
    }

    /// Closes the window and returns true when main window is closed.
    pub fn close_window(&mut self, id: u32) -> bool {
        self.backend.close_window(id)
    }

    pub fn update_config(&mut self, config: &VideoConfig) {
        self.min_render_interval = config.render.calc_min_frame_interval();
        self.frame_blend = FrameBlend::new(&config.render.frame_blend_mode);
        self.backend
            .set_fullscreen(config.interface.is_fullscreen, config.interface.scale_mode);
        self.backend.update_config(config);
        self.config = config.clone();
        // Loading a shader makes the frame texture over, so whatever was in it is gone.
        self.backdrop_changed();
    }

    #[inline]
    pub fn draw_buffer(&mut self, buffer: &[u8]) {
        let buffer = if let Some(blend) = &mut self.frame_blend {
            blend.process_buffer(buffer, &self.config)
        } else {
            buffer
        };

        self.backend.draw_buffer(buffer, &self.config);
        // What went up is the blended frame, which is not what a backdrop draws.
        self.backdrop_changed();
    }

    /// Draws the paused frame the menu sits over, uploading it only when the texture
    /// does not already hold it.
    #[inline(always)]
    pub fn draw_backdrop(&mut self, buffer: &[u8]) {
        let fresh = (!self.backdrop_uploaded).then_some(buffer);
        self.backend.draw_backdrop(fresh, &self.config);
        self.backdrop_uploaded = true;
    }

    /// The framebuffer was written into, so the texture no longer holds it.
    #[inline(always)]
    fn backdrop_changed(&mut self) {
        self.backdrop_uploaded = false;
    }

    /// The text menu's screen, which is the whole framebuffer.
    pub fn fill_menu(&mut self, fb: &mut FrameBuffer, lines: &[&str], center: bool, align: bool) {
        self.overlay.fill_menu(fb, lines, center, align);
        self.backdrop_changed();
    }

    #[inline(always)]
    pub fn fill_fps(&mut self, fb: &mut FrameBuffer, text: &str) {
        self.overlay.fill_fps(fb, text);
        self.backdrop_changed();
    }

    /// Notification lines, drawn into the frame itself so both frontends get them.
    /// Nothing is written for none of them — and nothing is erased either, which is
    /// what leaves a line standing in a frozen frame until one is drawn over it.
    #[inline(always)]
    pub fn fill_notif(&mut self, fb: &mut FrameBuffer, lines: &[&str]) {
        if lines.is_empty() {
            return;
        }

        self.overlay.fill_notif(fb, lines);
        self.backdrop_changed();
    }

    pub fn set_overlay_colors(&mut self, text_color: PixelColor, bg_color: PixelColor) {
        self.overlay.text_color = text_color;
        self.overlay.bg_color = bg_color;
    }

    #[inline(always)]
    pub fn draw_tiles(&mut self, tiles: impl Iterator<Item = TileData>) {
        self.backend.draw_tiles(tiles);
    }

    #[inline(always)]
    pub fn render(&mut self) {
        self.backend.show();
        self.last_render_time = Instant::now();
    }

    #[inline(always)]
    pub fn must_render(&self) -> bool {
        self.last_render_time.elapsed() >= self.min_render_interval
    }

    pub fn set_scale(&mut self, scale: u32, mode: ScaleMode) -> Result<(), String> {
        self.backend.set_scale(scale, mode)
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, scale_mode: ScaleMode) {
        self.backend.set_fullscreen(fullscreen, scale_mode);
    }

    pub fn handle_resize(&mut self, mode: ScaleMode) {
        self.backend.handle_resize(mode);
    }

    /// Returns whether the UI took the event.
    #[cfg(feature = "frontend-modern")]
    pub fn ui_took_event(&mut self, event: &sdl2::event::Event) -> bool {
        self.backend.ui_took_event(event)
    }

    /// Draws the UI over the frame already drawn; [`Self::render`] presents it.
    #[cfg(feature = "frontend-modern")]
    pub fn draw_ui(&mut self, run_ui: DrawUi) {
        self.backend.draw_ui(run_ui);
    }

    #[cfg(feature = "frontend-modern")]
    pub fn ui_frame_delay(&self) -> Duration {
        self.backend.ui_frame_delay()
    }
}

#[cfg(feature = "frontend-modern")]
impl Drop for AppVideo {
    fn drop(&mut self) {
        self.backend.destroy_ui();
    }
}
