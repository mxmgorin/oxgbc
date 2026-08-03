use super::{menu_toggle, SubMenu, MAX_MENU_ITEM_CHARS};
use crate::cmd::{AppCmd, BindCmds, BindTarget};
use crate::config::AppConfig;
use crate::library::RomsState;
use crate::video::truncate_text;
use core::auxiliary::joypad::JoypadButton;

pub enum AppMenuItem {
    Resume,
    SaveState,
    LoadState,
    OpenRom,
    SettingsMenu,
    InterfaceMenu,
    Back,
    Quit,
    Palette,
    ToggleFps,
    ToggleFullscreen,
    AudioMenu,
    Volume,
    Scale,
    ScaleMode,
    AdvancedMenu,
    TileWindow,
    SpinDuration,
    SystemMenu,
    AutoSaveState,
    NormalSpeed,
    TurboSpeed,
    SlowSpeed,
    RewindSize,
    RewindFrames,
    AudioBufferSize,
    MuteTurbo,
    MuteSlow,
    /// Audibility toggle for channel N (0-based).
    AudioChannel(u8),
    ResetConfig,
    RestartGame,

    InputMenu,
    ComboInterval,
    KeyboardInput,
    ButtonsBinding(Box<[JoypadButton]>),
    CmdsBinding(BindCmds),
    KeyboardShortcuts,
    WaitInput(BindTarget),

    PaletteInverted,
    CpuFrameBlendMode,
    FrameBlendAlpha,
    FrameBlendFade,
    FrameBlendDim,
    VideoMenu,
    FrameBlendProfile,
    FrameBlendRise,
    FrameBlendFall,
    FrameBlendBleed,
    GridFilter,
    SubpixelFilter,
    LoadedRoms,
    LoadedRomsSubMenu(Box<dyn SubMenu>),
    RomsDir,
    Confirm(AppCmd),
    ScanlineFilter,
    DotMatrixFilter,
    VignetteFilter,
    VideoBackend,
    VideoShader,
    ShaderFrameBlend,
    BrowseRoms,
    BrowseRomsSubMenu(Box<dyn SubMenu>),
    FrameSkip,
    OpenedRoms,
    OpenedRomsSubMenu(Box<dyn SubMenu>),
    GbModel,
    TargetFps,
}

impl AppMenuItem {
    pub fn items_mut(&mut self) -> Option<&mut Box<dyn SubMenu>> {
        match self {
            AppMenuItem::Resume
            | AppMenuItem::Confirm(_)
            | AppMenuItem::RomsDir
            | AppMenuItem::ShaderFrameBlend
            | AppMenuItem::VideoBackend
            | AppMenuItem::VideoShader
            | AppMenuItem::SaveState
            | AppMenuItem::LoadState
            | AppMenuItem::OpenRom
            | AppMenuItem::SettingsMenu
            | AppMenuItem::InterfaceMenu
            | AppMenuItem::Back
            | AppMenuItem::FrameSkip
            | AppMenuItem::Quit
            | AppMenuItem::Palette
            | AppMenuItem::ToggleFps
            | AppMenuItem::ToggleFullscreen
            | AppMenuItem::AudioMenu
            | AppMenuItem::VignetteFilter
            | AppMenuItem::Volume
            | AppMenuItem::Scale
            | AppMenuItem::AdvancedMenu
            | AppMenuItem::TileWindow
            | AppMenuItem::SpinDuration
            | AppMenuItem::SystemMenu
            | AppMenuItem::AutoSaveState
            | AppMenuItem::NormalSpeed
            | AppMenuItem::TurboSpeed
            | AppMenuItem::SlowSpeed
            | AppMenuItem::RewindSize
            | AppMenuItem::RewindFrames
            | AppMenuItem::AudioBufferSize
            | AppMenuItem::MuteTurbo
            | AppMenuItem::MuteSlow
            | AppMenuItem::AudioChannel(_)
            | AppMenuItem::ResetConfig
            | AppMenuItem::RestartGame
            | AppMenuItem::InputMenu
            | AppMenuItem::ComboInterval
            | AppMenuItem::PaletteInverted
            | AppMenuItem::CpuFrameBlendMode
            | AppMenuItem::FrameBlendAlpha
            | AppMenuItem::FrameBlendFade
            | AppMenuItem::FrameBlendDim
            | AppMenuItem::VideoMenu
            | AppMenuItem::FrameBlendProfile
            | AppMenuItem::FrameBlendRise
            | AppMenuItem::FrameBlendFall
            | AppMenuItem::FrameBlendBleed
            | AppMenuItem::GridFilter
            | AppMenuItem::SubpixelFilter
            | AppMenuItem::ScanlineFilter
            | AppMenuItem::DotMatrixFilter
            | AppMenuItem::BrowseRoms
            | AppMenuItem::OpenedRoms
            | AppMenuItem::KeyboardInput
            | AppMenuItem::LoadedRoms
            | AppMenuItem::WaitInput(_)
            | AppMenuItem::CmdsBinding(_)
            | AppMenuItem::KeyboardShortcuts
            | AppMenuItem::ScaleMode
            | AppMenuItem::GbModel
            | AppMenuItem::TargetFps
            | AppMenuItem::ButtonsBinding(_) => None,
            AppMenuItem::LoadedRomsSubMenu(x)
            | AppMenuItem::OpenedRomsSubMenu(x)
            | AppMenuItem::BrowseRomsSubMenu(x) => Some(x),
        }
    }

    pub fn items(&self) -> Option<&Box<dyn SubMenu>> {
        match self {
            AppMenuItem::Resume
            | AppMenuItem::Confirm(_)
            | AppMenuItem::SaveState
            | AppMenuItem::ShaderFrameBlend
            | AppMenuItem::RomsDir
            | AppMenuItem::VideoBackend
            | AppMenuItem::FrameSkip
            | AppMenuItem::VideoShader
            | AppMenuItem::LoadState
            | AppMenuItem::OpenRom
            | AppMenuItem::VignetteFilter
            | AppMenuItem::SettingsMenu
            | AppMenuItem::InterfaceMenu
            | AppMenuItem::Back
            | AppMenuItem::Quit
            | AppMenuItem::Palette
            | AppMenuItem::ToggleFps
            | AppMenuItem::ToggleFullscreen
            | AppMenuItem::AudioMenu
            | AppMenuItem::Volume
            | AppMenuItem::Scale
            | AppMenuItem::AdvancedMenu
            | AppMenuItem::TileWindow
            | AppMenuItem::SpinDuration
            | AppMenuItem::SystemMenu
            | AppMenuItem::AutoSaveState
            | AppMenuItem::NormalSpeed
            | AppMenuItem::TurboSpeed
            | AppMenuItem::SlowSpeed
            | AppMenuItem::RewindSize
            | AppMenuItem::RewindFrames
            | AppMenuItem::AudioBufferSize
            | AppMenuItem::MuteTurbo
            | AppMenuItem::MuteSlow
            | AppMenuItem::AudioChannel(_)
            | AppMenuItem::ResetConfig
            | AppMenuItem::RestartGame
            | AppMenuItem::InputMenu
            | AppMenuItem::ComboInterval
            | AppMenuItem::PaletteInverted
            | AppMenuItem::CpuFrameBlendMode
            | AppMenuItem::FrameBlendAlpha
            | AppMenuItem::FrameBlendFade
            | AppMenuItem::FrameBlendDim
            | AppMenuItem::VideoMenu
            | AppMenuItem::FrameBlendProfile
            | AppMenuItem::FrameBlendRise
            | AppMenuItem::FrameBlendFall
            | AppMenuItem::FrameBlendBleed
            | AppMenuItem::GridFilter
            | AppMenuItem::SubpixelFilter
            | AppMenuItem::ScanlineFilter
            | AppMenuItem::DotMatrixFilter
            | AppMenuItem::BrowseRoms
            | AppMenuItem::OpenedRoms
            | AppMenuItem::KeyboardInput
            | AppMenuItem::LoadedRoms
            | AppMenuItem::WaitInput(_)
            | AppMenuItem::CmdsBinding(_)
            | AppMenuItem::KeyboardShortcuts
            | AppMenuItem::ScaleMode
            | AppMenuItem::GbModel
            | AppMenuItem::TargetFps
            | AppMenuItem::ButtonsBinding(_) => None,
            AppMenuItem::LoadedRomsSubMenu(x)
            | AppMenuItem::OpenedRomsSubMenu(x)
            | AppMenuItem::BrowseRomsSubMenu(x) => Some(x),
        }
    }
}

fn with_value(label: &str, value: impl std::fmt::Display) -> String {
    format!("{label}: {value}")
}

fn with_toggle(label: &str, value: bool) -> String {
    format!("{label}: {}", menu_toggle(value))
}

fn with_count(label: &str, value: usize) -> String {
    format!("{label}({})", value)
}

impl AppMenuItem {
    pub fn to_string(&self, config: &AppConfig, roms: &RomsState) -> String {
        let item_str = match self {
            AppMenuItem::Resume => "Resume".to_string(),
            AppMenuItem::OpenRom => "Open ROM".to_string(),
            AppMenuItem::Quit => "Quit".to_string(),
            AppMenuItem::SaveState => with_count("Save", config.current_save_slot),
            AppMenuItem::LoadState => with_count("Load", config.current_load_slot),
            AppMenuItem::SettingsMenu => "Settings".to_string(),
            AppMenuItem::InterfaceMenu => "Interface".to_string(),
            AppMenuItem::Back => "Back".to_string(),
            AppMenuItem::Palette => {
                with_value("Palette", config.video.interface.selected_palette_idx)
            }
            AppMenuItem::ToggleFps => with_toggle("FPS", config.video.interface.show_fps),
            AppMenuItem::ToggleFullscreen => {
                with_toggle("Fullscreen", config.video.interface.is_fullscreen)
            }
            AppMenuItem::AudioMenu => "Audio".to_string(),
            AppMenuItem::Volume => with_value("Volume", (config.audio.volume * 100.0) as i32),
            AppMenuItem::Scale => with_value("Scale", config.video.interface.scale),
            AppMenuItem::AdvancedMenu => "Advanced".to_string(),
            AppMenuItem::TileWindow => with_toggle("Show Tiles", config.video.interface.show_tiles),
            AppMenuItem::SpinDuration => with_value(
                "Spin Wait(µs)",
                config.emu_config().spin_duration.as_micros(),
            ),
            AppMenuItem::SystemMenu => "System".to_string(),
            AppMenuItem::AutoSaveState => with_toggle("Auto Save State", config.auto_save_state),
            AppMenuItem::NormalSpeed => with_value("Normal Speed", config.emulation.normal_speed),
            AppMenuItem::TurboSpeed => with_value("Turbo Speed", config.emulation.turbo_speed),
            AppMenuItem::SlowSpeed => with_value("Slow Speed", config.emulation.slow_speed),
            AppMenuItem::RewindSize => with_value("Rewind Size", config.emulation.rewind_size),
            AppMenuItem::RewindFrames => {
                with_value("Rewind Frames", config.emulation.rewind_frames)
            }
            AppMenuItem::AudioBufferSize => with_value("Buffer Size", config.audio.buffer_size),
            AppMenuItem::MuteTurbo => with_toggle("Mute Turbo", config.audio.mute_turbo),
            AppMenuItem::MuteSlow => with_toggle("Mute Slow", config.audio.mute_slow),
            AppMenuItem::AudioChannel(i) => with_toggle(
                &format!("Channel {}", i + 1),
                config.audio.channel_mask & (1 << i) != 0,
            ),
            AppMenuItem::ResetConfig => "Reset Settings".to_string(),
            AppMenuItem::RestartGame => "Restart".to_string(),
            AppMenuItem::InputMenu => "Input".to_string(),
            AppMenuItem::ComboInterval => {
                with_value("Combo Dur(ms)", config.input.combo_interval.as_millis())
            }
            AppMenuItem::PaletteInverted => with_toggle(
                "Palette Inverted",
                config.video.interface.is_palette_inverted,
            ),
            AppMenuItem::CpuFrameBlendMode => with_value(
                "CPU Frame Blend",
                config.video.render.frame_blend_mode.name(),
            ),
            AppMenuItem::FrameBlendAlpha => {
                with_value("Blend Alpha", config.video.render.frame_blend_mode.alpha())
            }
            AppMenuItem::FrameBlendFade => {
                with_value("Blend Fade", config.video.render.frame_blend_mode.fade())
            }
            AppMenuItem::FrameBlendDim => with_value("Blend Dim", config.video.render.blend_dim),
            AppMenuItem::VideoMenu => "Video".to_string(),
            AppMenuItem::FrameBlendProfile => with_value(
                "Blend Profile",
                config
                    .video
                    .render
                    .frame_blend_mode
                    .profile()
                    .unwrap()
                    .name(),
            ),
            AppMenuItem::FrameBlendRise => with_value(
                "Blend Rise",
                config.video.render.frame_blend_mode.profile().unwrap().rise,
            ),
            AppMenuItem::FrameBlendFall => with_value(
                "Blend Fall",
                config.video.render.frame_blend_mode.profile().unwrap().fall,
            ),
            AppMenuItem::FrameBlendBleed => with_value(
                "Blend Bleed",
                config
                    .video
                    .render
                    .frame_blend_mode
                    .profile()
                    .unwrap()
                    .bleed,
            ),
            AppMenuItem::GridFilter => with_toggle("Grid", config.video.render.sdl2.grid_enabled),
            AppMenuItem::SubpixelFilter => {
                with_toggle("Mask", config.video.render.sdl2.subpixel_enabled)
            }
            AppMenuItem::LoadedRoms => format!("ROMs({})", roms.loaded_count()),
            AppMenuItem::LoadedRomsSubMenu(_) => "ROMs Sub".to_string(),
            AppMenuItem::RomsDir => "Select ROMs Dir".to_string(),
            AppMenuItem::Confirm(_) => "Confirm".to_string(),
            AppMenuItem::ScanlineFilter => {
                with_toggle("Scanline", config.video.render.sdl2.scanline_enabled)
            }
            AppMenuItem::DotMatrixFilter => {
                with_toggle("Dot-Matrix", config.video.render.sdl2.dot_matrix_enabled)
            }
            AppMenuItem::VignetteFilter => {
                with_toggle("Vignette", config.video.render.sdl2.vignette_enabled)
            }
            AppMenuItem::VideoBackend => with_value("Backend", config.video.render.backend),
            AppMenuItem::VideoShader => with_value("Shader", &config.video.render.gl.shader_name),
            AppMenuItem::ShaderFrameBlend => with_value(
                "GPU Frame Blend",
                config.video.render.gl.shader_frame_blend_mode,
            ),
            AppMenuItem::BrowseRoms => "Browse".to_string(),
            AppMenuItem::BrowseRomsSubMenu(_) => "Browse Sub".to_string(),
            AppMenuItem::FrameSkip => with_value("Frame Skip", config.video.render.frame_skip),
            AppMenuItem::OpenedRoms => with_count("Recent", roms.opened_count()),
            AppMenuItem::OpenedRomsSubMenu(_) => "Recent Sub".to_string(),
            AppMenuItem::KeyboardInput => "Keyboard".to_string(),
            AppMenuItem::ButtonsBinding(btns) => {
                let cmd = if btns.len() == 1 && !btns.is_empty() {
                    AppCmd::PressButton(btns[0])
                } else {
                    AppCmd::new_macro_buttons(btns.to_owned(), true)
                };

                let name = btns
                    .iter()
                    .map(|b| format!("{:?}", b))
                    .collect::<Vec<_>>()
                    .join("+");

                with_value(&name, config.input.bindings.keyboard.desc(&cmd))
            }
            AppMenuItem::CmdsBinding(cmd) => {
                format!(
                    "{}: {}",
                    cmd.pressed,
                    config.input.bindings.keyboard.desc(&cmd.pressed)
                )
            }
            AppMenuItem::WaitInput(_) => "Press a key".to_string(),
            AppMenuItem::KeyboardShortcuts => "Shortcuts".to_string(),
            AppMenuItem::ScaleMode => with_value("Scale Mode", config.video.interface.scale_mode),
            AppMenuItem::GbModel => {
                let model_name = match config.emulation.model {
                    Some(m) => format!("{:?}", m).to_uppercase(),
                    None => "Auto".to_string(),
                };

                with_value("Model", model_name)
            }
            AppMenuItem::TargetFps => {
                with_value("Target FPS", config.video.render.target_fps as usize)
            }
        };

        truncate_text(&item_str, MAX_MENU_ITEM_CHARS)
    }
}
