use super::files::FilesMenu;
use super::item::AppMenuItem;
use super::roms::RomsMenu;
use super::SubMenu;
use crate::cmd::{AppCmd, BindCmds, BindTarget, ChangeConfigCmd};
use crate::config::{InterfaceConfig, VideoBackendType, VideoConfig};
use crate::roms::RomsState;
use crate::video::frame_blend::FrameBlendMode;
use crate::PlatformFileSystem;
use core::auxiliary::joypad::JoypadButton;
use core::emu::runtime::RunMode;
use core::emu::state::SaveStateCmd;
use std::path::Path;

pub fn video_menu(conf: &VideoConfig) -> Box<[AppMenuItem]> {
    let mut items = Vec::with_capacity(15);
    items.push(AppMenuItem::VideoBackend);

    if conf.render.backend == VideoBackendType::Sdl2 {
        items.push(AppMenuItem::GridFilter);
        items.push(AppMenuItem::SubpixelFilter);
        items.push(AppMenuItem::ScanlineFilter);
        items.push(AppMenuItem::DotMatrixFilter);
        items.push(AppMenuItem::VignetteFilter);
    } else if conf.render.backend == VideoBackendType::Gl {
        items.push(AppMenuItem::VideoShader);
        items.push(AppMenuItem::ShaderFrameBlend);
    }

    items.push(AppMenuItem::CpuFrameBlendMode);

    match conf.render.frame_blend_mode {
        FrameBlendMode::None => {}
        FrameBlendMode::Linear(_) => {
            items.push(AppMenuItem::FrameBlendDim);
            items.push(AppMenuItem::FrameBlendAlpha);
        }
        FrameBlendMode::Exp(_) => {
            items.push(AppMenuItem::FrameBlendDim);
            items.push(AppMenuItem::FrameBlendFade);
        }
        FrameBlendMode::Gamma(_) | FrameBlendMode::Additive(_) => {
            items.push(AppMenuItem::FrameBlendDim);
            items.push(AppMenuItem::FrameBlendFade);
            items.push(AppMenuItem::FrameBlendAlpha);
        }
        FrameBlendMode::Accurate(_) => {
            items.push(AppMenuItem::FrameBlendDim);
            items.push(AppMenuItem::FrameBlendProfile);
            items.push(AppMenuItem::FrameBlendFall);
            items.push(AppMenuItem::FrameBlendRise);
            items.push(AppMenuItem::FrameBlendBleed);
        }
    }

    items.push(AppMenuItem::Back);

    items.into_boxed_slice()
}

pub fn loaded_roms_menu(
    filesystem: &impl PlatformFileSystem,
    roms: &RomsState,
) -> Box<[AppMenuItem]> {
    let roms: Box<dyn SubMenu> = Box::new(RomsMenu::from_loaded(filesystem, roms));

    vec![AppMenuItem::LoadedRomsSubMenu(roms), AppMenuItem::Back].into_boxed_slice()
}

pub fn opened_roms_menu(
    filesystem: &impl PlatformFileSystem,
    roms: &RomsState,
) -> Box<[AppMenuItem]> {
    let roms: Box<dyn SubMenu> = Box::new(RomsMenu::from_opened(filesystem, roms));

    vec![AppMenuItem::OpenedRomsSubMenu(roms), AppMenuItem::Back].into_boxed_slice()
}

pub fn files_menu(
    _filesystem: &impl PlatformFileSystem,
    last_path: Option<impl AsRef<Path>>,
) -> Box<[AppMenuItem]> {
    let files: Box<dyn SubMenu> = Box::new(FilesMenu::new(last_path));

    vec![AppMenuItem::BrowseRomsSubMenu(files)].into_boxed_slice()
}

pub fn input_menu() -> Box<[AppMenuItem]> {
    vec![
        AppMenuItem::KeyboardInput,
        AppMenuItem::ComboInterval,
        AppMenuItem::Back,
    ]
    .into_boxed_slice()
}

pub fn confirm_menu(cmd: AppCmd) -> Box<[AppMenuItem]> {
    vec![AppMenuItem::Confirm(cmd), AppMenuItem::Back].into_boxed_slice()
}

pub fn system_menu() -> Box<[AppMenuItem]> {
    vec![
        AppMenuItem::GbModel,
        AppMenuItem::AutoSaveState,
        AppMenuItem::NormalSpeed,
        AppMenuItem::TurboSpeed,
        AppMenuItem::SlowSpeed,
        AppMenuItem::RewindSize,
        AppMenuItem::RewindFrames,
        #[cfg(feature = "file-dialog")]
        AppMenuItem::RomsDir,
        AppMenuItem::Back,
    ]
    .into_boxed_slice()
}

pub fn advanced_menu() -> Box<[AppMenuItem]> {
    vec![
        AppMenuItem::TargetFps,
        AppMenuItem::TileWindow,
        AppMenuItem::SpinDuration,
        AppMenuItem::FrameSkip,
        AppMenuItem::ResetConfig,
        AppMenuItem::Back,
    ]
    .into_boxed_slice()
}

pub fn start_menu(roms: &RomsState) -> Box<[AppMenuItem]> {
    let mut items = Vec::with_capacity(10);

    if roms.get_last_path().is_some() {
        items.push(AppMenuItem::Resume);
        items.push(AppMenuItem::SaveState);
        items.push(AppMenuItem::LoadState);
        items.push(AppMenuItem::RestartGame);
    }

    if roms.opened_count() != 0 {
        items.push(AppMenuItem::OpenedRoms);
    }

    #[cfg(not(feature = "file-dialog"))]
    if roms.loaded_count() != 0 {
        items.push(AppMenuItem::LoadedRoms);
    }

    #[cfg(feature = "file-dialog")]
    {
        items.push(AppMenuItem::OpenRom);
        items.push(AppMenuItem::LoadedRoms);
    }

    #[cfg(feature = "file-browser")]
    items.push(AppMenuItem::BrowseRoms);

    items.push(AppMenuItem::SettingsMenu);
    items.push(AppMenuItem::Quit);

    items.into_boxed_slice()
}

pub fn settings_menu() -> Box<[AppMenuItem]> {
    vec![
        AppMenuItem::InterfaceMenu,
        AppMenuItem::VideoMenu,
        AppMenuItem::AudioMenu,
        AppMenuItem::InputMenu,
        AppMenuItem::SystemMenu,
        AppMenuItem::AdvancedMenu,
        AppMenuItem::Back,
    ]
    .into_boxed_slice()
}

pub fn interface_menu(conf: &InterfaceConfig) -> Box<[AppMenuItem]> {
    let mut items = Vec::with_capacity(6);
    items.push(AppMenuItem::Palette);
    items.push(AppMenuItem::PaletteInverted);
    items.push(AppMenuItem::ToggleFullscreen);
    items.push(AppMenuItem::ToggleFps);

    if conf.is_fullscreen {
        items.push(AppMenuItem::ScaleMode);
    } else {
        items.push(AppMenuItem::Scale);
    }

    items.push(AppMenuItem::Back);

    items.into_boxed_slice()
}

pub fn audio_menu() -> Box<[AppMenuItem]> {
    vec![
        AppMenuItem::Volume,
        AppMenuItem::AudioBufferSize,
        AppMenuItem::MuteTurbo,
        AppMenuItem::MuteSlow,
        AppMenuItem::AudioChannel(0),
        AppMenuItem::AudioChannel(1),
        AppMenuItem::AudioChannel(2),
        AppMenuItem::AudioChannel(3),
        AppMenuItem::Back,
    ]
    .into_boxed_slice()
}

pub fn keyboard_menu() -> Box<[AppMenuItem]> {
    vec![
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Up].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Down].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Left].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Right].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::A].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::B].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Start].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Select].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Up, JoypadButton::Left].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(vec![JoypadButton::Up, JoypadButton::Right].into_boxed_slice()),
        AppMenuItem::ButtonsBinding(
            vec![JoypadButton::Down, JoypadButton::Left].into_boxed_slice(),
        ),
        AppMenuItem::ButtonsBinding(
            vec![JoypadButton::Down, JoypadButton::Right].into_boxed_slice(),
        ),
        AppMenuItem::KeyboardShortcuts,
        AppMenuItem::Back,
    ]
    .into_boxed_slice()
}
pub fn keyboard_shortcuts_menu() -> Box<[AppMenuItem]> {
    vec![
        AppMenuItem::CmdsBinding(BindCmds::new(AppCmd::ToggleMenu, None)),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::SaveState(SaveStateCmd::Create, None),
            None,
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::SaveState(SaveStateCmd::Load, None),
            None,
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::ChangeConfig(ChangeConfigCmd::IncSaveAndLoadSlots),
            None,
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::ToggleRewind,
            AppCmd::ChangeMode(RunMode::Normal).into(),
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::ChangeMode(RunMode::Turbo),
            AppCmd::ChangeMode(RunMode::Normal).into(),
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::ChangeMode(RunMode::Slow),
            AppCmd::ChangeMode(RunMode::Normal).into(),
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::ChangeConfig(ChangeConfigCmd::NextPalette),
            None,
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::ChangeConfig(ChangeConfigCmd::InvertPalette),
            None,
        )),
        AppMenuItem::CmdsBinding(BindCmds::new(
            AppCmd::ChangeConfig(ChangeConfigCmd::NextShader),
            None,
        )),
        AppMenuItem::Back,
    ]
    .into_boxed_slice()
}

pub fn wait_input_menu(target: BindTarget) -> Box<[AppMenuItem]> {
    vec![AppMenuItem::WaitInput(target)].into_boxed_slice()
}
