use crate::app::{App, AppState};
use crate::cmd::{AppCmd, BindTarget, ChangeConfigCmd};
use crate::config::AppConfig;
use crate::frontend::{BrowseTarget, Capture, Frontend, UiUpdate};
use crate::input::bindings::{BindableInput, InputBindings, InputIndex, InputKind};
use crate::input::emu::handle_emu_btn;
use crate::input::gamepad::GamepadHandler;
use crate::input::keyboard::handle_key;
use crate::{PlatformFileDialog, PlatformFileSystem};
use core::emu::state::EmuState;
use core::emu::Emu;
use sdl2::controller::{Button, GameController};
use sdl2::event::Event;
use sdl2::{EventPump, GameControllerSubsystem, Sdl};
use std::path::Path;

/// Points one input at one action, on whichever device's table it belongs to.
///
/// Both directions are cleared first: the action loses whatever else used to reach
/// it, and the input loses whatever else it used to do — otherwise a key rebound
/// from A to the menu would still release A when let go.
fn rebind<I: BindableInput>(
    bindings: &mut InputBindings<I>,
    index: InputIndex,
    target: BindTarget,
) {
    let Some(input) = index.into_input::<I>() else {
        log::warn!("Failed to bind: invalid input index {index:?}");

        return;
    };

    let (pressed, released) = target.cmds();
    bindings.clear_cmd(&pressed);
    bindings.bind_cmd(input, true, pressed);

    match released {
        Some(released) => {
            bindings.clear_cmd(&released);
            bindings.bind_cmd(input, false, released);
        }
        None => bindings.unbind(input, false),
    }
}

pub struct InputHandler {
    event_pump: EventPump,
    game_controllers: Vec<GameController>,
    game_controller_subsystem: GameControllerSubsystem,
    gamepad_handler: GamepadHandler,
}

impl InputHandler {
    pub fn new(sdl: &Sdl) -> Result<Self, String> {
        let mut game_controllers = vec![];
        let game_controller_subsystem = sdl.game_controller()?;

        for id in 0..game_controller_subsystem.num_joysticks()? {
            if game_controller_subsystem.is_game_controller(id) {
                let controller = game_controller_subsystem.open(id).unwrap();
                game_controllers.push(controller);
            }
        }

        Ok(Self {
            event_pump: sdl.event_pump()?,
            game_controllers,
            game_controller_subsystem,
            gamepad_handler: GamepadHandler::new(),
        })
    }

    /// Polls and handles events. Returns false on quit.
    pub fn handle_events<FS, FD>(&mut self, app: &mut App<FS, FD>, emu: &mut Emu)
    where
        FS: PlatformFileSystem,
        FD: PlatformFileDialog,
    {
        while let Some(event) = self.event_pump.poll_event() {
            // egui tracks window size and focus even while the game runs, but
            // only owns the input while its UI is up.
            #[cfg(feature = "frontend-modern")]
            if app.video.ui_took_event(&event) && app.state == AppState::Paused {
                continue;
            }

            match event {
                Event::ControllerDeviceAdded { which, .. } => {
                    if let Ok(controller) = self.game_controller_subsystem.open(which) {
                        self.game_controllers.push(controller);
                        log::info!("Controller {which} connected");
                    }
                }
                Event::ControllerDeviceRemoved { which, .. } => {
                    self.game_controllers.retain(|c| c.instance_id() != which);
                    log::info!("Controller {which} disconnected");
                }
                Event::DropFile { filename, .. } => {
                    self.handle_cmd(app, emu, AppCmd::LoadFile(filename.into()))
                }
                Event::KeyDown {
                    scancode: Some(sc), ..
                } => match app.frontend.capture_bind(sc, true) {
                    Capture::Took(cmd) => {
                        if let Some(cmd) = cmd {
                            self.handle_cmd(app, emu, cmd);
                        }
                    }
                    Capture::Pass => {
                        if let Some(cmd) = handle_key(&app.config.input, sc, true) {
                            self.handle_cmd(app, emu, cmd);
                        }
                    }
                },
                // Releases go through the capture too, or a combo would never notice
                // its first button being let go before the second arrived.
                Event::KeyUp {
                    scancode: Some(sc), ..
                } => match app.frontend.capture_bind(sc, false) {
                    Capture::Took(cmd) => {
                        if let Some(cmd) = cmd {
                            self.handle_cmd(app, emu, cmd);
                        }
                    }
                    Capture::Pass => {
                        if let Some(cmd) = handle_key(&app.config.input, sc, false) {
                            self.handle_cmd(app, emu, cmd);
                        }
                    }
                },
                Event::ControllerButtonDown { button, .. } => {
                    match app.frontend.capture_bind(button, true) {
                        Capture::Took(cmd) => {
                            if let Some(cmd) = cmd {
                                self.handle_cmd(app, emu, cmd);
                            }
                        }
                        Capture::Pass => {
                            if let Some(evt) =
                                self.gamepad_handler
                                    .handle_button(&app.config.input, button, true)
                            {
                                self.handle_cmd(app, emu, evt);
                            }
                        }
                    }
                }
                Event::ControllerButtonUp { button, .. } => {
                    match app.frontend.capture_bind(button, false) {
                        Capture::Took(cmd) => {
                            if let Some(cmd) = cmd {
                                self.handle_cmd(app, emu, cmd);
                            }
                        }
                        Capture::Pass => {
                            if let Some(evt) =
                                self.gamepad_handler
                                    .handle_button(&app.config.input, button, false)
                            {
                                self.handle_cmd(app, emu, evt);
                            }
                        }
                    }
                }
                Event::JoyAxisMotion {
                    axis_idx, value, ..
                } => {
                    if let Some(evt) = self.gamepad_handler.handle_axis(
                        &app.config.input.bindings.gamepad,
                        axis_idx,
                        value,
                    ) {
                        self.handle_cmd(app, emu, evt);
                    }
                }
                Event::Quit { .. } => self.handle_cmd(app, emu, AppCmd::Quit),
                Event::Window {
                    win_event: sdl2::event::WindowEvent::Close,
                    window_id,
                    ..
                } => {
                    if app.video.close_window(window_id) {
                        self.handle_cmd(app, emu, AppCmd::Quit);
                    }
                }
                Event::Window {
                    win_event: sdl2::event::WindowEvent::SizeChanged(..),
                    ..
                } => {
                    let mode = app.config.video.interface.scale_mode;
                    app.video.handle_resize(mode);
                }
                _ => {}
            }
        }
    }

    pub fn handle_cmd<FS, FD>(&mut self, app: &mut App<FS, FD>, emu: &mut Emu, cmd: AppCmd)
    where
        FS: PlatformFileSystem,
        FD: PlatformFileDialog,
    {
        match cmd {
            AppCmd::LoadFile(path) => {
                if let Err(err) = app.load_cart_file(emu, Path::new(&path)) {
                    log::warn!("Failed to load cart file: {err}");
                }
            }
            AppCmd::ToggleMenu => {
                if app.state == AppState::Paused && !emu.runtime.cpu.clock.bus.cart.is_empty() {
                    emu.runtime.cpu.clock.bus.io.joypad.reset();
                    app.state = AppState::Running;
                } else {
                    app.state = AppState::Paused;
                    app.frontend
                        .open(!emu.runtime.cpu.clock.bus.cart.is_empty());
                    app.frontend.request_update(UiUpdate::All);
                }
            }
            AppCmd::RestartRom => {
                app.restart_rom(emu);
            }
            AppCmd::ChangeMode(mode) => {
                emu.state = EmuState::Running;
                emu.runtime.set_mode(mode);
            }
            AppCmd::SaveState(event, index) => app.handle_save_state(emu, event, index),
            AppCmd::DeleteState(index) => app.handle_delete_state(index),
            AppCmd::RenameState(index, name) => app.handle_rename_state(index, name),
            AppCmd::RenameRom(path, name) => app.handle_rename_rom(&path, name),
            AppCmd::SetRomCover(rom) => {
                // Same rule as picking a game: only a picker this device's input can
                // reach is worth opening.
                if app.platform.fd.is_navigable() {
                    app.ask_rom_cover(&rom);
                } else {
                    let from = app.roms.last_browse_dir_path.clone();
                    app.frontend
                        .open_browse(BrowseTarget::Cover(rom), from.as_deref());
                }
            }
            AppCmd::UseRomCover(rom, image) => app.use_rom_cover(&rom, &image),
            AppCmd::RemoveRomCover(path) => app.handle_remove_rom_cover(&path),
            AppCmd::SetCoverFromState(path, index) => app.handle_cover_from_state(&path, index),
            AppCmd::SelectRom => {
                if app.state != AppState::Paused {
                    return;
                }

                // Only a picker the device's own input can reach is worth opening;
                // otherwise the app walks storage itself.
                if !app.platform.fd.is_navigable() {
                    let from = app.roms.last_browse_dir_path.clone();
                    app.frontend.open_browse(BrowseTarget::Rom, from.as_deref());

                    return;
                }

                if let Some(path) = app.platform.fd.select_file(
                    "Select Game Boy ROM",
                    (&["*.gb", "*.gbc"], "Game Boy ROMs (*.gb, *.gbc)"),
                ) {
                    if let Err(err) = app.load_cart_file(emu, Path::new(&path)) {
                        log::warn!("Failed to load cart file: {err}");
                    }
                }
            }
            AppCmd::ToggleRewind => {
                if emu.state == EmuState::Rewind {
                    emu.state = EmuState::Running
                } else {
                    emu.state = EmuState::Rewind
                }
            }
            AppCmd::Quit => app.state = AppState::Quitting,
            AppCmd::SelectRomsDir => {
                if !app.platform.fd.is_navigable() {
                    let from = app.roms.last_browse_dir_path.clone();
                    app.frontend.open_browse(BrowseTarget::Dir, from.as_deref());

                    return;
                }

                if let Some(dir) = app.platform.fd.select_dir("Select ROMs Folder") {
                    app.use_roms_dir(Path::new(&dir));
                }
            }
            AppCmd::UseRomsDir(dir) => app.use_roms_dir(&dir),
            AppCmd::ChangeConfig(cmd) => {
                match cmd {
                    ChangeConfigCmd::Volume(x) => app.change_volume(emu, x),
                    ChangeConfigCmd::Scale(x) => app.change_scale(x).unwrap(),
                    ChangeConfigCmd::TileWindow => {
                        app.config.video.interface.show_tiles =
                            !app.config.video.interface.show_tiles;
                        app.video.update_config(&app.config.video);
                    }
                    ChangeConfigCmd::Fullscreen => app.toggle_fullscreen(),
                    ChangeConfigCmd::Fps => {
                        app.config.video.interface.show_fps = !app.config.video.interface.show_fps;
                        emu.runtime
                            .cpu
                            .clock
                            .bus
                            .io
                            .ppu
                            .toggle_fps(app.config.video.interface.show_fps);
                    }
                    ChangeConfigCmd::SpinDuration(x) => {
                        emu.config.spin_duration =
                            core::change_duration(emu.config.spin_duration, x);
                        app.config.emulation.spin_duration = emu.config.spin_duration;
                    }
                    ChangeConfigCmd::NextPalette => app.next_palette(emu),
                    ChangeConfigCmd::PrevPalette => app.prev_palette(emu),
                    ChangeConfigCmd::ToggleMute => app.config.audio.mute = !app.config.audio.mute,
                    ChangeConfigCmd::NormalSpeed(x) => {
                        emu.config.normal_speed =
                            core::change_f64_rounded(emu.config.normal_speed, x as f64).max(0.05);
                        app.config.emulation.normal_speed = emu.config.normal_speed;
                    }
                    ChangeConfigCmd::TurboSpeed(x) => {
                        emu.config.turbo_speed =
                            core::change_f64_rounded(emu.config.turbo_speed, x as f64).max(0.05);
                        app.config.emulation.turbo_speed = emu.config.turbo_speed;
                    }
                    ChangeConfigCmd::SlowSpeed(x) => {
                        emu.config.slow_speed =
                            core::change_f64_rounded(emu.config.slow_speed, x as f64).max(0.05);
                        app.config.emulation.slow_speed = emu.config.slow_speed;
                    }
                    ChangeConfigCmd::RewindSize(x) => {
                        emu.config.rewind_size =
                            core::change_usize(emu.config.rewind_size, x).clamp(0, 500);
                        app.config.emulation.rewind_size = emu.config.rewind_size;
                    }
                    ChangeConfigCmd::RewindFrames(delta) => {
                        emu.config.rewind_frames =
                            core::change_usize(emu.config.rewind_frames, delta).clamp(0, 600);
                        app.config.emulation.rewind_frames = emu.config.rewind_frames;
                    }
                    ChangeConfigCmd::AutoSaveState => {
                        app.config.auto_save_state = !app.config.auto_save_state
                    }
                    ChangeConfigCmd::AudioBufferSize(x) => {
                        emu.runtime.cpu.clock.bus.io.apu.config.buffer_size = core::change_usize(
                            emu.runtime.cpu.clock.bus.io.apu.config.buffer_size,
                            x,
                        )
                        .clamp(0, 2560);
                        emu.runtime.cpu.clock.bus.io.apu.update_buffer_size();
                        app.config.audio.buffer_size =
                            emu.runtime.cpu.clock.bus.io.apu.config.buffer_size;
                    }
                    ChangeConfigCmd::MuteTurbo => {
                        app.config.audio.mute_turbo = !app.config.audio.mute_turbo
                    }
                    ChangeConfigCmd::MuteSlow => {
                        app.config.audio.mute_slow = !app.config.audio.mute_slow
                    }
                    ChangeConfigCmd::ToggleChannel(i) => {
                        app.config.audio.channel_mask ^= 1 << i;
                        emu.runtime.cpu.clock.bus.io.apu.config.channel_mask =
                            app.config.audio.channel_mask;
                    }
                    ChangeConfigCmd::Reset => {
                        app.config = AppConfig::default();
                        emu.config = app.config.emulation.clone();
                        app.notifications.add("Defaults restored");
                    }
                    ChangeConfigCmd::ComboInterval(x) => {
                        app.config.input.combo_interval =
                            core::change_duration(app.config.input.combo_interval, x);
                    }
                    ChangeConfigCmd::SetSaveSlot(x) => app.config.current_save_slot = x,
                    ChangeConfigCmd::SetLoadSlot(x) => app.config.current_load_slot = x,
                    ChangeConfigCmd::InvertPalette => {
                        app.config.video.interface.is_palette_inverted =
                            !app.config.video.interface.is_palette_inverted;
                        app.update_palette(emu);
                    }
                    ChangeConfigCmd::Video(x) => {
                        if app.config.video.render.backend != x.render.backend {
                            app.notifications.add("Restart is required to apply");
                        }

                        app.config.video = *x;
                        app.video.update_config(&app.config.video);
                    }
                    ChangeConfigCmd::IncSaveAndLoadSlots => {
                        app.config.inc_save_slot();
                        app.config.inc_load_slot();
                        app.notifications.add(format!(
                            "Save Slot: {}, Load Slot: {}",
                            app.config.current_save_slot, app.config.current_load_slot
                        ));
                    }
                    ChangeConfigCmd::DecSaveAndLoadSlots => {
                        app.config.dec_load_slot();
                        app.config.dec_save_slot();
                        app.notifications.add(format!(
                            "Save Slot: {}, Load Slot: {}",
                            app.config.current_save_slot, app.config.current_load_slot
                        ));
                    }
                    ChangeConfigCmd::NextShader => app.next_shader(),
                    ChangeConfigCmd::PrevShader => app.prev_shader(),
                    ChangeConfigCmd::FrameSkip(x) => {
                        app.config.video.render.frame_skip = x;
                        app.video.update_config(&app.config.video);
                    }
                    ChangeConfigCmd::SetGbModel(model) => {
                        app.config.emulation.model = model;
                        emu.config.model = model;
                        emu.runtime.cpu.clock.bus.update_model(model);
                        app.refresh_dmg_palette(emu);
                    }
                    // The shelf is built in the order it is sorted by.
                    ChangeConfigCmd::LibrarySort(sort) => {
                        app.config.library_sort = sort;
                        app.frontend.request_update(UiUpdate::Library);
                    }
                    ChangeConfigCmd::TargetFps(x) => {
                        app.config.video.render.target_fps = x;
                        app.video.update_config(&app.config.video);
                    }
                }

                // Both UIs show config values, so any change makes the screen stale.
                app.frontend.request_update(UiUpdate::Settings);
            }
            AppCmd::ReleaseButton(btn) => {
                if let Some(cmd) = handle_emu_btn(btn, false, app, emu) {
                    self.handle_cmd(app, emu, cmd);
                }
            }
            AppCmd::PressButton(btn) => {
                if let Some(cmd) = handle_emu_btn(btn, true, app, emu) {
                    self.handle_cmd(app, emu, cmd);
                }
            }
            AppCmd::SetFileBrowsePath(path) => app.roms.last_browse_dir_path = Some(path),
            AppCmd::ToggleFullscreen => app.toggle_fullscreen(),
            AppCmd::Macro(cmds) => {
                for cmd in cmds {
                    self.handle_cmd(app, emu, cmd);
                }
            }
            AppCmd::BindInput(bind_cmd) => {
                let bindings = &mut app.config.input.bindings;
                let index = bind_cmd.input_index;

                match bind_cmd.input_kind {
                    InputKind::Keyboard => rebind(&mut bindings.keyboard, index, bind_cmd.target),
                    InputKind::Gamepad => {
                        rebind(&mut bindings.gamepad.buttons, index, bind_cmd.target)
                    }
                }

                app.frontend.request_update(UiUpdate::Settings);
            }
            AppCmd::BindCombo(bind_cmd) => {
                let buttons = (
                    Button::from_code(bind_cmd.first),
                    Button::from_code(bind_cmd.second),
                );
                let (Some(first), Some(second)) = buttons else {
                    log::warn!("Failed to bind combo: unknown button code");

                    return;
                };
                // A combo fires on the press and has no release of its own, so only
                // the pressed command can ever reach it.
                let (pressed, _) = bind_cmd.target.cmds();
                let combos = &mut app.config.input.bindings.gamepad.combo;
                combos.clear_cmd(&pressed);
                combos.add_cmd(first, second, pressed);
                app.frontend.request_update(UiUpdate::Settings);
            }
            AppCmd::ToggleDebug => {
                #[cfg(feature = "debug")]
                emu.runtime.toggle_debug();
            }
            AppCmd::StepFrame => {
                app.state = AppState::Stepping;
                app.render_frame(emu);
                log::info!(
                    "Step frame: {}",
                    emu.runtime.cpu.clock.bus.io.ppu.current_frame
                );
            }
            AppCmd::StepScanline => {
                app.state = AppState::Stepping;
                app.render_scanline(emu);
                log::info!("Step scanline: {}", emu.runtime.cpu.clock.bus.io.ppu.lcd.ly);
            }
            AppCmd::ToggleStepping => match app.state {
                AppState::Paused | AppState::Quitting => {}
                AppState::Running => app.state = AppState::Stepping,
                AppState::Stepping => app.state = AppState::Running,
            },
            AppCmd::ClearScreen => {
                emu.get_framebuffer().clear();
                app.render_framebuffer(emu);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::auxiliary::joypad::JoypadButton;
    use sdl2::keyboard::Scancode;

    fn buttons(button: JoypadButton) -> BindTarget {
        BindTarget::Buttons(vec![button].into_boxed_slice())
    }

    fn bind_to(bindings: &mut InputBindings<Scancode>, key: Scancode, target: BindTarget) {
        rebind(bindings, InputIndex::new(key, true), target);
    }

    /// The point of rebinding from a settings row: the row shows one binding, so the
    /// input it used to be on has to stop working.
    #[test]
    fn rebinding_moves_an_action_rather_than_adding_to_it() {
        let mut bindings = InputBindings::<Scancode>::default();
        bindings.bind_btn(Scancode::Up, JoypadButton::Up);

        bind_to(&mut bindings, Scancode::W, buttons(JoypadButton::Up));

        assert_eq!(
            bindings.cmd(Scancode::W, true),
            Some(&AppCmd::PressButton(JoypadButton::Up))
        );
        assert_eq!(
            bindings.cmd(Scancode::W, false),
            Some(&AppCmd::ReleaseButton(JoypadButton::Up))
        );
        assert_eq!(bindings.cmd(Scancode::Up, true), None, "the old key");
        assert_eq!(bindings.cmd(Scancode::Up, false), None);
    }

    /// The other direction: whatever the captured input used to do, it stops doing.
    #[test]
    fn a_rebound_input_keeps_nothing_of_what_it_did_before() {
        let mut bindings = InputBindings::<Scancode>::default();
        bindings.bind_btn(Scancode::X, JoypadButton::A);

        bind_to(
            &mut bindings,
            Scancode::X,
            BindTarget::Cmds(crate::cmd::BindCmds::new(AppCmd::ToggleMenu, None)),
        );

        assert_eq!(bindings.cmd(Scancode::X, true), Some(&AppCmd::ToggleMenu));
        // A target with no release command must leave the release slot empty, or the
        // key would still be letting go of A.
        assert_eq!(bindings.cmd(Scancode::X, false), None);
    }

    /// A diagonal is a macro, and a macro is a different command from the plain
    /// button — rebinding one must not disturb the other.
    #[test]
    fn a_diagonal_and_its_parts_are_bound_apart() {
        let mut bindings = InputBindings::<Scancode>::default();
        bindings.bind_btn(Scancode::Up, JoypadButton::Up);

        bind_to(
            &mut bindings,
            Scancode::Y,
            BindTarget::Buttons(vec![JoypadButton::Up, JoypadButton::Left].into_boxed_slice()),
        );

        assert_eq!(
            bindings.cmd(Scancode::Up, true),
            Some(&AppCmd::PressButton(JoypadButton::Up)),
            "the plain button is untouched"
        );
        assert!(bindings.cmd(Scancode::Y, true).is_some());
    }
}
