//! The egui frontend: the `ui` crate's screens over the paused game. This side
//! owns the platform half — building the view models from app state and turning
//! the UI's requests back into [`AppCmd`]s.

mod browse;
mod settings;
mod states;

use crate::cmd::AppCmd;
use crate::config::AppConfig;
use crate::file_browser::FileBrowser;
use crate::frontend::{BrowseTarget, Capture, Frontend, FrontendCtx, NavAction};
use crate::input::bindings::BindableInput;
use crate::rom_cover;
use crate::rom_meta::RomMeta;
use crate::roms::RomsState;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::cart::header::CgbFlag;
use core::emu::state::SaveStateCmd;
use core::ppu::framebuffer::FrameBuffer;
use std::collections::{HashSet, VecDeque};
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
    /// The settings row waiting for an input to land on it, and how far a combo's
    /// pair has got.
    capturing: settings::Capturing,
    /// Same, from the save-state files of the loaded game.
    states: ui::StatesView,
    /// Slot whose screen was read out of its state file, to keep it off the disk
    /// while the sheet stays put.
    shot_slot: Option<usize>,
    /// The save states of the cart whose cover is being worked on; read only while
    /// one of those screens is open.
    rom_states: ui::StatesView,
    /// Cart the states above belong to, so they are read once per cart.
    cover_rom: Option<usize>,
    /// Path of the loaded game, for the commands that name no cart of their own.
    loaded: Option<PathBuf>,
    /// Bumped for every rebuild, so the UI can tell one view from the next.
    version: u64,
    /// The storage walk, alive only while its screen is up.
    walk: Option<FileBrowser>,
    walk_target: BrowseTarget,
    browse: ui::BrowseView,
    /// Filled by pointer input during `render`, drained by the app afterwards.
    pending: VecDeque<AppCmd>,
    stale: bool,
    frame_delay: Duration,
}

impl Frontend for ModernFrontend {
    /// The library is left for the first refresh to build: it needs the platform's
    /// filesystem, which only a [`FrontendCtx`] carries.
    fn new(_roms: &RomsState) -> Self {
        Self {
            stale: true,
            ..Default::default()
        }
    }

    fn open_browse(&mut self, target: BrowseTarget, from: Option<&Path>) {
        // Where this session's walk stopped, else where the app remembers one
        // stopping — the same place the text menu picks up from.
        let last = self.walk.as_ref().map(|walk| walk.current_dir.clone());
        self.walk_target = target;
        self.walk = browse::start(&self.walk_target, last.as_deref().or(from));
        self.browse = browse::view(self.walk.as_ref(), &self.walk_target);
        self.menu.open_browse();
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
                version: self.version,
            },
            settings: &self.settings,
            states: &self.states,
            rom_states: &self.rom_states,
            browse: &self.browse,
        };
        let cmd = self.menu.nav(into_nav(action), &views)?;

        self.app_cmd(cmd, ctx.config)
    }

    fn capture_bind<I: BindableInput>(&mut self, input: I, pressed: bool) -> Capture {
        let Some(id) = self.capturing.row else {
            return Capture::Pass;
        };

        if pressed && input.is_cancel() {
            self.capturing = settings::Capturing::default();
            self.stale = true;

            return Capture::Took(None);
        }

        // A row rebinds the device of the page it is on, so the other device is
        // swallowed rather than bound and the row keeps waiting for its own.
        if input.kind() != settings::device(id) {
            return Capture::Took(None);
        }

        if settings::is_combo(id) {
            return self.capture_combo(id, input, pressed);
        }

        // Swallowed but not bound: the input that opened the capture is still on its
        // way up, and binding a row to the key that started it is never the intent.
        if !pressed {
            return Capture::Took(None);
        }

        self.capturing = settings::Capturing::default();
        self.stale = true;

        Capture::Took(settings::bind(id, input))
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
        self.refresh_cover_states();
        video.draw_menu(fb);

        let views = ui::Views {
            library: ui::LibraryView {
                entries: &self.entries,
                version: self.version,
            },
            settings: &self.settings,
            states: &self.states,
            rom_states: &self.rom_states,
            browse: &self.browse,
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
    /// A combo is two buttons held together, so the capture takes two presses: the
    /// first is remembered and shown, the second closes the pair. Letting the first
    /// go before the second arrives puts the row back to waiting.
    fn capture_combo<I: BindableInput>(
        &mut self,
        id: ui::SettingId,
        input: I,
        pressed: bool,
    ) -> Capture {
        let code = input.code();

        if !pressed {
            if self.capturing.first == Some(code) {
                self.capturing.first = None;
                self.stale = true;
            }

            return Capture::Took(None);
        }

        match self.capturing.first {
            None => {
                self.capturing.first = Some(code);
                self.stale = true;

                Capture::Took(None)
            }
            // The pad repeating the button it is already holding is not a pair.
            Some(first) if first == code => Capture::Took(None),
            Some(first) => {
                self.capturing = settings::Capturing::default();
                self.stale = true;

                Capture::Took(settings::bind_combo(id, first, code))
            }
        }
    }

    /// Both view models are read-only snapshots, so they only need rebuilding
    /// when the app says something under them changed.
    fn refresh<FS: PlatformFileSystem>(&mut self, ctx: &FrontendCtx<'_, FS>) {
        if !self.stale {
            return;
        }

        self.load_library(ctx);
        self.settings = settings::view(ctx.config, ctx.palettes, self.capturing);
        self.loaded = ctx.roms.get_last_path().cloned();
        self.version += 1;
        self.states = states::view(ctx, self.version);
        // The rebuilt views dropped the screens read into them.
        self.shot_slot = None;
        self.cover_rom = None;
        self.stale = false;
    }

    /// Reads a cart's save states when a cover screen moves to another cart: it is
    /// a stat per slot, and those screens outlive many frames.
    fn refresh_cover_states(&mut self) {
        let open = self.menu.open_cover_rom();

        if open == self.cover_rom {
            return;
        }

        self.cover_rom = open;
        // A new build of this view even though nothing else changed, so the picker's
        // textures have to go: another cart's slot 0 is not this one's.
        self.version += 1;
        self.rom_states = open
            .and_then(|index| self.paths.get(index))
            .and_then(|path| path.file_name())
            .map(|name| states::choices_for(&name.to_string_lossy(), self.version))
            .unwrap_or_default();
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

    /// The shelf is what has been played plus whatever else is in the chosen ROMs
    /// directory: played first and most recent leading, then the rest by name, so
    /// the order is the same on every run — the scan itself is unordered.
    ///
    /// One card per file *name*, not per path. Everything an app saves beside a game
    /// — battery, states, metadata, cover, play time — goes by that name, so two
    /// copies of a game in different folders already share all of it; shelving them
    /// twice would only pretend otherwise. The played copy wins, and of those the
    /// most recent.
    fn load_library<FS: PlatformFileSystem>(&mut self, ctx: &FrontendCtx<'_, FS>) {
        let mut seen = HashSet::new();
        let played = ctx.roms.iter_opened().cloned();
        let mut unplayed: Vec<PathBuf> =
            ctx.roms.iter_loaded(ctx.fs).into_iter().flatten().collect();
        unplayed.sort_by_key(|path| path.file_name().map(|name| name.to_ascii_lowercase()));

        self.paths = played
            .chain(unplayed)
            .filter(|path| match path.file_name() {
                Some(name) => seen.insert(name.to_owned()),
                None => false,
            })
            .collect();

        self.entries = self
            .paths
            .iter()
            .map(|path| {
                let meta = rom_meta(path);

                ui::RomEntry {
                    title: title_of(path, &meta),
                    kind: kind_of(meta.cgb),
                    cover: cover_of(path),
                }
            })
            .collect();
    }

    /// A folder only moves the walk along; a file ends it, and what it ends as
    /// depends on what the walk was for.
    fn browse_enter(&mut self, index: usize) -> Option<AppCmd> {
        let walk = self.walk.as_mut()?;
        let picked = browse::enter(walk, index);
        self.browse = browse::view(self.walk.as_ref(), &self.walk_target);

        let picked = picked?;
        self.menu.close_browse();

        // Remembered for the next walk, this run and the next.
        if let Some(walk) = self.walk.as_ref() {
            self.pending
                .push_back(AppCmd::SetFileBrowsePath(walk.current_dir.clone()));
        }

        match &self.walk_target {
            BrowseTarget::Rom => Some(AppCmd::LoadFile(picked)),
            BrowseTarget::Cover(rom) => Some(AppCmd::UseRomCover(rom.clone(), picked)),
            // A folder walk shows no files to end on.
            BrowseTarget::Dir => None,
        }
    }

    fn app_cmd(&mut self, cmd: ui::UiCmd, config: &AppConfig) -> Option<AppCmd> {
        Some(match cmd {
            // A binding row is not applied, it opens a capture; picking the row that
            // is already waiting puts it back rather than leaving no way out.
            ui::UiCmd::Setting { id, .. } if settings::is_binding(id) => {
                let row = (self.capturing.row != Some(id)).then_some(id);
                self.capturing = settings::Capturing { row, first: None };
                self.stale = true;

                return None;
            }
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
            ui::UiCmd::AddRom => AppCmd::SelectRom,
            ui::UiCmd::BrowseEnter(index) => return self.browse_enter(index),
            ui::UiCmd::BrowseChooseDir => {
                let dir = self.walk.as_ref()?.current_dir.clone();
                self.menu.close_browse();

                AppCmd::UseRomsDir(dir)
            }
            ui::UiCmd::SetRomCover(index) => AppCmd::SetRomCover(self.paths.get(index)?.clone()),
            ui::UiCmd::RemoveRomCover(index) => {
                AppCmd::RemoveRomCover(self.paths.get(index)?.clone())
            }
            // No cart index means the slot sheet asked, which is about the game
            // being played.
            ui::UiCmd::SetCoverFromState { rom, slot } => {
                let path = match rom {
                    Some(index) => self.paths.get(index)?,
                    None => self.loaded.as_ref()?,
                };

                AppCmd::SetCoverFromState(path.clone(), slot)
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

/// A few KB of PNG per cart, read while the shelf is built; most games have none.
fn cover_of(path: &Path) -> Option<ui::RgbImage> {
    let name = path.file_name()?.to_string_lossy();
    let cover = rom_cover::load(&name).ok()?;

    Some(ui::RgbImage {
        rgb: cover.rgb,
        width: cover.width as usize,
        height: cover.height as usize,
    })
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
