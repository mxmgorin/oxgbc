//! The console-style frontend: a text menu baked into the 160×144 framebuffer
//! with the app's own bitmap font. Thin delegation to [`AppMenu`] — behavior is
//! the menu's, unchanged.

pub mod menu;

use crate::cmd::AppCmd;
use crate::frontend::{Capture, Frontend, FrontendCtx, NavAction};
use crate::input::bindings::BindableInput;
use crate::library::RomsState;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::ppu::framebuffer::FrameBuffer;
use menu::AppMenu;
use std::time::Duration;

/// The menu redraws only on change, so idling between frames costs nothing.
const FRAME_DELAY: Duration = Duration::from_millis(30);

pub struct RetroFrontend {
    menu: AppMenu,
}

impl Frontend for RetroFrontend {
    fn new(roms: &RomsState) -> Self {
        Self {
            menu: AppMenu::new(roms),
        }
    }

    /// The text menu walks files on its own screen, reached from its own rows.
    fn open_browse(
        &mut self,
        _target: crate::frontend::BrowseTarget,
        _from: Option<&std::path::Path>,
    ) {
    }

    fn nav<FS: PlatformFileSystem>(
        &mut self,
        action: NavAction,
        ctx: FrontendCtx<'_, FS>,
    ) -> Option<AppCmd> {
        match action {
            NavAction::Up => self.menu.move_up(),
            NavAction::Down => self.menu.move_down(),
            NavAction::Left => return self.menu.move_left(ctx.config),
            NavAction::Right => return self.menu.move_right(ctx.config),
            NavAction::Confirm => return self.menu.select(ctx.config, ctx.fs, ctx.roms),
            NavAction::Back => self.menu.back(),
            // Every item of a text menu shows its own options in place; there is
            // nothing behind one to open.
            NavAction::Options => {}
        }

        None
    }

    /// The text menu binds from a screen of its own, so an input it does not want
    /// simply passes through.
    fn capture_bind<I: BindableInput>(&mut self, input: I, pressed: bool) -> Capture {
        // Presses only: its wait screen is opened by Confirm, and that key letting go
        // arrives while the screen is up — it would bind itself.
        if !pressed {
            return Capture::Pass;
        }

        match self.menu.handle_input(input, pressed) {
            Some(cmd) => Capture::Took(Some(cmd)),
            None => Capture::Pass,
        }
    }

    #[inline(always)]
    fn request_update(&mut self) {
        self.menu.request_update();
    }

    /// The text menu always opens at its root, game or not.
    fn open(&mut self, _has_game: bool) {}

    /// Everything here is reached through `nav`, which returns its command.
    fn take_cmd(&mut self) -> Option<AppCmd> {
        None
    }

    #[inline(always)]
    fn render<FS: PlatformFileSystem>(
        &mut self,
        video: &mut AppVideo,
        fb: &mut FrameBuffer,
        ctx: FrontendCtx<'_, FS>,
    ) {
        let (items, updated) = self.menu.get_items(ctx.config, ctx.roms);

        if updated {
            video.ui.fill_menu(fb, items, true, true);
        }

        video.draw_menu(fb);
    }

    #[inline(always)]
    fn frame_delay(&self) -> Duration {
        FRAME_DELAY
    }
}
