use crate::config::{RenderConfig, ScaleMode, VideoConfig};
use crate::video::sdl2_filters::Sdl2Filters;
use crate::video::sdl2_tiles::Sdl2TilesView;
use crate::video::{calc_win_height, calc_win_width, new_scaled_rect};
use core::ppu::tile::TileData;
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Canvas, Texture, TextureCreator};
use sdl2::video::{Window, WindowContext};
use sdl2::{Sdl, VideoSubsystem};

pub struct Sdl2Backend {
    video_subsystem: VideoSubsystem,
    tiles_view: Option<Sdl2TilesView>,
    texture_creator: TextureCreator<WindowContext>,
    game_texture: Texture,
    game_rect: Rect,
    filters: Sdl2Filters,
    #[cfg(feature = "frontend-modern")]
    egui: egui_sdl2::EguiCanvas,
    pub canvas: Canvas<Window>,
}

impl Sdl2Backend {
    pub fn new(sdl: &Sdl, config: &VideoConfig, game_rect: Rect) -> Self {
        let video_subsystem = sdl.video().unwrap();
        let window = video_subsystem
            .window("oxGBC SDL2", game_rect.width(), game_rect.height())
            .position_centered()
            .resizable()
            .build()
            .unwrap();
        let mut canvas = window.into_canvas().build().unwrap();
        let texture_creator = canvas.texture_creator();
        let mut game_texture = texture_creator
            .create_texture_streaming(
                PixelFormatEnum::RGB565,
                RenderConfig::WIDTH as u32,
                RenderConfig::HEIGHT as u32,
            )
            .unwrap();
        game_texture.set_blend_mode(sdl2::render::BlendMode::Blend);

        Self {
            filters: Sdl2Filters::new(&mut canvas, &texture_creator, game_rect),
            tiles_view: if config.interface.show_tiles {
                Some(Sdl2TilesView::new(&video_subsystem))
            } else {
                None
            },
            #[cfg(feature = "frontend-modern")]
            egui: egui_sdl2::EguiCanvas::new(&canvas),
            video_subsystem,
            texture_creator,
            canvas,
            game_texture,
            game_rect,
        }
    }

    pub fn update_config(&mut self, config: &VideoConfig) {
        if config.interface.show_tiles {
            self.tiles_view = Some(Sdl2TilesView::new(&self.video_subsystem));
        } else {
            self.tiles_view = None;
        }
    }

    /// Closes the window and returns true when main window is closed.
    pub fn close_window(&mut self, id: u32) -> bool {
        if let Some(tiles) = self.tiles_view.as_mut() {
            if tiles.get_window_id() == id {
                self.tiles_view = None;
                return false;
            }
        }

        true
    }

    pub fn draw_buffer(&mut self, buffer: &[u8], config: &VideoConfig) {
        self.clear();
        let pitch = RenderConfig::WIDTH * core::ppu::PPU_BYTES_PER_PIXEL;
        self.game_texture.update(None, buffer, pitch).unwrap();
        self.canvas
            .copy(&self.game_texture, None, Some(self.game_rect))
            .unwrap();
        self.filters.apply(&mut self.canvas, &config.render.sdl2);
    }

    pub fn draw_menu(&mut self, buffer: &[u8], config: &VideoConfig) {
        self.clear();

        self.game_texture
            .update(None, buffer, core::ppu::PPU_PITCH)
            .unwrap();
        self.canvas
            .copy(&self.game_texture, None, Some(self.game_rect))
            .unwrap();
        self.filters.apply(&mut self.canvas, &config.render.sdl2);
    }

    pub fn show(&mut self) {
        self.canvas.present();
    }

    /// Returns whether egui consumed the event.
    #[cfg(feature = "frontend-modern")]
    pub fn egui_on_event(&mut self, event: &sdl2::event::Event) -> bool {
        self.egui.on_event(&self.canvas, event).consumed
    }

    /// Runs and paints egui over the frame already drawn; [`Self::show`] presents.
    #[cfg(feature = "frontend-modern")]
    pub fn render_egui(&mut self, run_ui: &mut dyn FnMut(&mut egui_sdl2::egui::Ui)) {
        self.egui.run_ui(run_ui);
        self.egui.paint(&mut self.canvas);
    }

    #[cfg(feature = "frontend-modern")]
    pub fn egui_repaint_delay(&self) -> std::time::Duration {
        self.egui.repaint_delay()
    }

    #[cfg(feature = "frontend-modern")]
    pub fn destroy_egui(&mut self) {
        self.egui.destroy();
    }

    pub fn set_scale(&mut self, scale: u32, mode: ScaleMode) -> Result<(), String> {
        let window = self.canvas.window_mut();
        window
            .set_size(calc_win_width(scale), calc_win_height(scale))
            .map_err(|e| e.to_string())?;
        window.set_position(
            sdl2::video::WindowPos::Centered,
            sdl2::video::WindowPos::Centered,
        );
        self.update_game_rect(mode);

        Ok(())
    }

    /// Recomputes the game rect for the window's current size (e.g. after an
    /// orientation change), without changing the window size.
    pub fn handle_resize(&mut self, mode: ScaleMode) {
        self.update_game_rect(mode);
    }

    pub fn set_fullscreen(&mut self, fullscreen: bool, scale_mode: ScaleMode) {
        if fullscreen {
            self.canvas
                .window_mut()
                .set_fullscreen(sdl2::video::FullscreenType::Desktop)
                .unwrap();
        } else {
            self.canvas
                .window_mut()
                .set_fullscreen(sdl2::video::FullscreenType::Off)
                .unwrap();
        }
        self.update_game_rect(scale_mode);
    }

    pub fn draw_tiles(&mut self, tiles: impl Iterator<Item = TileData>) {
        if let Some(tiles_view) = self.tiles_view.as_mut() {
            tiles_view.draw_tiles(tiles);
        }
    }

    fn clear(&mut self) {
        self.canvas.set_draw_color(Color::RGB(0, 0, 0)); // black
        self.canvas.clear();
    }

    fn update_game_rect(&mut self, scale_mode: ScaleMode) {
        let (win_width, win_height) = self.canvas.window().size();
        self.game_rect = new_scaled_rect(scale_mode, win_width, win_height);
        self.filters = Sdl2Filters::new(&mut self.canvas, &self.texture_creator, self.game_rect);
    }
}
