//! The emulator's egui screens, free of any windowing or rendering backend: the
//! desktop app paints them through egui-sdl2, the web build will paint the same
//! code through a web egui backend. Nothing here may depend on `sdl2`.

pub mod cart;
pub mod library;
pub mod nav;

pub use cart::CartKind;
pub use library::{library, LibraryView, RomEntry};
pub use nav::{FocusEvent, GridFocus, NavAction};
