use crate::config::{ScaleMode, VideoBackendType, VideoConfig};
use crate::video::frame_blend::FrameBlend;
use crate::video::gl_backend::GlBackend;
use crate::video::overlay::Overlay;
use crate::video::sdl2_backend::Sdl2Backend;
use crate::video::{calc_win_height, calc_win_width, new_scaled_rect, VideoBackend};
use core::ppu::tile::PixelColor;
use core::ppu::tile::TileData;
use sdl2::Sdl;
use std::time::{Duration, Instant};

pub struct AppVideo {
    frame_blend: Option<FrameBlend>,
    backend: VideoBackend,
    config: VideoConfig,
    last_render_time: Instant,
    pub min_render_interval: Duration,
    pub ui: Overlay,
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
                let backend = GlBackend::new(sdl, game_rect, &config.render)?;
                VideoBackend::Gl(backend)
            }
        };
        backend.set_fullscreen(config.interface.is_fullscreen, config.interface.scale_mode);
        let ui = Overlay::new(text_color, bg_color);

        Ok(Self {
            frame_blend: FrameBlend::new(&config.render.frame_blend_mode),
            config: config.clone(),
            last_render_time: Instant::now(),
            min_render_interval: config.render.calc_min_frame_interval(),
            backend,
            ui,
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
    }

    #[inline]
    pub fn draw_buffer(&mut self, buffer: &[u8]) {
        let buffer = if let Some(blend) = &mut self.frame_blend {
            blend.process_buffer(buffer, &self.config)
        } else {
            buffer
        };

        self.backend.draw_buffer(buffer, &self.config);
    }

    #[inline(always)]
    pub fn draw_menu(&mut self, buffer: &[u8]) {
        self.backend.draw_menu(buffer, &self.config)
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

    /// Returns whether egui consumed the event.
    #[cfg(feature = "frontend-modern")]
    pub fn egui_on_event(&mut self, event: &sdl2::event::Event) -> bool {
        self.backend.egui_on_event(event)
    }

    /// Draws egui over the frame already drawn; [`Self::render`] presents it.
    #[cfg(feature = "frontend-modern")]
    pub fn render_egui(&mut self, run_ui: &mut dyn FnMut(&mut egui_sdl2::egui::Ui)) {
        self.backend.render_egui(run_ui);
    }

    #[cfg(feature = "frontend-modern")]
    pub fn egui_repaint_delay(&self) -> Duration {
        self.backend.egui_repaint_delay()
    }
}

#[cfg(feature = "frontend-modern")]
impl Drop for AppVideo {
    fn drop(&mut self) {
        self.backend.destroy_egui();
    }
}
