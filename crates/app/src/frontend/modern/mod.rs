//! The egui frontend: the `ui` crate's screens over the paused game. This side
//! owns the platform half — building the view models from app state and turning
//! the UI's requests back into [`AppCmd`]s.

use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::frontend::{Frontend, FrontendCtx, NavAction};
use crate::input::bindings::BindableInput;
use crate::roms::RomsState;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::cart::header::{CartHeader, CgbFlag};
use core::emu::state::SaveStateCmd;
use core::ppu::framebuffer::FrameBuffer;
use std::collections::VecDeque;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Cap on egui's own repaint delay, so input keeps being polled while it idles.
const MAX_FRAME_DELAY: Duration = Duration::from_millis(30);

#[derive(Default)]
pub struct ModernFrontend {
    menu: ui::Menu,
    entries: Vec<ui::RomEntry>,
    paths: Vec<PathBuf>,
    /// Filled by pointer input during `render`, drained by the app afterwards.
    pending: VecDeque<AppCmd>,
    stale: bool,
    frame_delay: Duration,
}

impl Frontend for ModernFrontend {
    fn new(roms: &RomsState) -> Self {
        let mut obj = Self::default();
        obj.load_library(roms);

        obj
    }

    fn nav<FS: PlatformFileSystem>(
        &mut self,
        action: NavAction,
        _ctx: FrontendCtx<'_, FS>,
    ) -> Option<AppCmd> {
        let cmd = self.menu.nav(into_nav(action))?;

        self.app_cmd(cmd)
    }

    fn capture_bind<I: BindableInput>(&mut self, _input: I, _pressed: bool) -> Option<AppCmd> {
        None
    }

    fn request_update(&mut self) {
        self.stale = true;
    }

    fn open(&mut self, has_game: bool) {
        self.menu.open(has_game);
    }

    fn take_cmd(&mut self) -> Option<AppCmd> {
        self.pending.pop_front()
    }

    fn render(
        &mut self,
        video: &mut AppVideo,
        fb: &mut FrameBuffer,
        _config: &AppConfig,
        roms: &RomsState,
    ) {
        if self.stale {
            self.load_library(roms);
        }

        video.draw_menu(fb);
        let view = ui::LibraryView {
            entries: &self.entries,
        };
        let menu = &mut self.menu;
        let mut cmds = Vec::new();
        video.render_egui(&mut |egui_ui| menu.show(egui_ui, &view, &mut cmds));

        for cmd in cmds {
            if let Some(cmd) = self.app_cmd(cmd) {
                self.pending.push_back(cmd);
            }
        }

        self.frame_delay = video.egui_repaint_delay().min(MAX_FRAME_DELAY);
    }

    fn frame_delay(&self) -> Duration {
        self.frame_delay
    }
}

impl ModernFrontend {
    fn load_library(&mut self, roms: &RomsState) {
        self.paths = roms.iter_opened().cloned().collect();
        self.entries = self
            .paths
            .iter()
            .map(|path| ui::RomEntry {
                title: title_of(path),
                kind: kind_of(path),
            })
            .collect();
        self.stale = false;
    }

    fn app_cmd(&self, cmd: ui::UiCmd) -> Option<AppCmd> {
        Some(match cmd {
            ui::UiCmd::LaunchRom(index) => AppCmd::LoadFile(self.paths.get(index)?.clone()),
            ui::UiCmd::Resume => AppCmd::ToggleMenu,
            ui::UiCmd::SaveState => AppCmd::SaveState(SaveStateCmd::Create, None),
            ui::UiCmd::LoadState => AppCmd::SaveState(SaveStateCmd::Load, None),
            ui::UiCmd::RestartRom => AppCmd::RestartRom,
            ui::UiCmd::Quit => AppCmd::Quit,
        })
    }
}

fn title_of(path: &Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Nintendo shipped the three shells by CGB support, so the header decides which
/// cart is drawn. Read straight off disk for now — zipped ROMs would have to be
/// inflated, and the result belongs in a cache with the rest of the ROM metadata.
fn kind_of(path: &Path) -> ui::CartKind {
    match read_cgb_flag(path).unwrap_or_default() {
        CgbFlag::DmgOnly => ui::CartKind::Dmg,
        CgbFlag::CgbEnhanced => ui::CartKind::CgbCompatible,
        CgbFlag::CgbOnly => ui::CartKind::CgbOnly,
    }
}

fn read_cgb_flag(path: &Path) -> Option<CgbFlag> {
    let mut header = [0; CartHeader::END];
    File::open(path).ok()?.read_exact(&mut header).ok()?;

    Some(CartHeader::parse_cgb_flag(&header))
}

fn into_nav(action: NavAction) -> ui::NavAction {
    match action {
        NavAction::Up => ui::NavAction::Up,
        NavAction::Down => ui::NavAction::Down,
        NavAction::Left => ui::NavAction::Left,
        NavAction::Right => ui::NavAction::Right,
        NavAction::Confirm => ui::NavAction::Confirm,
        NavAction::Back => ui::NavAction::Back,
    }
}
