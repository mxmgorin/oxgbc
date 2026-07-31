use crate::audio::AppAudio;
use crate::battery::BatterySave;
use crate::config::{AppConfig, VideoBackendType};

use crate::frontend::{ActiveFrontend, Frontend};
use crate::input::handler::InputHandler;
use crate::notification::Notifications;
use crate::palette::LcdPalette;
use crate::roms::RomsState;
use crate::video::shader::{next_shader_by_name, prev_shader_by_name};
use crate::video::AppVideo;
use crate::{AppConfigFile, AppPlatform, PlatformFileDialog, PlatformFileSystem};
use arrayvec::ArrayString;
use core::cart::Cart;
use core::emu::config::GbModel;
use core::emu::runtime::EmuRuntime;
use core::emu::runtime::RunMode;
use core::emu::state::SaveStateCmd;
use core::emu::Emu;
use core::emu::EmuAudioCallback;
use core::ppu::framebuffer::FrameBuffer;
use core::ppu::tile::PixelColor;
use sdl2::Sdl;
use std::fmt::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

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
    palettes: Box<[LcdPalette]>,
    pub video: AppVideo,
    pub state: AppState,
    pub config: AppConfig,
    pub frontend: ActiveFrontend,
    pub notifications: Notifications,
    pub platform: AppPlatform<FS, FD>,
    pub roms: RomsState,
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
        let colors = config.video.interface.get_palette_colors(&palettes);
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
        let roms = RomsState::get_or_create(&platform.fs);

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
        })
    }

    /// Execution loop. The starting state is decided at cart-load time (see
    /// `crate::load_cart`): an explicitly passed ROM always runs, the
    /// remembered one only with `auto_continue`.
    pub fn run(&mut self, emu: &mut Emu, input: &mut InputHandler) {
        if emu.runtime.cpu.clock.bus.cart.is_empty() {
            self.state = AppState::Paused;
        }

        self.frontend.open(!emu.runtime.cpu.clock.bus.cart.is_empty());

        loop {
            input.handle_events(self, emu);

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

    #[inline(always)]
    pub fn render_frame(&mut self, emu: &mut Emu) {
        let on_time = emu.run_frame(self);

        // The audio callback only gets a shared runtime borrow, so the rate
        // computed by the queue's rate control is applied here instead.
        emu.runtime
            .cpu
            .clock
            .bus
            .io
            .apu
            .set_sample_rate(self.audio.sample_rate());

        if on_time || self.video.must_render() {
            self.render_framebuffer(emu);
        }
    }

    #[inline(always)]
    pub fn render_scanline(&mut self, emu: &mut Emu) {
        emu.runtime.run_scanline(self);
        self.render_framebuffer(emu);
    }

    #[inline(always)]
    pub fn render_framebuffer(&mut self, emu: &mut Emu) {
        let fps = emu.get_fps();
        let fb = emu.get_framebuffer();
        self.update_notif(fb);

        if let Some(new_fps) = fps {
            self.fps_str.clear();
            write!(&mut self.fps_str, "{new_fps:.2}").unwrap();
            self.video.ui.fill_fps(fb, &self.fps_str);
        }

        self.video.draw_buffer(fb);

        if self.config.video.interface.show_tiles {
            let tiles = emu.runtime.cpu.clock.bus.io.ppu.video_ram.iter_tiles();
            self.video.draw_tiles(tiles);
        }

        self.video.render();
    }

    #[inline(always)]
    pub fn render_menu(&mut self, emu: &mut Emu) {
        emu.runtime.cpu.clock.reset();
        let fb = emu.get_framebuffer();
        self.frontend
            .render(&mut self.video, fb, &self.config, &self.roms);
        self.update_notif(fb);
        self.video.render();

        thread::sleep(self.frontend.frame_delay());
    }

    #[inline(always)]
    pub fn update_notif(&mut self, fb: &mut FrameBuffer) {
        let (lines, updated) = self.notifications.update_and_get();
        self.video.ui.fill_notif(fb, lines);

        if updated {
            self.frontend.request_update();
        }
    }

    pub fn restart_rom(&mut self, emu: &mut Emu) {
        if let Some(cart_path) = self.roms.get_last_path() {
            if let Err(err) = self.load_cart_file(emu, &cart_path.to_path_buf()) {
                log::warn!("Failed to load cart file: {err}");
            }
        }
    }

    pub fn change_scale(&mut self, delta: f32) -> Result<(), String> {
        self.config.video.interface.scale = (self.config.video.interface.scale + delta).max(0.0);
        self.video.set_scale(
            self.config.video.interface.scale as u32,
            self.config.video.interface.scale_mode,
        )?;
        let msg = format!("Scale: {}", self.config.video.interface.scale);
        self.notifications.add(msg);

        Ok(())
    }

    pub fn next_palette(&mut self, emu: &mut Emu) {
        self.config.video.interface.selected_palette_idx = core::move_next_wrapped(
            self.config.video.interface.selected_palette_idx,
            self.palettes.len() - 1,
        );
        self.update_palette(emu);
    }

    pub fn prev_palette(&mut self, emu: &mut Emu) {
        self.config.video.interface.selected_palette_idx = core::move_prev_wrapped(
            self.config.video.interface.selected_palette_idx,
            self.palettes.len() - 1,
        );
        self.update_palette(emu);
    }

    /// Apply the DMG palette to the emulator: the authentic GBC boot-ROM
    /// colorization when the CGB model is selected for a monochrome cart,
    /// otherwise the selected preset `colors`.
    fn apply_dmg_palette(&self, emu: &mut Emu, colors: [PixelColor; 4]) {
        let compat = (emu.config.model == Some(GbModel::Cgb))
            .then(|| emu.dmg_compat_palette())
            .flatten();

        let lcd = &mut emu.runtime.cpu.clock.bus.io.ppu.lcd;
        match compat {
            Some(compat) => {
                // Colorized DMG game: force the DMG-compat render path (the CGB
                // path has no DMG-compat handling) so BGP/OBP still permute the
                // assigned colors.
                lcd.set_model(GbModel::Dmg);
                compat.apply(&mut lcd.dmg_palette);
            }
            None => lcd.dmg_palette.set_colors(colors),
        }
    }

    /// Re-apply the DMG palette using the current config (preset colors or the
    /// GBC colorization). Call after the model or preset changes.
    pub(crate) fn refresh_dmg_palette(&self, emu: &mut Emu) {
        let colors = self
            .config
            .video
            .interface
            .get_palette_colors(&self.palettes);
        self.apply_dmg_palette(emu, colors);
    }

    pub fn update_palette(&mut self, emu: &mut Emu) {
        let palette = &self.palettes[self.config.video.interface.selected_palette_idx];
        let colors = self
            .config
            .video
            .interface
            .get_palette_colors(&self.palettes);
        self.video.ui.text_color = colors[0];
        self.video.ui.bg_color = colors[3];
        self.apply_dmg_palette(emu, colors);
        self.frontend.request_update();

        let suffix = if self.config.video.interface.is_palette_inverted {
            " (inv)"
        } else {
            ""
        };
        let msg = format!("Palette: {}{}", palette.name, suffix);
        self.notifications.add(msg);
    }

    pub fn next_shader(&mut self) {
        let (name, _shader) = next_shader_by_name(&self.config.video.render.gl.shader_name);
        self.update_shader(name);
    }

    pub fn prev_shader(&mut self) {
        let (name, _shader) = prev_shader_by_name(&self.config.video.render.gl.shader_name);
        self.update_shader(name);
    }

    pub fn update_shader(&mut self, name: impl Into<String>) {
        self.config.video.render.gl.shader_name = name.into();
        self.video.update_config(&self.config.video);
        self.frontend.request_update();
        self.notifications.add(format!(
            "Shader: {}",
            self.config.video.render.gl.shader_name
        ));
    }

    pub fn toggle_fullscreen(&mut self) {
        self.config.video.interface.is_fullscreen = !self.config.video.interface.is_fullscreen;
        self.video.set_fullscreen(
            self.config.video.interface.is_fullscreen,
            self.config.video.interface.scale_mode,
        );
    }

    pub fn handle_save_state(&mut self, emu: &mut Emu, event: SaveStateCmd, index: Option<usize>) {
        let path = self.roms.get_last_path().unwrap();
        let name = self.platform.fs.get_file_name(path).unwrap();

        match event {
            SaveStateCmd::Create => {
                let save_state = emu.create_save_state();
                let index = index.unwrap_or(self.config.current_save_slot).to_string();

                if let Err(err) = AppConfigFile::write_save_state_file(&save_state, &name, &index) {
                    log::error!("Failed save_state: {err}");
                    return;
                }

                let msg = format!("Saved save state: {index}");
                self.notifications.add(msg);
            }
            SaveStateCmd::Load => {
                let index = index.unwrap_or(self.config.current_load_slot).to_string();
                let save_state = AppConfigFile::read_save_state_file(&name, &index);

                let Ok(save_state) = save_state else {
                    log::error!("Failed load save_state: {}", save_state.unwrap_err());
                    return;
                };

                emu.load_save_state(save_state);
                let colors = self
                    .config
                    .video
                    .interface
                    .get_palette_colors(&self.palettes);
                self.apply_dmg_palette(emu, colors);
                emu.runtime.cpu.clock.bus.io.apu.config = self.config.audio.get_apu_config();

                let msg = format!("Loaded save state: {index}");
                self.notifications.add(msg);
                self.state = AppState::Running;
            }
        }
    }

    pub fn save_files(&mut self, emu: &mut Emu) -> Result<(), String> {
        self.roms.save_file();
        // save config
        self.config.set_emu_config(emu.config.clone());

        if let Err(err) = self.config.save_file().map_err(|e| e.to_string()) {
            log::warn!("Failed config.save: {err}");
        }

        let roms = RomsState::get_or_create(&self.platform.fs);
        let path = roms.get_last_path();

        let Some(path) = path else {
            return Ok(());
        };

        let name = self.platform.fs.get_file_name(path);

        let Some(name) = name else {
            return Err("Failed filesystem.get_file_name: not found".to_string());
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
            if let Err(err) =
                AppConfigFile::write_save_state_file(&state, &name, AUTO_SAVE_STATE_SUFFIX)
            {
                log::warn!("Failed save_state: {err}");
            }
        }

        Ok(())
    }

    pub fn load_cart_file(&mut self, emu: &mut Emu, path: &Path) -> Result<(), String> {
        let is_reload = self.roms.get_last_path().map(|x| x.as_path()) == Some(path)
            && !emu.runtime.cpu.clock.bus.cart.is_empty();

        let file_name = self
            .platform
            .fs
            .get_file_name(path)
            .ok_or("filesystem.get_file_name: None")?;
        let ram_bytes = BatterySave::load_file(&file_name).ok().map(|x| x.ram_bytes);
        let mut file_bytes = self
            .platform
            .fs
            .read_file_bytes(path)
            .ok_or("filesystem.read_file_bytes: None")?;

        println!("{:?}", path);

        if crate::is_zip(path) {
            file_bytes = crate::unzip_rom(&file_bytes)?.into_boxed_slice();
        }

        let mut cart = Cart::new(file_bytes).map_err(|e| e.to_string())?;
        _ = core::print_cart(&cart).map_err(|e| log::error!("Failed print_cart: {e}"));

        if let Some(ram_bytes) = ram_bytes {
            cart.load_ram(ram_bytes);
        }

        emu.load_cart(cart);
        self.roms.insert_or_update(path.to_path_buf());

        let colors = self
            .config
            .video
            .interface
            .get_palette_colors(&self.palettes);
        self.apply_dmg_palette(emu, colors);
        emu.runtime
            .cpu
            .clock
            .bus
            .io
            .ppu
            .toggle_fps(self.config.video.interface.show_fps);

        emu.runtime.cpu.clock.bus.io.apu.config = self.config.audio.get_apu_config();
        self.state = AppState::Running;
        self.frontend = ActiveFrontend::new(&self.roms);

        if !is_reload && self.config.auto_save_state {
            let path = self.roms.get_last_path().unwrap();
            let name = self.platform.fs.get_file_name(path).unwrap();
            let save_state = AppConfigFile::read_save_state_file(&name, AUTO_SAVE_STATE_SUFFIX);

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
