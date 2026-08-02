//! The emulator application: what turns the `core` emulator into something a
//! person uses. [`run`] is the whole entry point — a platform hands over its file
//! access and gets a running app.

use crate::app::{App, AppState};
use crate::config::AppConfig;
use crate::input::handler::InputHandler;
use crate::storage::get_base_dir;
use crate::video::palette::LcdPalette;
use core::apu::Apu;
use core::auxiliary::io::Io;
use core::bus::Bus;
use core::cart::Cart;
use core::emu::runtime::EmuRuntime;
use core::emu::Emu;
use core::ppu::lcd::Lcd;
use core::ppu::Ppu;
use std::env;
use std::path::{Path, PathBuf};

// Both enabled is not an error: a workspace build unifies desktop's modern with
// android's retro, and modern wins (see `frontend::ActiveFrontend`).
#[cfg(not(any(feature = "frontend-modern", feature = "frontend-retro")))]
compile_error!("select a frontend: `frontend-modern` or `frontend-retro`");

pub mod app;
pub mod audio;
pub mod cmd;
pub mod config;
pub mod frontend;
pub mod input;
pub mod library;
pub mod notification;
pub mod platform;
pub mod save;
pub mod storage;
pub mod video;

// What a platform build implements to start the app, kept reachable from the crate
// root: `app::PlatformFileSystem` is what desktop and android write.
pub use platform::{AppPlatform, EmptyFileDialog, PlatformFileDialog, PlatformFileSystem};

pub fn run<FS, FD>(args: Vec<String>, platform: AppPlatform<FS, FD>)
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    let base_dir = get_base_dir();
    log::info!("Using base_dir: {base_dir:?}");

    let config = AppConfig::load_or_create();
    let palettes = LcdPalette::load_or_create();
    let mut emu = new_emu(&config, &palettes);
    let mut sdl = sdl2::init().unwrap();
    let mut input = InputHandler::new(&sdl).unwrap();
    let mut app = App::new(&mut sdl, config, palettes, platform).unwrap();
    load_cart(&mut app, &mut emu, args);

    app.run(&mut emu, &mut input);

    if let Err(err) = app.save_files(&mut emu) {
        log::error!("Failed app.save_files: {err}");
    }
}

pub fn new_emu(config: &AppConfig, palettes: &[LcdPalette]) -> Emu {
    let emu_config = config.get_emu_config();
    let apu_config = config.audio.get_apu_config();
    let colors = config.video.interface.get_palette_colors(palettes);

    let lcd = Lcd::new(colors, core::emu::config::GbModel::default());
    let mut ppu = Ppu::new(lcd);
    ppu.toggle_fps(config.video.interface.show_fps);
    let apu = Apu::new(apu_config);
    let bus = Bus::new(Cart::empty(), Io::new(ppu, apu), emu_config.model);

    #[cfg(feature = "debug")]
    {
        let debugger = core::debugger::Debugger::new(core::debugger::DebugLogType::Asm, false);
        return Emu::new(emu_config.clone(), EmuRuntime::new(bus, Some(debugger))).unwrap();
    }

    #[cfg(not(feature = "debug"))]
    Emu::new(emu_config.clone(), EmuRuntime::new(bus)).unwrap()
}

pub fn load_cart<FS, FD>(app: &mut App<FS, FD>, emu: &mut Emu, mut args: Vec<String>)
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    let cart_path = if args.len() < 2 {
        env::var("CART_PATH").ok()
    } else {
        Some(args.remove(1))
    }
    .map(PathBuf::from);

    if let Some(cart_path) = cart_path {
        // An explicitly requested ROM starts running regardless of
        // auto_continue.
        if let Err(err) = app.load_cart_file(emu, Path::new(&cart_path)) {
            log::warn!("Failed to load cart file: {err}");
        }
    } else {
        app.restart_rom(emu);

        if !app.config.auto_continue {
            app.state = AppState::Paused;
        }
    }
}
