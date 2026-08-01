//! The emulator's egui screens, free of any windowing or rendering backend: the
//! desktop app paints them through egui-sdl2, the web build will paint the same
//! code through a web egui backend. Nothing here may depend on `sdl2`.

pub mod cart;
pub mod cover;
pub mod image;
pub mod library;
pub mod menu;
pub mod nav;
mod overlay;
pub mod rename;
pub mod settings;
pub mod states;

pub use cart::CartKind;
pub use image::RgbImage;
pub use library::{LibraryView, RomEntry};
pub use menu::{Menu, UiCmd, Views};
pub use nav::NavAction;
pub use settings::{Control, Row, Section, SettingId, SettingsView};
pub use states::{StateSlot, StatesView};
