//! Drawing one frame, of the game or of the menu over it.

use crate::app::App;
use crate::frontend::{Frontend, FrontendCtx, UiUpdate};
use crate::{PlatformFileDialog, PlatformFileSystem};
use core::emu::Emu;
use core::ppu::framebuffer::FrameBuffer;
use std::fmt::Write;
use std::thread;
use std::time::Instant;

impl<FS, FD> App<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    #[inline(always)]
    pub fn render_frame(&mut self, emu: &mut Emu) {
        let on_time = emu.run_frame(self);

        // The audio callback only gets a shared runtime borrow, so the rate
        // computed by the queue's rate control is applied here instead.
        emu.runtime
            .cpu
            .clock
            .bus
            .io
            .apu
            .set_sample_rate(self.audio.sample_rate());

        if on_time || self.video.must_render() {
            self.render_framebuffer(emu);
        }
    }

    #[inline(always)]
    pub fn render_scanline(&mut self, emu: &mut Emu) {
        emu.runtime.run_scanline(self);
        self.render_framebuffer(emu);
    }

    #[inline(always)]
    pub fn render_framebuffer(&mut self, emu: &mut Emu) {
        let fps = emu.get_fps();
        let fb = emu.get_framebuffer();
        self.update_notif(fb);

        if let Some(new_fps) = fps {
            self.fps_str.clear();
            write!(&mut self.fps_str, "{new_fps:.2}").unwrap();
            self.video.fill_fps(fb, &self.fps_str);
        }

        self.video.draw_buffer(fb);

        if self.config.video.interface.show_tiles {
            let tiles = emu.runtime.cpu.clock.bus.io.ppu.video_ram.iter_tiles();
            self.video.draw_tiles(tiles);
        }

        self.video.render();
    }

    /// One turn of the menu loop. The frame is built and presented only when it would
    /// differ from the one on screen — an idle menu leaves the last frame standing and
    /// costs the sleep alone — while the period is slept out either way, so input keeps
    /// being polled at the same rate.
    #[inline(always)]
    pub fn render_menu(&mut self, emu: &mut Emu) {
        let started = Instant::now();
        emu.runtime.cpu.clock.reset();
        let fb = emu.get_framebuffer();
        // Ages the lines out on a clock of its own, so it runs whether or not a frame
        // is drawn: a notification going away is itself a reason to draw one.
        self.update_notif(fb);

        if self.frontend.needs_render() {
            self.frontend.render(
                &mut self.video,
                fb,
                FrontendCtx {
                    config: &self.config,
                    fs: &self.platform.fs,
                    roms: &self.roms,
                    palettes: &self.palettes,
                },
            );
            self.video.render();
        }

        // What the frame cost comes off its period, so a slow frame stretches the
        // period instead of the wait being added on top of it.
        thread::sleep(
            self.frontend
                .frame_period()
                .saturating_sub(started.elapsed()),
        );
    }

    #[inline(always)]
    pub fn update_notif(&mut self, fb: &mut FrameBuffer) {
        let (lines, updated) = self.notifications.update_and_get();
        self.video.fill_notif(fb, lines);

        if updated {
            self.frontend.request_update(UiUpdate::Overlay);
        }
    }
}
