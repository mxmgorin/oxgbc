use crate::audio::AppAudio;
use crate::config::{AppConfig, VideoBackendType};
use crate::frontend::{ActiveFrontend, Frontend};
use crate::input::handler::InputHandler;
use crate::library::RomsState;
use crate::notification::Notifications;
use crate::save;
use crate::save::battery::BatterySave;
use crate::storage::zip::{is_zip, unzip_rom};
use crate::video::palette::LcdPalette;
use crate::video::AppVideo;
use crate::{AppPlatform, PlatformFileDialog, PlatformFileSystem};
use arrayvec::ArrayString;
use core::cart::Cart;
use core::emu::runtime::EmuRuntime;
use core::emu::runtime::RunMode;
use core::emu::Emu;
use core::emu::EmuAudioCallback;
use sdl2::Sdl;
use std::path::Path;
use std::time::{Duration, Instant};

pub mod render;
pub mod roms;
pub mod screen;
pub mod states;

pub const AUTO_SAVE_STATE_SUFFIX: &str = "auto";

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum AppState {
    Paused,
    Running,
    Quitting,
    Stepping,
}

pub struct App<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    fps_str: ArrayString<10>,
    audio: AppAudio,
    pub palettes: Box<[LcdPalette]>,
    pub video: AppVideo,
    pub state: AppState,
    pub config: AppConfig,
    pub frontend: ActiveFrontend,
    pub notifications: Notifications,
    pub platform: AppPlatform<FS, FD>,
    pub roms: RomsState,
    /// Play time not yet banked into `roms`, which happens a second at a time.
    session_play: Duration,
}

impl<FS, FD> EmuAudioCallback for App<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    fn update(&mut self, output: &[f32], runtime: &EmuRuntime) {
        if self.config.audio.mute {
            return;
        }

        if self.config.audio.mute_turbo && runtime.mode == RunMode::Turbo {
            return;
        }

        if self.config.audio.mute_slow && runtime.mode == RunMode::Slow {
            return;
        }

        self.audio.queue(output);
    }
}

impl<FS, FD> App<FS, FD>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    pub fn new(
        sdl: &mut Sdl,
        mut config: AppConfig,
        palettes: Box<[LcdPalette]>,
        platform: AppPlatform<FS, FD>,
    ) -> Result<Self, String> {
        let colors = config.video.interface.palette_colors(&palettes);
        let mut notifications = Notifications::new(Duration::from_secs(3));

        let video = AppVideo::new(sdl, colors[0], colors[3], &config.video);
        let video = match video {
            Ok(video) => video,
            Err(err) => {
                log::error!("Failed to init AppVideo: {err}");
                if config.video.render.backend == VideoBackendType::Gl {
                    let msg = "GL init failed, fallback to SDL2";
                    log::info!("{msg}");
                    notifications.add(msg);
                    config.video.render.backend = VideoBackendType::Sdl2;

                    AppVideo::new(sdl, colors[0], colors[3], &config.video)?
                } else {
                    return Err(err);
                }
            }
        };
        let roms = RomsState::load_or_create(&platform.fs);

        Ok(Self {
            audio: AppAudio::new(sdl, &config.audio),
            frontend: ActiveFrontend::new(&roms),
            state: AppState::Paused,
            fps_str: ArrayString::<10>::new(),
            video,
            palettes,
            config,
            notifications,
            platform,
            roms,
            session_play: Duration::ZERO,
        })
    }

    /// Execution loop. The starting state is decided at cart-load time (see
    /// `crate::load_cart`): an explicitly passed ROM always runs, the
    /// remembered one only with `auto_continue`.
    pub fn run(&mut self, emu: &mut Emu, input: &mut InputHandler) {
        if emu.runtime.cpu.clock.bus.cart.is_empty() {
            self.state = AppState::Paused;
        }

        self.frontend
            .start(!emu.runtime.cpu.clock.bus.cart.is_empty());

        let mut tick = Instant::now();

        loop {
            // The interval that just ended counts as play if the game was running
            // through it; a menu or a paused emulator does not.
            let now = Instant::now();

            if self.state == AppState::Running {
                self.add_playtime(now - tick);
            }

            tick = now;
            input.handle_events(self, emu);
            input.handle_repeat(self, emu);

            match self.state {
                AppState::Quitting => break,
                AppState::Paused => {
                    self.render_menu(emu);

                    // Pointer input reaches the app only here: the UI collects it
                    // while drawing and hands it over once the frame is done.
                    while let Some(cmd) = self.frontend.take_cmd() {
                        input.handle_cmd(self, emu, cmd);
                    }
                }
                AppState::Running => self.render_frame(emu),
                AppState::Stepping => continue,
            }
        }
    }

    pub fn restart_rom(&mut self, emu: &mut Emu) {
        if let Some(cart_path) = self.roms.last_path() {
            if let Err(err) = self.load_cart_file(emu, &cart_path.to_path_buf()) {
                log::warn!("Failed to load cart file: {err}");
            }
        }
    }

    /// Banked a whole second at a time: a frame's worth rounds to nothing, and the
    /// name only has to be looked up when there is something to add.
    fn add_playtime(&mut self, elapsed: Duration) {
        self.session_play += elapsed;
        let secs = self.session_play.as_secs();

        if secs == 0 {
            return;
        }

        self.session_play -= Duration::from_secs(secs);

        if let Some(game) = self.game_name() {
            self.roms.add_playtime(&game, secs);
        }
    }

    /// File name of the loaded ROM, which is what every save beside it goes by.
    pub(super) fn game_name(&self) -> Option<String> {
        self.roms
            .last_path()
            .and_then(|path| self.platform.fs.file_name(path))
    }

    pub fn save_files(&mut self, emu: &mut Emu) -> Result<(), String> {
        self.roms.save_file();
        // save config
        self.config.set_emu_config(emu.config.clone());

        if let Err(err) = self.config.save_file().map_err(|e| e.to_string()) {
            log::warn!("Failed config.save: {err}");
        }

        let roms = RomsState::load_or_create(&self.platform.fs);
        let path = roms.last_path();

        let Some(path) = path else {
            return Ok(());
        };

        let name = self.platform.fs.file_name(path);

        let Some(name) = name else {
            return Err("Failed filesystem.file_name: not found".to_string());
        };

        // save sram for battery emulation
        if let Some(bytes) = emu.runtime.cpu.clock.bus.cart.dump_ram() {
            let battery = BatterySave::from_bytes(bytes)
                .save_file(&name)
                .map_err(|e| e.to_string());

            if let Err(err) = battery {
                log::warn!("Failed BatterySave: {err}");
            };
        }

        if self.config.auto_save_state {
            let state = emu.create_save_state();
            if let Err(err) = save::write_state(&state, &name, AUTO_SAVE_STATE_SUFFIX) {
                log::warn!("Failed save_state: {err}");
            }
        }

        Ok(())
    }

    pub fn load_cart_file(&mut self, emu: &mut Emu, path: &Path) -> Result<(), String> {
        let is_reload = self.roms.last_path().map(|x| x.as_path()) == Some(path)
            && !emu.runtime.cpu.clock.bus.cart.is_empty();

        let file_name = self
            .platform
            .fs
            .file_name(path)
            .ok_or("filesystem.file_name: None")?;
        let ram_bytes = BatterySave::load_file(&file_name).ok().map(|x| x.ram_bytes);
        let mut file_bytes = self
            .platform
            .fs
            .read_file_bytes(path)
            .ok_or("filesystem.read_file_bytes: None")?;

        if is_zip(path) {
            file_bytes = unzip_rom(&file_bytes)?.into_boxed_slice();
        }

        let mut cart = Cart::new(file_bytes).map_err(|e| e.to_string())?;
        _ = core::print_cart(&cart).map_err(|e| log::error!("Failed print_cart: {e}"));

        if let Some(ram_bytes) = ram_bytes {
            cart.load_ram(ram_bytes);
        }

        emu.load_cart(cart);
        self.roms.insert_or_update(path.to_path_buf());

        let colors = self.config.video.interface.palette_colors(&self.palettes);
        self.apply_dmg_palette(emu, colors);
        emu.runtime
            .cpu
            .clock
            .bus
            .io
            .ppu
            .toggle_fps(self.config.video.interface.show_fps);

        emu.runtime.cpu.clock.bus.io.apu.config = self.config.audio.apu_config();
        self.state = AppState::Running;
        self.frontend = ActiveFrontend::new(&self.roms);

        if !is_reload && self.config.auto_save_state {
            let path = self.roms.last_path().unwrap();
            let name = self.platform.fs.file_name(path).unwrap();
            let save_state = save::read_state(&name, AUTO_SAVE_STATE_SUFFIX);

            if let Ok(save_state) = save_state {
                emu.load_save_state(save_state);
            } else {
                log::warn!("Failed load save_state: {}", save_state.unwrap_err());
            };
        }

        Ok(())
    }

    pub fn change_volume(&mut self, emu: &mut Emu, delta: f32) {
        emu.runtime.cpu.clock.bus.io.apu.config.change_volume(delta);
        self.config.audio.volume = emu.runtime.cpu.clock.bus.io.apu.config.volume;

        let msg = format!("Volume: {}", self.config.audio.volume * 100.0);
        self.notifications.add(msg);
    }
}
