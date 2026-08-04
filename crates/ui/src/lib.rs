//! The emulator's egui screens, free of any windowing or rendering backend: the
//! desktop app paints them through egui-sdl2, the web build will paint the same
//! code through a web egui backend. Nothing here may depend on `sdl2`.

pub mod browse;
pub mod cart;
pub mod cover;
pub mod image;
pub mod library;
pub mod menu;
pub mod nav;
mod osk;
mod overlay;
pub mod rename;
pub mod settings;
mod splash;
pub mod states;
pub mod theme;

/// What the app is called. The release it is stays on the About page: a version on
/// the way in is small print nobody reads twice.
pub(crate) const BRAND: &str = "oxGBC";
/// What it is, set under the name on the way in. The README's own line, which is where
/// the project says this once — the name alone tells a first-time user nothing.
pub(crate) const BRAND_LINE: &str = "Game Boy & Game Boy Color emulator";
/// The handle the project is published under; public, so About and the splash agree.
pub const AUTHOR: &str = "mxmgorin";
/// The words a cartridge title screen signed with.
pub(crate) const SIGNED_BY: &str = "programmed by";

pub use browse::{BrowseEntry, BrowseTarget, BrowseView};
pub use cart::CartKind;
pub use image::RgbImage;
pub use library::{LibraryView, RomEntry, SortBy};
pub use menu::{Menu, UiCmd, Views};
pub use nav::NavAction;
pub use settings::{Control, Page, PageId, Row, Section, SettingId, SettingsView, ROOT_PAGE};
pub use states::{StateSlot, StatesView};
