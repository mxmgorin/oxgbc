use crate::get_base_dir;
use crate::input::config::InputConfig;
use crate::palette::LcdPalette;
use crate::video::frame_blend::FrameBlendMode;
use crate::video::shader::{ShaderFrameBlendMode, ShaderPrecision};
use core::apu::apu::ApuConfig;
use core::emu::config::{EmuConfig, GbModel};
use core::ppu::tile::PixelColor;
use core::ppu::LCD_X_RES;
use core::ppu::LCD_Y_RES;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use std::{fmt, fs, io};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AppConfig {
    pub emulation: EmuConfig, // only for deserialization

    pub auto_save_state: bool,
    pub current_save_slot: usize,
    pub current_load_slot: usize,
    pub auto_continue: bool,
    pub audio: AudioConfig,
    pub video: VideoConfig,
    pub input: InputConfig,
    #[serde(default)]
    pub library_sort: LibrarySort,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq, PartialEq, Default)]
pub enum LibrarySort {
    #[default]
    Recent,
    Name,
    Playtime,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, Eq, PartialEq)]
pub enum VideoBackendType {
    Sdl2,
    Gl,
}

impl fmt::Display for VideoBackendType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VideoBackendType::Sdl2 => write!(f, "SDL2"),
            VideoBackendType::Gl => write!(f, "GL"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Sdl2Config {
    pub grid_enabled: bool,
    pub subpixel_enabled: bool,
    pub dot_matrix_enabled: bool,
    pub scanline_enabled: bool,
    pub vignette_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GlConfig {
    pub shader_name: String,
    pub shader_frame_blend_mode: ShaderFrameBlendMode,
    pub shader_precision: ShaderPrecision,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VideoConfig {
    pub interface: InterfaceConfig,
    pub render: RenderConfig,
}

impl PartialEq for VideoConfig {
    fn eq(&self, _other: &Self) -> bool {
        false
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RenderConfig {
    pub frame_blend_mode: FrameBlendMode,
    pub blend_dim: f32,
    pub backend: VideoBackendType,
    pub sdl2: Sdl2Config,
    pub gl: GlConfig,
    pub frame_skip: usize,
    pub target_fps: f32,
}

impl RenderConfig {
    pub fn calc_min_frame_interval(&self) -> Duration {
        let frame_skip = self.frame_skip as f32;
        let frame_skip = frame_skip.clamp(0.0, 59.0);

        Duration::from_secs_f32(1.0 / (self.target_fps - frame_skip))
    }

    pub fn change_dim(&mut self, v: f32) {
        self.blend_dim = core::change_f32_rounded(self.blend_dim, v).clamp(0.0, 1.0)
    }

    pub const WIDTH: usize = LCD_X_RES as usize;
    pub const HEIGHT: usize = LCD_Y_RES as usize;
}

pub fn update_frame_skip(v: usize, delta: isize) -> usize {
    let v = v as isize + delta;

    v.clamp(0, 59) as usize
}

impl AppConfig {
    pub fn get_emu_config(&self) -> &EmuConfig {
        &self.emulation
    }

    pub fn set_emu_config(&mut self, config: EmuConfig) {
        self.emulation = config;
    }

    /// Highest save-state slot; a slot is a file named after its index, so this
    /// is only a bound on how many a game may keep.
    pub const MAX_SAVE_SLOT: usize = 99;

    pub fn inc_save_slot(&mut self) {
        self.current_save_slot =
            core::move_next_wrapped(self.current_save_slot, Self::MAX_SAVE_SLOT);
    }

    pub fn dec_save_slot(&mut self) {
        self.current_save_slot =
            core::move_prev_wrapped(self.current_save_slot, Self::MAX_SAVE_SLOT);
    }

    pub fn inc_load_slot(&mut self) {
        self.current_load_slot =
            core::move_next_wrapped(self.current_load_slot, Self::MAX_SAVE_SLOT);
    }

    pub fn dec_load_slot(&mut self) {
        self.current_load_slot =
            core::move_prev_wrapped(self.current_load_slot, Self::MAX_SAVE_SLOT);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AudioConfig {
    pub mute: bool,
    pub mute_turbo: bool,
    pub mute_slow: bool,
    pub buffer_size: usize,
    pub volume: f32,
    /// Ring-buffer fill the audio output converges to; the effective output
    /// latency. Too low risks underruns when a frame runs long.
    #[serde(default = "default_latency_ms")]
    pub latency_ms: u32,
    /// Per-channel mute mask, bit N audible = channel N+1.
    #[serde(default = "core::apu::apu::default_channel_mask")]
    pub channel_mask: u8,
}

fn default_latency_ms() -> u32 {
    50
}

impl AudioConfig {
    pub fn get_apu_config(&self) -> ApuConfig {
        let mut config = ApuConfig::new(self.buffer_size, self.volume);
        config.channel_mask = self.channel_mask;

        config
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub enum ScaleMode {
    Integer,
    Fit,
    Stretch,
}

impl fmt::Display for ScaleMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScaleMode::Integer => write!(f, "Integer"),
            ScaleMode::Fit => write!(f, "Fit"),
            ScaleMode::Stretch => write!(f, "Stretch"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InterfaceConfig {
    pub selected_palette_idx: usize,
    pub scale: f32,
    pub scale_mode: ScaleMode,
    pub is_fullscreen: bool,
    pub show_fps: bool,
    pub show_tiles: bool,
    pub is_palette_inverted: bool,
}

impl InterfaceConfig {
    pub fn get_palette_colors(&self, palettes: &[LcdPalette]) -> [PixelColor; 4] {
        let idx = self.selected_palette_idx;

        let mut colors = core::into_pixel_colors(&palettes[idx].hex_colors);

        if self.is_palette_inverted {
            colors.reverse();
        }

        colors
    }
}

impl AppConfig {
    pub fn from_file(path: &Path) -> io::Result<Self> {
        let data = fs::read_to_string(path)?;
        let config: Self = serde_json::from_str(&data)?;

        Ok(config)
    }

    pub fn save_file(&self) -> Result<(), io::Error> {
        let save_path = AppConfig::default_path();

        // Open file in write mode, truncating (overwriting) any existing content
        let mut file = File::create(save_path)?;
        let json = serde_json::to_string_pretty(self)?;
        file.write_all(json.as_bytes())
    }

    pub fn default_path() -> PathBuf {
        get_base_dir().join("config.json")
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        let apu_config = ApuConfig::default();

        Self {
            auto_save_state: false,
            current_save_slot: 0,
            current_load_slot: 0,
            // Default to CGB so DMG games are colorized out of the box, like a
            // real Game Boy Color. DMG-only carts still render on the DMG-compat
            // path (see App::apply_dmg_palette); users can pick DMG for mono.
            emulation: EmuConfig {
                model: Some(GbModel::Cgb),
                ..Default::default()
            },
            audio: AudioConfig {
                mute: false,
                mute_turbo: true,
                mute_slow: true,
                buffer_size: apu_config.buffer_size,
                volume: apu_config.volume,
                latency_ms: default_latency_ms(),
                channel_mask: apu_config.channel_mask,
            },
            input: InputConfig::default(),
            library_sort: LibrarySort::default(),
            video: VideoConfig {
                interface: InterfaceConfig {
                    selected_palette_idx: 0,
                    scale: 5.0,
                    scale_mode: ScaleMode::Fit,
                    is_fullscreen: true,
                    show_fps: false,
                    show_tiles: false,
                    is_palette_inverted: false,
                },
                render: RenderConfig {
                    frame_skip: 30,
                    frame_blend_mode: FrameBlendMode::None,
                    blend_dim: 1.0,
                    backend: VideoBackendType::Gl,
                    sdl2: Sdl2Config {
                        grid_enabled: false,
                        subpixel_enabled: false,
                        dot_matrix_enabled: false,
                        scanline_enabled: false,
                        vignette_enabled: false,
                    },
                    gl: GlConfig {
                        shader_name: "passthrough".to_string(),
                        shader_frame_blend_mode: ShaderFrameBlendMode::Simple,
                        shader_precision: ShaderPrecision::Auto,
                    },
                    target_fps: 60.0,
                },
            },
            auto_continue: false,
        }
    }
}
