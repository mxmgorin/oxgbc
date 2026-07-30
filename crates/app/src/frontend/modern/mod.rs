//! The egui frontend. Spike stage: a throwaway panel over the paused game that
//! exercises the two backends' egui integration (GL and canvas) and its event
//! routing. The real library/settings screens come from the `ui` crate later.

use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::frontend::{Frontend, FrontendCtx, NavAction};
use crate::input::bindings::BindableInput;
use crate::roms::RomsState;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::ppu::framebuffer::FrameBuffer;
use egui_sdl2::egui;
use std::time::Duration;

/// Cap on egui's own repaint delay, so input keeps being polled while it idles.
const MAX_FRAME_DELAY: Duration = Duration::from_millis(30);
const NAV_LOG_LEN: usize = 6;

#[derive(Default)]
pub struct ModernFrontend {
    frame_delay: Duration,
    nav_log: Vec<&'static str>,
    slider: f32,
}

impl Frontend for ModernFrontend {
    fn new(_roms: &RomsState) -> Self {
        Self::default()
    }

    fn nav<FS: PlatformFileSystem>(
        &mut self,
        action: NavAction,
        _ctx: FrontendCtx<'_, FS>,
    ) -> Option<AppCmd> {
        // No focus model yet; log the action so the bridge can be seen working.
        self.nav_log.push(match action {
            NavAction::Up => "Up",
            NavAction::Down => "Down",
            NavAction::Left => "Left",
            NavAction::Right => "Right",
            NavAction::Confirm => "Confirm",
            NavAction::Back => "Back",
        });

        if self.nav_log.len() > NAV_LOG_LEN {
            self.nav_log.remove(0);
        }

        None
    }

    fn capture_bind<I: BindableInput>(&mut self, _input: I, _pressed: bool) -> Option<AppCmd> {
        None
    }

    fn request_update(&mut self) {}

    fn render(
        &mut self,
        video: &mut AppVideo,
        fb: &mut FrameBuffer,
        config: &AppConfig,
        roms: &RomsState,
    ) {
        video.draw_menu(fb);
        let nav = self.nav_log.join(" ");
        let roms_len = roms.loaded_count();
        let scale = config.video.interface.scale;
        let slider = &mut self.slider;

        video.render_egui(&mut |ctx| {
            egui::Window::new("oxGBC")
                .default_pos([16.0, 16.0])
                .show(ctx, |ui| {
                    ui.label(format!("roms: {roms_len}   scale: {scale}"));
                    ui.add(egui::Slider::new(slider, 0.0..=1.0).text("pointer test"));
                    ui.label(format!("nav: {nav}"));
                });
        });

        self.frame_delay = video.egui_repaint_delay().min(MAX_FRAME_DELAY);
    }

    fn frame_delay(&self) -> Duration {
        self.frame_delay
    }
}
