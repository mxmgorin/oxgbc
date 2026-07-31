use crate::app::{App, AppState};
use crate::config::AppConfig;
use crate::input::handler::InputHandler;
use core::apu::Apu;
use core::auxiliary::io::Io;
use core::bus::Bus;
use core::cart::Cart;
use core::emu::runtime::EmuRuntime;
use core::emu::state::EmuSaveState;
use core::emu::Emu;
use core::ppu::lcd::Lcd;
use core::ppu::Ppu;
use palette::LcdPalette;
use std::fs::File;
use std::io::Cursor;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::{env, fs};
use zip::ZipArchive;

// Both enabled is not an error: a workspace build unifies desktop's modern with
// android's retro, and modern wins (see `frontend::ActiveFrontend`).
#[cfg(not(any(feature = "frontend-modern", feature = "frontend-retro")))]
compile_error!("select a frontend: `frontend-modern` or `frontend-retro`");

pub mod app;
pub mod audio;
pub mod battery;
pub mod cmd;
pub mod config;
pub mod file_browser;
pub mod frontend;
pub mod input;
pub mod notification;
pub mod palette;
pub mod roms;
pub mod state_meta;
pub mod state_shot;
pub mod video;

pub fn is_zip(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    extension == "zip"
}

pub fn unzip_rom(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let reader = Cursor::new(bytes);
    let mut archive = ZipArchive::new(reader).map_err(|_| "Invalid zip archive".to_string())?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).unwrap();
        let name = file.name().to_ascii_lowercase();

        if name.ends_with(".gb") || name.ends_with(".gbc") {
            let mut buffer = Vec::new();
            file.read_to_end(&mut buffer).unwrap();
            return Ok(buffer);
        }
    }

    Err("No valid .gb or .gbc file found in zip".to_string())
}

pub fn run<FS, FD>(args: Vec<String>, platform: AppPlatform<FS, FD>)
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    let base_dir = get_base_dir();
    log::info!("Using base_dir: {base_dir:?}");

    let config = get_config();
    let palettes = get_palettes();
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

pub fn get_config() -> AppConfig {
    let config_path = AppConfig::default_path();

    let config = if config_path.exists() {
        let config = AppConfig::from_file(&config_path);

        let Ok(config) = config else {
            log::error!("Failed to parse config file: {}", config.unwrap_err());

            let backup_path = config_path.with_file_name(format!(
                "{}.bak",
                config_path.file_name().unwrap().to_string_lossy()
            ));
            if let Err(rename_err) = fs::rename(config_path, &backup_path) {
                log::error!("Failed to rename invalid config file: {rename_err}");
            } else {
                log::error!("Renamed invalid config to {backup_path:?}");
            }

            let default_config = AppConfig::default();

            if let Err(save_err) = default_config.save_file() {
                panic!("Failed to save default config: {save_err}");
            }

            return default_config;
        };

        config
    } else {
        let default_config = AppConfig::default();

        if let Err(err) = default_config.save_file() {
            panic!("Failed to save default config: {err}");
        }

        default_config
    };

    config
}

pub fn get_palettes() -> Box<[LcdPalette]> {
    let path = LcdPalette::default_palettes_path();

    if path.exists() {
        core::read_json_file(&path).unwrap()
    } else {
        let palettes = LcdPalette::default_palettes().into_boxed_slice();
        LcdPalette::save_palettes_file(&palettes).unwrap();

        palettes
    }
}

pub fn get_base_dir() -> PathBuf {
    let path = sdl2::filesystem::pref_path("mxmgorin", "oxGBC").unwrap();

    PathBuf::from(path)
}

pub struct AppConfigFile;

impl AppConfigFile {
    pub fn write_save_state_file(
        state: &EmuSaveState,
        name: &str,
        suffix: &str,
    ) -> Result<(), String> {
        let path = AppConfigFile::get_save_state_path(name, suffix);

        if let Some(parent) = Path::new(&path).parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        let mut file = File::create(path).map_err(|e| e.to_string())?;
        postcard::to_io(state, &mut file).map_err(|e| e.to_string())?;

        Ok(())
    }

    pub fn read_save_state_file(name: &str, suffix: &str) -> Result<EmuSaveState, String> {
        let path = Self::get_save_state_path(name, suffix);
        let mut file = File::open(path).map_err(|e| e.to_string())?;
        let mut buffer = Vec::new();
        file.read_to_end(&mut buffer).map_err(|e| e.to_string())?;
        let decoded: EmuSaveState = postcard::from_bytes(&buffer).map_err(|e| e.to_string())?;

        Ok(decoded)
    }

    pub fn delete_save_state_file(name: &str, suffix: &str) -> Result<(), String> {
        let path = Self::get_save_state_path(name, suffix);

        fs::remove_file(path).map_err(|e| e.to_string())
    }

    pub fn get_save_state_path(game_name: &str, suffix: &str) -> PathBuf {
        get_base_dir()
            .join("save_states")
            .join(format!("{game_name}_{suffix}.state"))
    }
}

pub trait PlatformFileDialog {
    fn select_file(&mut self, title: &str, filter: (&[&str], &str)) -> Option<String>;
    fn select_dir(&mut self, title: &str) -> Option<String>;
}

pub struct AppPlatform<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    pub fs: FS,
    pub fd: FD,
}

impl<FS, FD> AppPlatform<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    pub fn new(fs: FS, fd: FD) -> Self {
        Self { fs, fd }
    }
}

pub trait PlatformFileSystem {
    fn get_file_name(&self, path: &Path) -> Option<String>;
    fn read_file_bytes(&self, path: &Path) -> Option<Box<[u8]>>;
    fn read_dir(&self, path: &Path) -> Result<Vec<String>, String>;
    fn can_split_paths(&self) -> bool;
}

pub struct EmptyFileDialog;

impl PlatformFileDialog for EmptyFileDialog {
    fn select_file(&mut self, _title: &str, _filter: (&[&str], &str)) -> Option<String> {
        None
    }

    fn select_dir(&mut self, _title: &str) -> Option<String> {
        None
    }
}
