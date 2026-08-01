use crate::{
    config::VideoConfig,
    input::bindings::{BindableInput, InputIndex, InputKind},
};
use core::{
    auxiliary::joypad::JoypadButton,
    emu::{config::GbModel, runtime::RunMode, state::SaveStateCmd},
};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum AppCmd {
    ToggleMenu,
    ToggleRewind,
    LoadFile(PathBuf),
    RestartRom,
    ChangeMode(RunMode),
    SaveState(SaveStateCmd, Option<usize>),
    /// Throws away the state file of the loaded game's slot.
    DeleteState(usize),
    /// Names the state in the loaded game's slot; an empty name clears it.
    RenameState(usize, String),
    /// Names a ROM of the library; an empty name goes back to its file name.
    RenameRom(PathBuf, String),
    /// Asks for a cover picture for this ROM and stores it beside its metadata.
    SetRomCover(PathBuf),
    /// Takes a ROM's cover away.
    RemoveRomCover(PathBuf),
    /// Makes the screen of one of this ROM's states its cover.
    SetCoverFromState(PathBuf, usize),
    SelectRom,
    Quit,
    ChangeConfig(ChangeConfigCmd),
    SelectRomsDir,
    ReleaseButton(JoypadButton),
    PressButton(JoypadButton),
    SetFileBrowsePath(PathBuf),
    ToggleFullscreen,
    Macro(Box<[AppCmd]>),
    BindInput(BindInputCmd),
    ToggleDebug,
    ToggleStepping,
    StepFrame,
    StepScanline,
    ClearScreen,
}

impl AppCmd {
    pub const fn name(&self) -> &'static str {
        match self {
            AppCmd::ToggleMenu => "Toggle Menu",
            AppCmd::ToggleRewind => "Rewind",
            AppCmd::LoadFile(_) => "Load File",
            AppCmd::RestartRom => "Restart ROM",
            AppCmd::ChangeMode(m) => m.name(),
            AppCmd::SaveState(m, _) => m.name(),
            AppCmd::DeleteState(_) => "Delete State",
            AppCmd::RenameState(_, _) => "Rename State",
            AppCmd::RenameRom(_, _) => "Rename ROM",
            AppCmd::SetRomCover(_) => "Set Cover",
            AppCmd::RemoveRomCover(_) => "Remove Cover",
            AppCmd::SetCoverFromState(_, _) => "Cover From State",
            AppCmd::SelectRom => "Select ROM",
            AppCmd::Quit => "Quit",
            AppCmd::ChangeConfig(conf) => conf.name(),
            AppCmd::SelectRomsDir => "Select ROMs Dir",
            AppCmd::ReleaseButton(_) => "Release Button",
            AppCmd::PressButton(_) => "Press Button",
            AppCmd::SetFileBrowsePath(_) => "Set File Browse Path",
            AppCmd::ToggleFullscreen => "Fullscreen",
            AppCmd::Macro(_) => "Macro",
            AppCmd::BindInput(_) => "Bind Input",
            AppCmd::ToggleDebug => "Toggle Debug",
            AppCmd::StepFrame => "Step Frame",
            AppCmd::ToggleStepping => "Toggle Stepping",
            AppCmd::StepScanline => "Step Scanline",
            AppCmd::ClearScreen => "Clear Screen",
        }
    }
}

impl fmt::Display for AppCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl AppCmd {
    pub fn new_macro_buttons<B: Into<Box<[JoypadButton]>>>(buttons: B, pressed: bool) -> AppCmd {
        let buttons: Box<[JoypadButton]> = buttons.into();

        if pressed {
            AppCmd::Macro(buttons.iter().map(|b| AppCmd::PressButton(*b)).collect())
        } else {
            AppCmd::Macro(buttons.iter().map(|b| AppCmd::ReleaseButton(*b)).collect())
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BindInputCmd {
    pub input_index: InputIndex,
    pub input_kind: InputKind,
    pub target: BindTarget,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum BindTarget {
    Buttons(Box<[JoypadButton]>),
    Cmds(BindCmds),
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BindCmds {
    pub pressed: Box<AppCmd>,
    pub released: Option<Box<AppCmd>>,
}

impl BindCmds {
    pub fn new(pressed: AppCmd, released: Option<AppCmd>) -> Self {
        Self {
            pressed: Box::new(pressed),
            released: released.map(|c| Box::new(c)),
        }
    }
}

impl BindInputCmd {
    pub fn new<I: BindableInput>(input: I, pressed: bool, target: BindTarget) -> Self {
        Self {
            input_index: InputIndex::new(input, pressed),
            input_kind: input.kind(),
            target,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub enum ChangeConfigCmd {
    Reset,
    Volume(f32),
    Scale(f32),
    TileWindow,
    Fullscreen,
    Fps,
    SpinDuration(i32),
    NextPalette,
    PrevPalette,
    ToggleMute,
    NormalSpeed(f32),
    TurboSpeed(f32),
    SlowSpeed(f32),
    RewindSize(i32),
    RewindFrames(i32),
    AutoSaveState,
    AudioBufferSize(i32),
    MuteTurbo,
    MuteSlow,
    /// Flips the audibility of channel N (0-based) in the mix.
    ToggleChannel(u8),
    ComboInterval(i32),
    SetSaveSlot(usize),
    SetLoadSlot(usize),
    IncSaveAndLoadSlots,
    DecSaveAndLoadSlots,
    InvertPalette,
    Video(Box<VideoConfig>),
    NextShader,
    PrevShader,
    FrameSkip(usize),
    SetGbModel(Option<GbModel>),
    TargetFps(f32),
}

impl ChangeConfigCmd {
    pub const fn name(&self) -> &'static str {
        match self {
            ChangeConfigCmd::Reset => "Reset",
            ChangeConfigCmd::Volume(_) => "Volume",
            ChangeConfigCmd::Scale(_) => "Scale",
            ChangeConfigCmd::TileWindow => "Tile Window",
            ChangeConfigCmd::Fullscreen => "Fullscreen",
            ChangeConfigCmd::Fps => "Fps",
            ChangeConfigCmd::SpinDuration(_) => "Spin Duration",
            ChangeConfigCmd::NextPalette => "Next Palette",
            ChangeConfigCmd::PrevPalette => "Prev Palette",
            ChangeConfigCmd::ToggleMute => "Mute",
            ChangeConfigCmd::NormalSpeed(_) => "Normal Speed",
            ChangeConfigCmd::TurboSpeed(_) => "Turbo Speed",
            ChangeConfigCmd::SlowSpeed(_) => "Slow Speed",
            ChangeConfigCmd::RewindSize(_) => "Rewind Size",
            ChangeConfigCmd::RewindFrames(_) => "Rewind Frames",
            ChangeConfigCmd::AutoSaveState => "Auto Save State",
            ChangeConfigCmd::AudioBufferSize(_) => "Audio Buffer Size",
            ChangeConfigCmd::MuteTurbo => "Mute Turbo",
            ChangeConfigCmd::MuteSlow => "Mute Slow",
            ChangeConfigCmd::ToggleChannel(_) => "Toggle Channel",
            ChangeConfigCmd::ComboInterval(_) => "Combo Interval",
            ChangeConfigCmd::SetSaveSlot(_) => "Set Save Slot",
            ChangeConfigCmd::SetLoadSlot(_) => "Set Load Slot",
            ChangeConfigCmd::IncSaveAndLoadSlots => "Next Save Slot",
            ChangeConfigCmd::DecSaveAndLoadSlots => "Prev Save Slot",
            ChangeConfigCmd::InvertPalette => "Invert Palette",
            ChangeConfigCmd::Video(_) => "Video",
            ChangeConfigCmd::NextShader => "Next Shader",
            ChangeConfigCmd::PrevShader => "Prev Shader",
            ChangeConfigCmd::FrameSkip(_) => "Frame Skip",
            ChangeConfigCmd::SetGbModel(_) => "Model",
            ChangeConfigCmd::TargetFps(_) => "Target Fps",
        }
    }
}

impl fmt::Display for ChangeConfigCmd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
