//! The egui frontend: the `ui` crate's screens over the paused game. This side
//! owns the platform half — building the view models from app state and turning
//! the UI's requests back into [`AppCmd`]s.

mod settings;
mod states;

use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::frontend::{Frontend, FrontendCtx, NavAction};
use crate::input::bindings::BindableInput;
use crate::rom_meta::RomMeta;
use crate::roms::RomsState;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::cart::header::CgbFlag;
use core::emu::state::SaveStateCmd;
use core::ppu::framebuffer::FrameBuffer;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Cap on egui's own repaint delay, so input keeps being polled while it idles.
const MAX_FRAME_DELAY: Duration = Duration::from_millis(30);

#[derive(Default)]
pub struct ModernFrontend {
    menu: ui::Menu,
    entries: Vec<ui::RomEntry>,
    paths: Vec<PathBuf>,
    /// Rebuilt from the config whenever the app reports a change.
    settings: ui::SettingsView,
    /// Same, from the save-state files of the loaded game.
    states: ui::StatesView,
    /// Slot whose screen was read out of its state file, to keep it off the disk
    /// while the sheet stays put.
    shot_slot: Option<usize>,
    /// Bumped for every rebuild, so the UI can tell one view from the next.
    version: u64,
    /// Filled by pointer input during `render`, drained by the app afterwards.
    pending: VecDeque<AppCmd>,
    stale: bool,
    frame_delay: Duration,
}

impl Frontend for ModernFrontend {
    fn new(roms: &RomsState) -> Self {
        let mut obj = Self {
            stale: true,
            ..Default::default()
        };
        obj.load_library(roms);

        obj
    }

    fn nav<FS: PlatformFileSystem>(
        &mut self,
        action: NavAction,
        ctx: FrontendCtx<'_, FS>,
    ) -> Option<AppCmd> {
        self.refresh(&ctx);
        // Built here rather than by a method, so the borrow stays split from the
        // menu's own field.
        let views = ui::Views {
            library: ui::LibraryView {
                entries: &self.entries,
            },
            settings: &self.settings,
            states: &self.states,
        };
        let cmd = self.menu.nav(into_nav(action), &views)?;

        self.app_cmd(cmd, ctx.config)
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

    fn render<FS: PlatformFileSystem>(
        &mut self,
        video: &mut AppVideo,
        fb: &mut FrameBuffer,
        ctx: FrontendCtx<'_, FS>,
    ) {
        self.refresh(&ctx);
        self.refresh_shot(&ctx);
        video.draw_menu(fb);

        let views = ui::Views {
            library: ui::LibraryView {
                entries: &self.entries,
            },
            settings: &self.settings,
            states: &self.states,
        };
        let menu = &mut self.menu;
        let mut cmds = Vec::new();
        video.render_egui(&mut |egui_ui| menu.show(egui_ui, &views, &mut cmds));

        for cmd in cmds {
            if let Some(cmd) = self.app_cmd(cmd, ctx.config) {
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
    /// Both view models are read-only snapshots, so they only need rebuilding
    /// when the app says something under them changed.
    fn refresh<FS: PlatformFileSystem>(&mut self, ctx: &FrontendCtx<'_, FS>) {
        if !self.stale {
            return;
        }

        self.load_library(ctx.roms);
        self.settings = settings::view(ctx.config, ctx.palettes);
        self.version += 1;
        self.states = states::view(ctx, self.version);
        // The rebuilt view dropped the screen read for the open slot with it.
        self.shot_slot = None;
        self.stale = false;
    }

    /// Fills in the open slot's screen when the slot has no shot of its own, which
    /// only states written before shots existed are missing. Costs a whole state
    /// file, so it runs once per slot the sheet lands on.
    fn refresh_shot<FS: PlatformFileSystem>(&mut self, ctx: &FrontendCtx<'_, FS>) {
        let open = self.menu.open_slot();

        if open == self.shot_slot {
            return;
        }

        self.shot_slot = open;
        let Some(slot) = open else {
            return;
        };
        let Some(state) = self.states.slots.iter_mut().find(|s| s.slot == slot) else {
            return;
        };

        if state.shot.is_none() {
            state.shot = states::load_shot(ctx, slot);
        }
    }

    fn load_library(&mut self, roms: &RomsState) {
        self.paths = roms.iter_opened().cloned().collect();
        self.entries = self
            .paths
            .iter()
            .map(|path| {
                let meta = rom_meta(path);

                ui::RomEntry {
                    title: title_of(path, &meta),
                    kind: kind_of(meta.cgb),
                }
            })
            .collect();
    }

    fn app_cmd(&self, cmd: ui::UiCmd, config: &AppConfig) -> Option<AppCmd> {
        Some(match cmd {
            ui::UiCmd::Setting { id, step } => return settings::apply(id, step, config),
            ui::UiCmd::LaunchRom(index) => AppCmd::LoadFile(self.paths.get(index)?.clone()),
            ui::UiCmd::Resume => AppCmd::ToggleMenu,
            ui::UiCmd::SaveState(slot) => AppCmd::SaveState(SaveStateCmd::Create, Some(slot)),
            ui::UiCmd::LoadState(slot) => AppCmd::SaveState(SaveStateCmd::Load, Some(slot)),
            ui::UiCmd::DeleteState(slot) => AppCmd::DeleteState(slot),
            ui::UiCmd::RenameState(slot, name) => AppCmd::RenameState(slot, name),
            ui::UiCmd::RenameRom(index, name) => {
                AppCmd::RenameRom(self.paths.get(index)?.clone(), name)
            }
            ui::UiCmd::RestartRom => AppCmd::RestartRom,
            ui::UiCmd::Quit => AppCmd::Quit,
        })
    }
}

/// The file name is what every sidecar goes by. Taken from the path rather than
/// through `PlatformFileSystem`, like the rest of this frontend's file handling:
/// it is the desktop and web builds that run it.
fn rom_meta(path: &Path) -> RomMeta {
    let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
        return RomMeta::default();
    };

    RomMeta::load_or_create(path, &name)
}

/// The user's name for the cart, or the file's own when it has none.
fn title_of(path: &Path, meta: &RomMeta) -> String {
    if !meta.name.is_empty() {
        return meta.name.clone();
    }

    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// Nintendo shipped the three shells by CGB support, so the header decides which
/// cart is drawn. It comes from the sidecar, which read it off the ROM once.
fn kind_of(cgb: CgbFlag) -> ui::CartKind {
    match cgb {
        CgbFlag::DmgOnly => ui::CartKind::Dmg,
        CgbFlag::CgbEnhanced => ui::CartKind::CgbCompatible,
        CgbFlag::CgbOnly => ui::CartKind::CgbOnly,
    }
}

fn into_nav(action: NavAction) -> ui::NavAction {
    match action {
        NavAction::Up => ui::NavAction::Up,
        NavAction::Down => ui::NavAction::Down,
        NavAction::Left => ui::NavAction::Left,
        NavAction::Right => ui::NavAction::Right,
        NavAction::Confirm => ui::NavAction::Confirm,
        NavAction::Back => ui::NavAction::Back,
        NavAction::Options => ui::NavAction::Options,
    }
}
