//! The console-style frontend: a text menu baked into the 160×144 framebuffer
//! with the app's own bitmap font. Thin delegation to [`AppMenu`] — behavior is
//! the menu's, unchanged.

pub mod menu;

use crate::cmd::AppCmd;
use crate::frontend::{Frontend, FrontendCtx, NavAction};
use crate::input::bindings::BindableInput;
use crate::roms::RomsState;
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
        }

        None
    }

    fn capture_bind<I: BindableInput>(&mut self, input: I, pressed: bool) -> Option<AppCmd> {
        self.menu.handle_input(input, pressed)
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
