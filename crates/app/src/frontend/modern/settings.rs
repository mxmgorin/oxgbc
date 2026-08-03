//! The settings rows: what the pages show and what each row does.
//!
//! The `ui` crate is told about rows, never about config types, so this is the
//! one place that knows both. Every row reuses an existing [`ChangeConfigCmd`],
//! so applying and persisting a change stays the app's unchanged path.
//!
//! Rebinding gets a page per device rather than a row per action: a row on a
//! device's page shows and rebinds that device alone, which is what makes a capture
//! unambiguous — and keeps the settings from opening on a wall of bindings.

use crate::cmd::{AppCmd, BindCmds, BindComboCmd, BindInputCmd, BindTarget, ChangeConfigCmd};
use crate::config::{AppConfig, RenderConfig, ScaleMode, VideoBackendType, VideoConfig};
use crate::input::bindings::{BindableInput, InputBindings, InputKind};
use crate::video::frame_blend::{
    AdditiveFrameBlend, BlendProfile, ExponentialFrameBlend, FrameBlendMode,
    GammaCorrectedFrameBlend, LinearFrameBlend, DMG_PROFILE, POCKET_PROFILE,
};
use crate::video::palette::LcdPalette;
use crate::video::shader::ShaderFrameBlendMode;
use core::apu::apu::CHANNELS_COUNT;
use core::auxiliary::joypad::JoypadButton;
use core::emu::config::GbModel;
use core::emu::runtime::RunMode;
use core::emu::state::SaveStateCmd;
use ui::{Control, Page, PageId, Row, Section, SettingId, SettingsView};

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
const KEYBOARD: SettingId = 24;
const GAMEPAD: SettingId = 25;
const CHANNEL: SettingId = 26;
/// The video rows, clear of `CHANNEL`'s per-channel block.
const VIDEO: SettingId = 32;
const SCALE_MODE: SettingId = 33;
const BACKEND: SettingId = 34;
const BLEND_MODE: SettingId = 35;
const BLEND_ALPHA: SettingId = 36;
const BLEND_FADE: SettingId = 37;
const BLEND_DIM: SettingId = 38;
const BLEND_PROFILE: SettingId = 39;
const BLEND_RISE: SettingId = 40;
const BLEND_FALL: SettingId = 41;
const BLEND_BLEED: SettingId = 42;
const GRID: SettingId = 43;
const SUBPIXEL: SettingId = 44;
const SCANLINE: SettingId = 45;
const DOT_MATRIX: SettingId = 46;
const VIGNETTE: SettingId = 47;
const SHADER_BLEND: SettingId = 48;
const COMBO_INTERVAL: SettingId = 49;
/// Where the rebinding rows start. Held clear of `CHANNEL`, which grows with the
/// APU's channel count; a binding row's id is its block's base plus its place in
/// [`bindable`].
const BIND: SettingId = 64;
/// Room for one row per [`bindable`] entry in each block, with the same entry
/// keeping the same offset throughout.
const BIND_STRIDE: SettingId = 64;
/// A block per device, since a row rebinds the device whose page it is on, and one
/// more for the combos — which only a pad has.
const PAD_BIND: SettingId = BIND + BIND_STRIDE;
const COMBO: SettingId = PAD_BIND + BIND_STRIDE;

/// The pages, by their place in [`SettingsView::pages`]; the root is [`ui::ROOT_PAGE`].
const KEYBOARD_PAGE: PageId = 1;
const GAMEPAD_PAGE: PageId = 2;
const VIDEO_PAGE: PageId = 3;

/// What a row waiting for an input says, and what it says once a combo's first
/// button is down and it wants the second.
const AWAITING: &str = "Press an input…";
const AWAITING_SECOND: &str = "+ …";
/// Shown by a row nothing reaches.
const UNBOUND: &str = "None";

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
const BLEND_STEP: f32 = 0.05;
const COMBO_STEP_US: i32 = 5_000;

/// What the page is waiting for, if anything.
#[derive(Clone, Copy, Default, Eq, PartialEq)]
pub struct Capturing {
    pub row: Option<SettingId>,
    /// The first button of a combo, once it is down and the second is awaited.
    pub first: Option<usize>,
}

/// What reaches `pressed` on this device today. A page shows one device, so the
/// other's table is not consulted: its own page says what it is bound to.
fn bound_to<I: BindableInput>(bindings: &InputBindings<I>, pressed: &AppCmd) -> String {
    // Only the press half: the release is bound to the same input and would list
    // every name twice.
    let names: Vec<&str> = bindings
        .inputs(pressed)
        .into_iter()
        .filter(|(_, pressed)| *pressed)
        .map(|(input, _)| input.name())
        .collect();

    if names.is_empty() {
        UNBOUND.to_owned()
    } else {
        names.join(" · ")
    }
}

/// Which pairs of pad buttons reach `pressed`. Several can: the defaults give one
/// action a pair per pad layout, since not every pad has a Guide button.
fn combos_for(config: &AppConfig, pressed: &AppCmd) -> String {
    let names: Vec<String> = config
        .input
        .bindings
        .gamepad
        .combo
        .iter_combos()
        .filter(|combo| combo.cmd == *pressed)
        .map(|combo| format!("{} + {}", combo.btn_1.name(), combo.btn_2.name()))
        .collect();

    if names.is_empty() {
        UNBOUND.to_owned()
    } else {
        names.join(" · ")
    }
}

fn button_name(code: usize) -> &'static str {
    sdl2::controller::Button::from_code(code).map_or("?", |button| button.name())
}

fn binding_rows<I: BindableInput>(
    base: SettingId,
    bindings: &InputBindings<I>,
    capturing: Capturing,
) -> Vec<Row> {
    bindable()
        .into_iter()
        .enumerate()
        .map(|(index, (label, target))| {
            let id = base + index as SettingId;
            let waiting = capturing.row == Some(id);
            let current = if waiting {
                AWAITING.to_owned()
            } else {
                bound_to(bindings, &target.cmds().0)
            };

            Row {
                id,
                label: label.to_owned(),
                control: Control::Binding {
                    current,
                    capturing: waiting,
                },
            }
        })
        .collect()
}

/// Combos are gamepad-only and fire on the press alone, so an action that has to be
/// undone on release — turbo, slow, rewind — cannot have one: the mode would latch
/// on with nothing left to turn it off. The list says which those are, rather than
/// this repeating them.
fn combo_rows(config: &AppConfig, capturing: Capturing) -> Vec<Row> {
    bindable()
        .into_iter()
        .enumerate()
        .filter(|(_, (_, target))| target.cmds().1.is_none())
        .map(|(index, (label, target))| {
            let id = COMBO + index as SettingId;
            let waiting = capturing.row == Some(id);
            let current = match (waiting, capturing.first) {
                (true, Some(first)) => format!("{} {AWAITING_SECOND}", button_name(first)),
                (true, None) => AWAITING.to_owned(),
                (false, _) => combos_for(config, &target.cmds().0),
            };

            Row {
                id,
                label: label.to_owned(),
                control: Control::Binding {
                    current,
                    capturing: waiting,
                },
            }
        })
        .collect()
}

pub fn view(config: &AppConfig, palettes: &[LcdPalette], capturing: Capturing) -> SettingsView {
    let bindings = &config.input.bindings;
    let mut gamepad = device_page("Gamepad", PAD_BIND, &bindings.gamepad.buttons, capturing);
    // Only a pad has them, so the section is the gamepad page's alone.
    gamepad.sections.push(Section {
        title: "Combos".to_owned(),
        rows: combo_rows(config, capturing),
    });

    SettingsView {
        pages: vec![
            root_page(config, palettes),
            device_page("Keyboard", BIND, &bindings.keyboard, capturing),
            gamepad,
            video_page(&config.video),
        ],
    }
}

/// A row is listed only where it does something: filters belong to the SDL2 backend,
/// shader rows to the GL one.
fn video_page(video: &VideoConfig) -> Page {
    let render = &video.render;
    let mut sections = vec![
        Section {
            title: "Display".to_owned(),
            rows: vec![
                stepper(
                    SCALE_MODE,
                    "Scale mode",
                    video.interface.scale_mode.to_string(),
                ),
                stepper(BACKEND, "Backend", render.backend.to_string()),
            ],
        },
        Section {
            title: "Frame blend".to_owned(),
            rows: blend_rows(render),
        },
    ];

    match render.backend {
        VideoBackendType::Sdl2 => sections.push(Section {
            title: "Filters".to_owned(),
            rows: vec![
                toggle(GRID, "Grid", render.sdl2.grid_enabled),
                toggle(SUBPIXEL, "Subpixel", render.sdl2.subpixel_enabled),
                toggle(SCANLINE, "Scanline", render.sdl2.scanline_enabled),
                toggle(DOT_MATRIX, "Dot matrix", render.sdl2.dot_matrix_enabled),
                toggle(VIGNETTE, "Vignette", render.sdl2.vignette_enabled),
            ],
        }),
        VideoBackendType::Gl => sections.push(Section {
            title: "Shader".to_owned(),
            rows: vec![
                stepper(SHADER, "Shader", render.gl.shader_name.clone()),
                stepper(
                    SHADER_BLEND,
                    "Frame blend",
                    render.gl.shader_frame_blend_mode.to_string(),
                ),
            ],
        }),
    }

    Page {
        title: "Video".to_owned(),
        sections,
    }
}

/// Which parameters exist depends on the mode, so the rows follow it.
fn blend_rows(render: &RenderConfig) -> Vec<Row> {
    let blend = &render.frame_blend_mode;
    let mut rows = vec![stepper(BLEND_MODE, "Mode", blend.name().to_owned())];

    if matches!(blend, FrameBlendMode::None) {
        return rows;
    }

    if has_alpha(blend) {
        rows.push(stepper(
            BLEND_ALPHA,
            "Alpha",
            format!("{:.2}", blend.alpha()),
        ));
    }

    if has_fade(blend) {
        rows.push(stepper(BLEND_FADE, "Fade", format!("{:.2}", blend.fade())));
    }

    rows.push(stepper(
        BLEND_DIM,
        "Dim",
        format!("{:.2}", render.blend_dim),
    ));

    if let Some(profile) = blend.profile() {
        rows.push(stepper(BLEND_PROFILE, "Profile", profile.name().to_owned()));
        rows.push(stepper(BLEND_RISE, "Rise", format!("{:.2}", profile.rise)));
        rows.push(stepper(BLEND_FALL, "Fall", format!("{:.2}", profile.fall)));
        rows.push(stepper(
            BLEND_BLEED,
            "Bleed",
            format!("{:.2}", profile.bleed),
        ));
    }

    rows
}

fn has_alpha(blend: &FrameBlendMode) -> bool {
    matches!(
        blend,
        FrameBlendMode::Linear(_) | FrameBlendMode::Additive(_) | FrameBlendMode::Gamma(_)
    )
}

fn has_fade(blend: &FrameBlendMode) -> bool {
    matches!(
        blend,
        FrameBlendMode::Additive(_) | FrameBlendMode::Exp(_) | FrameBlendMode::Gamma(_)
    )
}

/// The buttons and shortcuts of one device, each device on a page of its own: a row
/// rebinds the device whose page it is on, so the two never have to be told apart
/// once a capture is open.
fn device_page<I: BindableInput>(
    title: &str,
    base: SettingId,
    bindings: &InputBindings<I>,
    capturing: Capturing,
) -> Page {
    let mut buttons = binding_rows(base, bindings, capturing);
    let shortcuts = buttons.split_off(BUTTON_ROWS);

    Page {
        title: title.to_owned(),
        sections: vec![
            Section {
                title: "Buttons".to_owned(),
                rows: buttons,
            },
            Section {
                title: "Shortcuts".to_owned(),
                rows: shortcuts,
            },
        ],
    }
}

fn root_page(config: &AppConfig, palettes: &[LcdPalette]) -> Page {
    let interface = &config.video.interface;
    let render = &config.video.render;
    let emu = &config.emulation;

    Page {
        title: "Settings".to_owned(),
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
                    page(VIDEO, "Video", VIDEO_PAGE),
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
                title: "Input".to_owned(),
                rows: vec![
                    page(KEYBOARD, "Keyboard", KEYBOARD_PAGE),
                    page(GAMEPAD, "Gamepad", GAMEPAD_PAGE),
                    stepper(
                        COMBO_INTERVAL,
                        "Combo interval",
                        format!("{} ms", config.input.combo_interval.as_millis()),
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

/// How many of [`bindable`]'s leading entries are joypad buttons; the rest are the
/// app's own shortcuts, and the two are shown apart.
const BUTTON_ROWS: usize = 12;

/// Everything that can be rebound, in the order the page lists it.
///
/// Built rather than `const`: an [`AppCmd`] carries owned data, so a target cannot be
/// a constant. Cheap all the same — the page is rebuilt only when the config changes.
fn bindable() -> Vec<(&'static str, BindTarget)> {
    use JoypadButton::*;

    let mut rows: Vec<(&'static str, BindTarget)> = [
        ("Up", &[Up][..]),
        ("Down", &[Down]),
        ("Left", &[Left]),
        ("Right", &[Right]),
        ("A", &[A]),
        ("B", &[B]),
        ("Start", &[Start]),
        ("Select", &[Select]),
        ("Up + Left", &[Up, Left]),
        ("Up + Right", &[Up, Right]),
        ("Down + Left", &[Down, Left]),
        ("Down + Right", &[Down, Right]),
    ]
    .into_iter()
    .map(|(label, buttons)| (label, BindTarget::Buttons(buttons.into())))
    .collect();

    debug_assert_eq!(rows.len(), BUTTON_ROWS);

    // A mode that lasts while a key is held has to put the emulator back on release,
    // so those three carry a second command.
    let held = |mode| Some(AppCmd::ChangeMode(mode));
    rows.extend(
        [
            ("Menu", AppCmd::ToggleMenu, None),
            (
                "Save state",
                AppCmd::SaveState(SaveStateCmd::Create, None),
                None,
            ),
            (
                "Load state",
                AppCmd::SaveState(SaveStateCmd::Load, None),
                None,
            ),
            (
                "Next slot",
                AppCmd::ChangeConfig(ChangeConfigCmd::IncSaveAndLoadSlots),
                None,
            ),
            ("Rewind", AppCmd::ToggleRewind, held(RunMode::Normal)),
            (
                "Turbo",
                AppCmd::ChangeMode(RunMode::Turbo),
                held(RunMode::Normal),
            ),
            (
                "Slow",
                AppCmd::ChangeMode(RunMode::Slow),
                held(RunMode::Normal),
            ),
            (
                "Next palette",
                AppCmd::ChangeConfig(ChangeConfigCmd::NextPalette),
                None,
            ),
            (
                "Invert palette",
                AppCmd::ChangeConfig(ChangeConfigCmd::InvertPalette),
                None,
            ),
            (
                "Next shader",
                AppCmd::ChangeConfig(ChangeConfigCmd::NextShader),
                None,
            ),
        ]
        .into_iter()
        .map(|(label, pressed, released)| {
            (label, BindTarget::Cmds(BindCmds::new(pressed, released)))
        }),
    );

    rows
}

/// Whether this row is a binding rather than a config value — those are not applied,
/// they start a capture.
pub fn is_binding(id: SettingId) -> bool {
    id >= BIND
}

/// Whether the capture this row starts wants a pair of pad buttons rather than one
/// input.
pub fn is_combo(id: SettingId) -> bool {
    id >= COMBO
}

/// Which device a binding row listens to, which its block already says: the pages
/// are per device, so a capture takes nothing else. Meaningless for other rows.
pub fn device(id: SettingId) -> InputKind {
    if id < PAD_BIND {
        InputKind::Keyboard
    } else {
        InputKind::Gamepad
    }
}

/// The action a binding row stands for, whichever block its id falls in — the same
/// entry of [`bindable`] keeps its offset in all of them.
fn target_of(id: SettingId) -> Option<BindTarget> {
    let base = if id >= COMBO {
        COMBO
    } else if id >= PAD_BIND {
        PAD_BIND
    } else {
        BIND
    };
    let at = id.checked_sub(base)?;

    bindable()
        .into_iter()
        .nth(at as usize)
        .map(|(_, target)| target)
}

/// The command that points `input` at the action row `id` stands for.
pub fn bind<I: BindableInput>(id: SettingId, input: I) -> Option<AppCmd> {
    let target = target_of(id)?;

    Some(AppCmd::BindInput(BindInputCmd::new(input, true, target)))
}

/// The command that points a pair of pad buttons at the action row `id` stands for.
pub fn bind_combo(id: SettingId, first: usize, second: usize) -> Option<AppCmd> {
    let target = target_of(id)?;

    Some(AppCmd::BindCombo(BindComboCmd {
        first,
        second,
        target,
    }))
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
        COMBO_INTERVAL => ChangeConfigCmd::ComboInterval(signed(COMBO_STEP_US, up)),
        // Not a config change: it opens a folder chooser and rescans.
        ROMS_DIR => return Some(AppCmd::SelectRomsDir),
        SCALE_MODE..=SHADER_BLEND => return video(id, step, &config.video),
        _ => ChangeConfigCmd::ToggleChannel((id - CHANNEL) as u8),
    };

    Some(AppCmd::ChangeConfig(cmd))
}

/// The video rows all travel as one whole config: the app applies a `Video` change
/// by handing the new config to the backend, so a row edits a copy of it.
fn video(id: SettingId, step: i8, video: &VideoConfig) -> Option<AppCmd> {
    let up = step >= 0;
    let delta = signed(BLEND_STEP, up);
    let mut next = video.clone();
    let render = &mut next.render;

    match id {
        SCALE_MODE => next.interface.scale_mode = next_scale_mode(video.interface.scale_mode, up),
        BACKEND => render.backend = next_backend(render.backend),
        BLEND_MODE => render.frame_blend_mode = next_blend(&video.render.frame_blend_mode, up),
        BLEND_ALPHA => render.frame_blend_mode.change_alpha(delta),
        BLEND_FADE => render.frame_blend_mode.change_fade(delta),
        BLEND_DIM => render.change_dim(delta),
        BLEND_PROFILE => render.frame_blend_mode = FrameBlendMode::Accurate(next_profile(video)?),
        BLEND_RISE | BLEND_FALL | BLEND_BLEED => {
            render.frame_blend_mode = FrameBlendMode::Accurate(tuned_profile(id, delta, video)?)
        }
        GRID => render.sdl2.grid_enabled = !render.sdl2.grid_enabled,
        SUBPIXEL => render.sdl2.subpixel_enabled = !render.sdl2.subpixel_enabled,
        SCANLINE => render.sdl2.scanline_enabled = !render.sdl2.scanline_enabled,
        DOT_MATRIX => render.sdl2.dot_matrix_enabled = !render.sdl2.dot_matrix_enabled,
        VIGNETTE => render.sdl2.vignette_enabled = !render.sdl2.vignette_enabled,
        SHADER_BLEND => {
            render.gl.shader_frame_blend_mode = next_shader_blend(render.gl.shader_frame_blend_mode)
        }
        _ => return None,
    }

    Some(AppCmd::ChangeConfig(ChangeConfigCmd::Video(Box::new(next))))
}

fn next_scale_mode(mode: ScaleMode, up: bool) -> ScaleMode {
    let at = match mode {
        ScaleMode::Integer => 0,
        ScaleMode::Fit => 1,
        ScaleMode::Stretch => 2,
    };

    match wrapped(at, 3, up) {
        0 => ScaleMode::Integer,
        1 => ScaleMode::Fit,
        _ => ScaleMode::Stretch,
    }
}

/// Two backends, so direction makes no difference. The app says a restart is
/// required; nothing here has to.
fn next_backend(backend: VideoBackendType) -> VideoBackendType {
    match backend {
        VideoBackendType::Sdl2 => VideoBackendType::Gl,
        VideoBackendType::Gl => VideoBackendType::Sdl2,
    }
}

fn next_shader_blend(mode: ShaderFrameBlendMode) -> ShaderFrameBlendMode {
    match mode {
        ShaderFrameBlendMode::None => ShaderFrameBlendMode::Simple,
        ShaderFrameBlendMode::Simple => ShaderFrameBlendMode::AccEven,
        ShaderFrameBlendMode::AccEven => ShaderFrameBlendMode::AccOdd,
        ShaderFrameBlendMode::AccOdd => ShaderFrameBlendMode::None,
    }
}

/// Every mode starts from its own defaults, so stepping through them and back
/// leaves the parameters where they were rather than at the last mode's values.
fn next_blend(mode: &FrameBlendMode, up: bool) -> FrameBlendMode {
    const COUNT: usize = 6;
    let at = match mode {
        FrameBlendMode::None => 0,
        FrameBlendMode::Linear(_) => 1,
        FrameBlendMode::Additive(_) => 2,
        FrameBlendMode::Exp(_) => 3,
        FrameBlendMode::Gamma(_) => 4,
        FrameBlendMode::Accurate(_) => 5,
    };

    match wrapped(at, COUNT, up) {
        1 => FrameBlendMode::Linear(LinearFrameBlend::default()),
        2 => FrameBlendMode::Additive(AdditiveFrameBlend::default()),
        3 => FrameBlendMode::Exp(ExponentialFrameBlend::default()),
        4 => FrameBlendMode::Gamma(GammaCorrectedFrameBlend::default()),
        5 => FrameBlendMode::Accurate(DMG_PROFILE),
        _ => FrameBlendMode::None,
    }
}

fn next_profile(video: &VideoConfig) -> Option<BlendProfile> {
    let profile = video.render.frame_blend_mode.profile()?;

    Some(if profile == &DMG_PROFILE {
        POCKET_PROFILE
    } else {
        DMG_PROFILE
    })
}

/// Hand-tuning a profile drops its tint back to neutral, as the text menu does:
/// the stored tints belong to the two presets.
fn tuned_profile(id: SettingId, delta: f32, video: &VideoConfig) -> Option<BlendProfile> {
    let mut profile = video.render.frame_blend_mode.profile()?.clone();
    let field = match id {
        BLEND_RISE => &mut profile.rise,
        BLEND_FALL => &mut profile.fall,
        _ => &mut profile.bleed,
    };
    *field = core::change_f32_rounded(*field, delta).clamp(0.0, 1.0);
    profile.tint.reset();

    Some(profile)
}

fn wrapped(at: usize, count: usize, up: bool) -> usize {
    if up {
        core::move_next_wrapped(at, count - 1)
    } else {
        core::move_prev_wrapped(at, count - 1)
    }
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

fn page(id: SettingId, label: &str, page: PageId) -> Row {
    Row {
        id,
        label: label.to_owned(),
        control: Control::Page(page),
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A binding row's id is its place in the same list offset by its block's base,
    /// so no block may run into the next or a row would answer to another's id.
    #[test]
    fn the_id_blocks_do_not_meet() {
        assert!(bindable().len() <= BIND_STRIDE as usize);
        assert!(CHANNEL + CHANNELS_COUNT as SettingId <= BIND);
    }

    /// Every row must be claimed by an arm of `apply`. The fallback arm turns an
    /// unclaimed id into a channel toggle, so a new row without its own arm would
    /// silently mute an APU channel.
    #[test]
    fn every_row_reaches_its_own_command() {
        for backend in [VideoBackendType::Sdl2, VideoBackendType::Gl] {
            let mut config = AppConfig::default();
            config.video.render.backend = backend;
            let palettes = LcdPalette::default_palettes();
            let view = view(&config, &palettes, Capturing::default());

            for page in &view.pages {
                for row in page.sections.iter().flat_map(|s| s.rows.iter()) {
                    // A page leads somewhere and a binding row opens a capture;
                    // neither goes through `apply`.
                    if matches!(row.control, Control::Page(_) | Control::Binding { .. }) {
                        continue;
                    }

                    let channels = CHANNEL..CHANNEL + CHANNELS_COUNT as SettingId;
                    let cmd = apply(row.id, 1, &config);
                    let toggled_channel = matches!(
                        cmd,
                        Some(AppCmd::ChangeConfig(ChangeConfigCmd::ToggleChannel(_)))
                    );

                    assert!(cmd.is_some(), "row {} asks for nothing", row.label);
                    assert_eq!(
                        toggled_channel,
                        channels.contains(&row.id),
                        "row {} fell through to the channel arm",
                        row.label
                    );
                }
            }
        }
    }

    /// Which table a capture writes to follows from the row's id alone, so the pages
    /// need no state of their own once one is open.
    #[test]
    fn a_row_belongs_to_the_device_of_its_page() {
        assert_eq!(device(BIND), InputKind::Keyboard);
        assert_eq!(device(BIND + BUTTON_ROWS as SettingId), InputKind::Keyboard);
        assert_eq!(device(PAD_BIND), InputKind::Gamepad);
        assert_eq!(device(COMBO), InputKind::Gamepad);
    }

    /// The same action keeps its offset in every block, so one list describes them
    /// all.
    #[test]
    fn the_blocks_line_up_on_the_same_actions() {
        let at = bindable()
            .into_iter()
            .position(|(label, _)| label == "Menu")
            .expect("the menu is bindable") as SettingId;

        assert_eq!(target_of(BIND + at), target_of(PAD_BIND + at));
        assert_eq!(target_of(BIND + at), target_of(COMBO + at));
    }

    /// Every page a row leads to has to exist, or Confirm would open nothing.
    #[test]
    fn the_input_rows_lead_to_pages_that_exist() {
        let view = view(&AppConfig::default(), &[], Capturing::default());
        let pages = view
            .pages
            .iter()
            .flat_map(|page| page.sections.iter())
            .flat_map(|section| section.rows.iter())
            .filter_map(|row| match row.control {
                Control::Page(page) => Some(page),
                _ => None,
            });

        for page in pages {
            assert!(view.pages.get(page).is_some(), "page {page} is missing");
        }

        assert_eq!(view.pages[KEYBOARD_PAGE].title, "Keyboard");
        assert_eq!(view.pages[GAMEPAD_PAGE].title, "Gamepad");
    }

    /// What [`combo_rows`] filters on. A combo fires on the press alone, so an action
    /// that has to be undone on release must not get one.
    #[test]
    fn a_mode_that_has_to_be_undone_gets_no_combo_row() {
        let held: Vec<&str> = bindable()
            .into_iter()
            .filter(|(_, target)| target.cmds().1.is_some())
            .map(|(label, _)| label)
            .collect();

        for latching in ["Turbo", "Slow", "Rewind", "A"] {
            assert!(held.contains(&latching), "{latching} latches, so no combo");
        }

        assert!(!held.contains(&"Menu"), "a plain toggle can be a combo");
    }

    #[test]
    fn only_a_binding_row_starts_a_capture() {
        assert!(!is_binding(PALETTE));
        assert!(!is_binding(CHANNEL));
        assert!(is_binding(BIND));
        assert!(is_binding(COMBO));
        assert!(!is_combo(BIND));
        assert!(is_combo(COMBO));
    }
}
