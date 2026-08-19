use crate::app::{App, AppState};
use crate::cmd::AppCmd;
use crate::frontend::{Frontend, FrontendCtx, NavAction};
use crate::{PlatformFileDialog, PlatformFileSystem};
use core::auxiliary::joypad::JoypadButton;
use core::emu::Emu;

pub fn handle_emu_btn<FS, FD>(
    btn: JoypadButton,
    pressed: bool,
    app: &mut App<FS, FD>,
    emu: &mut Emu,
) -> Option<AppCmd>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    match btn {
        JoypadButton::Start => return handle_start(pressed, app, emu),
        JoypadButton::Select => return handle_select(pressed, app, emu),
        JoypadButton::A => return handle_a(pressed, app, emu),
        JoypadButton::B => handle_b(pressed, app, emu),
        JoypadButton::Up => handle_up(pressed, app, emu),
        JoypadButton::Down => handle_down(pressed, app, emu),
        JoypadButton::Left => return handle_left(pressed, app, emu),
        JoypadButton::Right => return handle_right(pressed, app, emu),
    }

    None
}

/// While the UI is open a button drives it; otherwise it reaches the joypad.
fn nav<FS, FD>(action: NavAction, app: &mut App<FS, FD>) -> Option<AppCmd>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    let ctx = FrontendCtx {
        config: &app.config,
        fs: &app.platform.fs,
        roms: &app.roms,
        palettes: &app.palettes,
    };

    app.frontend.nav(action, ctx)
}

pub fn handle_up<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu)
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        nav(NavAction::Up, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.up = pressed;
    }
}

pub fn handle_down<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu)
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        nav(NavAction::Down, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.down = pressed;
    }
}

pub fn handle_left<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu) -> Option<AppCmd>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        return nav(NavAction::Left, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.left = pressed;
    }

    None
}

pub fn handle_right<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu) -> Option<AppCmd>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        return nav(NavAction::Right, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.right = pressed
    }

    None
}

pub fn handle_a<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu) -> Option<AppCmd>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        return nav(NavAction::Confirm, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.a = pressed;
    }

    None
}

pub fn handle_b<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu)
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        nav(NavAction::Back, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.b = pressed;
    }
}

/// Start is the options button while a menu is up: A confirms and B backs out, so
/// a second confirm is all it would otherwise be.
pub fn handle_start<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu) -> Option<AppCmd>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        return nav(NavAction::Options, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.start = pressed;
    }

    None
}

/// Select reaches the settings while a menu is up, which are otherwise several
/// presses of walking a header away.
pub fn handle_select<FS, FD>(pressed: bool, app: &mut App<FS, FD>, emu: &mut Emu) -> Option<AppCmd>
where
    FS: PlatformFileSystem,
    FD: PlatformFileDialog,
{
    if app.state == AppState::Paused && pressed {
        return nav(NavAction::Settings, app);
    } else {
        emu.runtime.cpu.clock.bus.io.joypad.select = pressed;
    }

    None
}
