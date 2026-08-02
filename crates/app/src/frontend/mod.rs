//! The UI seam: everything the app calls to draw and drive its menus. One
//! frontend is selected at compile time ([`ActiveFrontend`]) — the retro text
//! menu or the modern egui UI — so the app never sees UI internals.
//!
//! Stays free of any UI toolkit: only `AppCmd`, config and platform types.

#[cfg(feature = "frontend-modern")]
pub mod modern;
#[cfg(feature = "frontend-retro")]
pub mod retro;

use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::input::bindings::BindableInput;
use crate::palette::LcdPalette;
use crate::roms::RomsState;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::ppu::framebuffer::FrameBuffer;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Modern wins when both features are on; every `frontend-modern` cfg elsewhere
/// follows the same rule, so the egui glue in `video/` matches this alias.
#[cfg(feature = "frontend-modern")]
pub type ActiveFrontend = modern::ModernFrontend;
#[cfg(all(feature = "frontend-retro", not(feature = "frontend-modern")))]
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
    /// Whatever else can be done with the focused item — the Select button, which
    /// no menu needs for anything else.
    Options,
}

/// What the rebinding flow did with a raw input.
///
/// An `Option<AppCmd>` cannot say this: cancelling a capture produces no command and
/// still has to stop the input reaching whatever it is bound to, or Escape would
/// close the menu on its way out of the capture.
pub enum Capture {
    /// Not capturing; the input goes on to its binding as usual.
    Pass,
    /// Taken by the rebinding flow — nothing else may act on this input.
    Took(Option<AppCmd>),
}

/// App state a frontend reads while it is itself mutably borrowed — passed as
/// disjoint field borrows because `App` owns the frontend.
pub struct FrontendCtx<'a, FS: PlatformFileSystem> {
    pub config: &'a AppConfig,
    pub fs: &'a FS,
    pub roms: &'a RomsState,
    pub palettes: &'a [LcdPalette],
}

/// What a storage walk is for. Defined here as well as in `ui` for the same reason
/// as [`NavAction`]: this seam must stay free of any UI toolkit. Not `Copy` — a
/// cover walk carries the cart it is for.
#[derive(Clone, Eq, PartialEq, Debug, Default)]
pub enum BrowseTarget {
    #[default]
    Rom,
    Dir,
    /// A cover picture for this ROM.
    Cover(PathBuf),
}

pub trait Frontend {
    fn new(roms: &RomsState) -> Self;

    /// Show the in-app storage walk, for platforms whose own picker cannot be
    /// reached the way the app is driven, starting `from` wherever the last walk
    /// stopped. A frontend with its own file screen — the text menu has one —
    /// ignores this.
    fn open_browse(&mut self, target: BrowseTarget, from: Option<&Path>);

    /// Move/activate the UI. `None` when the action only changed selection.
    fn nav<FS: PlatformFileSystem>(
        &mut self,
        action: NavAction,
        ctx: FrontendCtx<'_, FS>,
    ) -> Option<AppCmd>;

    /// Offer a raw input to the rebinding flow before it reaches the emulator.
    fn capture_bind<I: BindableInput>(&mut self, input: I, pressed: bool) -> Capture;

    /// Mark the UI dirty — app state it displays changed underneath it.
    fn request_update(&mut self);

    /// Called when the UI opens; `has_game` decides whether it is a pause menu
    /// over a running game or the app's home screen.
    fn open(&mut self, has_game: bool);

    /// Commands the last [`Self::render`] produced — pointer input has no other
    /// way out, since rendering can't return through the UI toolkit.
    fn take_cmd(&mut self) -> Option<AppCmd>;

    fn render<FS: PlatformFileSystem>(
        &mut self,
        video: &mut AppVideo,
        fb: &mut FrameBuffer,
        ctx: FrontendCtx<'_, FS>,
    );

    /// How long to idle after a frame while the UI is open.
    fn frame_delay(&self) -> Duration;
}
