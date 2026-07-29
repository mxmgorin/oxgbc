//! The UI seam: everything the app calls to draw and drive its menus. One
//! frontend is selected at compile time ([`ActiveFrontend`]) — today only the
//! retro text menu, an egui one later — so the app never sees UI internals.
//!
//! Stays free of any UI toolkit: only `AppCmd`, config and platform types.

#[cfg(feature = "frontend-retro")]
pub mod retro;

use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::input::bindings::BindableInput;
use crate::roms::RomsState;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::ppu::framebuffer::FrameBuffer;
use std::time::Duration;

#[cfg(feature = "frontend-retro")]
pub type ActiveFrontend = retro::RetroFrontend;

/// Directional intent from any bound input, already resolved from joypad
/// buttons — the frontend decides what moving means.
#[derive(Clone, Copy, Eq, PartialEq)]
pub enum NavAction {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
}

/// App state a frontend reads while it is itself mutably borrowed — passed as
/// disjoint field borrows because `App` owns the frontend.
pub struct FrontendCtx<'a, FS: PlatformFileSystem> {
    pub config: &'a AppConfig,
    pub fs: &'a FS,
    pub roms: &'a RomsState,
}

pub trait Frontend {
    fn new(roms: &RomsState) -> Self;

    /// Move/activate the UI. `None` when the action only changed selection.
    fn nav<FS: PlatformFileSystem>(
        &mut self,
        action: NavAction,
        ctx: FrontendCtx<'_, FS>,
    ) -> Option<AppCmd>;

    /// Offer a raw input to the rebinding flow before it reaches the emulator;
    /// `Some` only while the UI is waiting to capture a binding.
    fn capture_bind<I: BindableInput>(&mut self, input: I, pressed: bool) -> Option<AppCmd>;

    /// Mark the UI dirty — app state it displays changed underneath it.
    fn request_update(&mut self);

    fn render(
        &mut self,
        video: &mut AppVideo,
        fb: &mut FrameBuffer,
        config: &AppConfig,
        roms: &RomsState,
    );

    /// How long to idle after a frame while the UI is open.
    fn frame_delay(&self) -> Duration;
}
