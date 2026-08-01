//! The settings rows: what the page shows and what each row does.
//!
//! The `ui` crate is told about rows, never about config types, so this is the
//! one place that knows both. Every row reuses an existing [`ChangeConfigCmd`],
//! so applying and persisting a change stays the app's unchanged path.

use crate::cmd::{AppCmd, ChangeConfigCmd};
use crate::config::AppConfig;
use crate::palette::LcdPalette;
use core::apu::apu::CHANNELS_COUNT;
use core::emu::config::GbModel;
use ui::{Control, Row, Section, SettingId, SettingsView};

/// Row ids. `Channel` covers one row per APU channel, so it stays last.
const PALETTE: SettingId = 0;
const INVERT_PALETTE: SettingId = 1;
const FULLSCREEN: SettingId = 2;
const SHOW_FPS: SettingId = 3;
const SCALE: SettingId = 4;
const SHADER: SettingId = 5;
const VOLUME: SettingId = 6;
const AUDIO_BUFFER: SettingId = 7;
const MUTE: SettingId = 8;
const MUTE_TURBO: SettingId = 9;
const MUTE_SLOW: SettingId = 10;
const MODEL: SettingId = 11;
const AUTO_SAVE_STATE: SettingId = 12;
const NORMAL_SPEED: SettingId = 13;
const TURBO_SPEED: SettingId = 14;
const SLOW_SPEED: SettingId = 15;
const REWIND_SIZE: SettingId = 16;
const REWIND_FRAMES: SettingId = 17;
const TARGET_FPS: SettingId = 18;
const FRAME_SKIP: SettingId = 19;
const SPIN_DURATION: SettingId = 20;
const TILE_WINDOW: SettingId = 21;
const RESET_CONFIG: SettingId = 22;
const ROMS_DIR: SettingId = 23;
const CHANNEL: SettingId = 24;

/// One press worth of change, matching what the text menu applies.
const VOLUME_STEP: f32 = 0.05;
const SCALE_STEP: f32 = 1.0;
const SPEED_STEP: f32 = 0.1;
const REWIND_SIZE_STEP: i32 = 10;
const REWIND_FRAMES_STEP: i32 = 10;
const AUDIO_BUFFER_STEP: i32 = 1;
const TARGET_FPS_STEP: f32 = 1.0;
const FRAME_SKIP_STEP: i32 = 1;
const SPIN_STEP_MS: i32 = 1;

pub fn view(config: &AppConfig, palettes: &[LcdPalette]) -> SettingsView {
    let interface = &config.video.interface;
    let render = &config.video.render;
    let emu = &config.emulation;

    SettingsView {
        sections: vec![
            Section {
                title: "Interface".to_owned(),
                rows: vec![
                    stepper(PALETTE, "Palette", palette_name(config, palettes)),
                    toggle(
                        INVERT_PALETTE,
                        "Invert palette",
                        interface.is_palette_inverted,
                    ),
                    toggle(FULLSCREEN, "Fullscreen", interface.is_fullscreen),
                    toggle(SHOW_FPS, "Show FPS", interface.show_fps),
                    stepper(SCALE, "Scale", format!("{:.0}x", interface.scale)),
                    stepper(SHADER, "Shader", render.gl.shader_name.clone()),
                ],
            },
            Section {
                title: "Audio".to_owned(),
                rows: audio_rows(config),
            },
            Section {
                title: "System".to_owned(),
                rows: vec![
                    stepper(MODEL, "Model", model_name(emu.model).to_owned()),
                    toggle(AUTO_SAVE_STATE, "Auto save state", config.auto_save_state),
                    stepper(NORMAL_SPEED, "Speed", format!("{:.1}x", emu.normal_speed)),
                    stepper(
                        TURBO_SPEED,
                        "Turbo speed",
                        format!("{:.1}x", emu.turbo_speed),
                    ),
                    stepper(SLOW_SPEED, "Slow speed", format!("{:.1}x", emu.slow_speed)),
                    stepper(REWIND_SIZE, "Rewind size", emu.rewind_size.to_string()),
                    stepper(
                        REWIND_FRAMES,
                        "Rewind frames",
                        emu.rewind_frames.to_string(),
                    ),
                ],
            },
            Section {
                title: "Advanced".to_owned(),
                rows: vec![
                    stepper(
                        TARGET_FPS,
                        "Target FPS",
                        format!("{:.0}", render.target_fps),
                    ),
                    stepper(FRAME_SKIP, "Frame skip", render.frame_skip.to_string()),
                    stepper(
                        SPIN_DURATION,
                        "Spin duration",
                        format!("{} ms", emu.spin_duration.as_millis()),
                    ),
                    toggle(TILE_WINDOW, "Tile window", interface.show_tiles),
                    Row {
                        id: ROMS_DIR,
                        label: "ROMs directory".to_owned(),
                        control: Control::Action,
                    },
                    Row {
                        id: RESET_CONFIG,
                        label: "Reset config".to_owned(),
                        control: Control::Action,
                    },
                ],
            },
        ],
    }
}

pub fn apply(id: SettingId, step: i8, config: &AppConfig) -> Option<AppCmd> {
    let up = step >= 0;
    let cmd = match id {
        PALETTE if up => ChangeConfigCmd::NextPalette,
        PALETTE => ChangeConfigCmd::PrevPalette,
        INVERT_PALETTE => ChangeConfigCmd::InvertPalette,
        FULLSCREEN => return Some(AppCmd::ToggleFullscreen),
        SHOW_FPS => ChangeConfigCmd::Fps,
        SCALE => ChangeConfigCmd::Scale(signed(SCALE_STEP, up)),
        SHADER if up => ChangeConfigCmd::NextShader,
        SHADER => ChangeConfigCmd::PrevShader,
        VOLUME => ChangeConfigCmd::Volume(signed(VOLUME_STEP, up)),
        AUDIO_BUFFER => ChangeConfigCmd::AudioBufferSize(signed(AUDIO_BUFFER_STEP, up)),
        MUTE => ChangeConfigCmd::ToggleMute,
        MUTE_TURBO => ChangeConfigCmd::MuteTurbo,
        MUTE_SLOW => ChangeConfigCmd::MuteSlow,
        MODEL => ChangeConfigCmd::SetGbModel(next_model(config.emulation.model, step)),
        AUTO_SAVE_STATE => ChangeConfigCmd::AutoSaveState,
        NORMAL_SPEED => ChangeConfigCmd::NormalSpeed(signed(SPEED_STEP, up)),
        TURBO_SPEED => ChangeConfigCmd::TurboSpeed(signed(SPEED_STEP, up)),
        SLOW_SPEED => ChangeConfigCmd::SlowSpeed(signed(SPEED_STEP, up)),
        REWIND_SIZE => ChangeConfigCmd::RewindSize(signed(REWIND_SIZE_STEP, up)),
        REWIND_FRAMES => ChangeConfigCmd::RewindFrames(signed(REWIND_FRAMES_STEP, up)),
        TARGET_FPS => ChangeConfigCmd::TargetFps(signed(TARGET_FPS_STEP, up)),
        FRAME_SKIP => ChangeConfigCmd::FrameSkip(signed(FRAME_SKIP_STEP, up) as usize),
        SPIN_DURATION => ChangeConfigCmd::SpinDuration(signed(SPIN_STEP_MS, up)),
        TILE_WINDOW => ChangeConfigCmd::TileWindow,
        RESET_CONFIG => ChangeConfigCmd::Reset,
        // Not a config change: it opens a folder chooser and rescans.
        ROMS_DIR => return Some(AppCmd::SelectRomsDir),
        _ => ChangeConfigCmd::ToggleChannel((id - CHANNEL) as u8),
    };

    Some(AppCmd::ChangeConfig(cmd))
}

fn audio_rows(config: &AppConfig) -> Vec<Row> {
    let audio = &config.audio;
    let mut rows = vec![
        stepper(VOLUME, "Volume", format!("{:.0}%", audio.volume * 100.0)),
        stepper(AUDIO_BUFFER, "Buffer size", audio.buffer_size.to_string()),
        toggle(MUTE, "Mute", audio.mute),
        toggle(MUTE_TURBO, "Mute on turbo", audio.mute_turbo),
        toggle(MUTE_SLOW, "Mute on slow", audio.mute_slow),
    ];

    for channel in 0..CHANNELS_COUNT {
        let audible = audio.channel_mask & (1 << channel) != 0;
        rows.push(toggle(
            CHANNEL + channel as SettingId,
            &format!("Channel {}", channel + 1),
            audible,
        ));
    }

    rows
}

fn stepper(id: SettingId, label: &str, value: String) -> Row {
    Row {
        id,
        label: label.to_owned(),
        control: Control::Stepper(value),
    }
}

fn toggle(id: SettingId, label: &str, on: bool) -> Row {
    Row {
        id,
        label: label.to_owned(),
        control: Control::Toggle(on),
    }
}

fn signed<T: std::ops::Neg<Output = T>>(step: T, up: bool) -> T {
    if up {
        step
    } else {
        -step
    }
}

fn palette_name(config: &AppConfig, palettes: &[LcdPalette]) -> String {
    palettes
        .get(config.video.interface.selected_palette_idx)
        .map(|palette| palette.name.clone())
        .unwrap_or_default()
}

fn model_name(model: Option<GbModel>) -> &'static str {
    match model {
        None => "Auto",
        Some(GbModel::Dmg) => "DMG",
        Some(GbModel::Cgb) => "CGB",
    }
}

/// Cycles Auto → DMG → CGB in whichever direction the row was stepped.
fn next_model(current: Option<GbModel>, step: i8) -> Option<GbModel> {
    const ORDER: [Option<GbModel>; 3] = [None, Some(GbModel::Dmg), Some(GbModel::Cgb)];
    let at = ORDER
        .iter()
        .position(|model| *model == current)
        .unwrap_or(0);
    let next = if step >= 0 {
        at + 1
    } else {
        at + ORDER.len() - 1
    };

    ORDER[next % ORDER.len()]
}
