//! The egui frontend: the `ui` crate's screens over the paused game. This side
//! owns the platform half — building the view models from app state and turning
//! the UI's requests back into [`AppCmd`]s.

mod browse;
mod settings;
mod states;

use crate::cmd::{AppCmd, ChangeConfigCmd};
use crate::config::{AppConfig, LibraryLayout, LibrarySort};
use crate::frontend::{BrowseTarget, Capture, Frontend, FrontendCtx, NavAction, UiUpdate};
use crate::input::bindings::BindableInput;
use crate::library::cover;
use crate::library::meta::RomMeta;
use crate::library::RomsState;
use crate::storage::browser::FileBrowser;
use crate::video::AppVideo;
use crate::PlatformFileSystem;
use core::cart::header::CgbFlag;
use core::emu::state::SaveStateCmd;
use core::ppu::framebuffer::FrameBuffer;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

/// Cap on egui's own repaint delay, so input keeps being polled while it idles.
const MAX_FRAME_PERIOD: Duration = Duration::from_millis(30);

/// Floor under it. egui asks for an immediate repaint for as long as anything
/// animates and no backend enables vsync, so unpaced the menu loop spins as fast as
/// it can tessellate and present.
const MENU_FRAME_RATE: u64 = 60;
const MIN_FRAME_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / MENU_FRAME_RATE);

/// The brand logo the splash shows, built into the binary rather than read at runtime.
/// Rasterized from `media/logo.svg` with its plate rounding dropped: the splash sets it
/// in a rounded plate of its own, and the asset's corners would show inside that one.
const LOGO_PNG: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../media/logo.png"));

#[derive(Default)]
pub struct ModernFrontend {
    menu: ui::Menu,
    /// Held in one field of its own, not spread over the frontend: the screens are
    /// shown while the menu is borrowed mutably, and only separate fields make those
    /// two borrows disjoint.
    views: ViewData,
    paths: Vec<PathBuf>,
    /// The settings row waiting for an input to land on it, and how far a combo's
    /// pair has got.
    capturing: settings::Capturing,
    /// Slot whose screen was read out of its state file, to keep it off the disk
    /// while the sheet stays put.
    shot_slot: Option<usize>,
    /// Cart the cover states belong to, so they are read once per cart.
    cover_rom: Option<usize>,
    /// Path of the loaded game, for the commands that name no cart of their own.
    loaded: Option<PathBuf>,
    /// The storage walk, alive only while its screen is up.
    walk: Option<FileBrowser>,
    walk_target: BrowseTarget,
    /// Whether the splash has reached the screen. Until it has, the work a first frame
    /// would otherwise wait on is left undone: see [`Self::render`].
    splash_drawn: bool,
    /// Filled by pointer input during `render`, drained by the app afterwards.
    pending: VecDeque<AppCmd>,
    stale: Stale,
    /// Something the UI shows moved with no view behind it to rebuild — a nav, an
    /// input egui read itself, a notification. Cleared by the frame that draws it.
    unpainted: bool,
    /// When egui asked to be run again, `None` while it wants nothing: the animations
    /// and the delayed repaints it keeps time of on its own.
    repaint_at: Option<Instant>,
    /// What the shelf was built from last time, so a rebuild reads only what moved.
    cards: CardCache,
    frame_period: Duration,
}

/// Which views no longer match the app state under them.
#[derive(Default, Clone, Copy)]
struct Stale {
    library: bool,
    settings: bool,
    states: bool,
}

impl Stale {
    const ALL: Self = Self {
        library: true,
        settings: true,
        states: true,
    };

    /// A view still to be rebuilt is also a frame still to be drawn.
    fn any(self) -> bool {
        self.library || self.settings || self.states
    }
}

/// Everything the screens read, rebuilt by [`ModernFrontend::refresh`] and handed
/// over as [`ui::Views`].
#[derive(Default)]
struct ViewData {
    entries: Vec<ui::RomEntry>,
    /// Bumped when a shelf position takes a different cover, which is the UI's cue
    /// to drop the textures it uploaded from the old ones.
    library_version: u64,
    /// Rebuilt from the config whenever the app reports a change.
    settings: ui::SettingsView,
    /// Same, from the save-state files of the loaded game.
    states: ui::StatesView,
    /// The save states of the cart whose cover is being worked on; read only while
    /// one of those screens is open.
    rom_states: ui::StatesView,
    /// Where the storage walk is, empty unless one is open.
    browse: ui::BrowseView,
    /// What the loaded game is called, empty with none loaded.
    playing: String,
    /// The brand logo, decoded once: the splash needs pixels, and this is the only
    /// side with a decoder.
    logo: Option<ui::RgbImage>,
}

impl ViewData {
    /// The borrowed form the `ui` crate reads. `sort` and `layout` come from the
    /// config rather than from here: they are the app's settings, not views this side
    /// builds.
    fn ui(&self, sort: ui::SortBy, layout: ui::LibraryLayout) -> ui::Views<'_> {
        ui::Views {
            library: ui::LibraryView {
                entries: &self.entries,
                version: self.library_version,
                sort,
                layout,
            },
            playing: &self.playing,
            logo: self.logo.as_ref(),
            settings: &self.settings,
            states: &self.states,
            rom_states: &self.rom_states,
            browse: &self.browse,
        }
    }
}

impl Frontend for ModernFrontend {
    /// The library is left for the first refresh to build: it needs the platform's
    /// filesystem, which only a [`FrontendCtx`] carries.
    fn new(_roms: &RomsState) -> Self {
        Self {
            stale: Stale::ALL,
            ..Default::default()
        }
    }

    fn open_browse(&mut self, target: BrowseTarget, from: Option<&Path>) {
        // Where this session's walk stopped, else where the app remembers one
        // stopping — the same place the text menu picks up from.
        let last = self.walk.as_ref().map(|walk| walk.current_dir.clone());
        self.walk_target = target;
        self.walk = browse::start(&self.walk_target, last.as_deref().or(from));
        self.views.browse = browse::view(self.walk.as_ref(), &self.walk_target);
        self.unpainted = true;
        self.menu.open_browse();
    }

    fn nav<FS: PlatformFileSystem>(
        &mut self,
        action: NavAction,
        ctx: FrontendCtx<'_, FS>,
    ) -> Option<AppCmd> {
        self.refresh(&ctx);
        // Selection and focus live in the menu, which no staleness flag covers.
        self.unpainted = true;
        let views = self.views.ui(
            into_sort(ctx.config.library_sort),
            into_layout(ctx.config.library_layout),
        );
        let cmd = self.menu.nav(into_nav(action), &views)?;

        self.app_cmd(cmd, ctx.config)
    }

    fn capture_bind<I: BindableInput>(&mut self, input: I, pressed: bool) -> Capture {
        let Some(id) = self.capturing.row else {
            return Capture::Pass;
        };

        if pressed && input.is_cancel() {
            self.capturing = settings::Capturing::default();
            self.stale.settings = true;

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
        self.stale.settings = true;

        Capture::Took(settings::bind(id, input))
    }

    fn request_update(&mut self, what: UiUpdate) {
        // Even an update no view is built from still has to reach the screen.
        self.unpainted = true;

        match what {
            UiUpdate::Library => self.stale.library = true,
            UiUpdate::Settings => self.stale.settings = true,
            UiUpdate::States => self.stale.states = true,
            // Drawn into the framebuffer this side only uploads as a backdrop.
            UiUpdate::Overlay => {}
            UiUpdate::All => self.stale = Stale::ALL,
        }
    }

    fn request_render(&mut self) {
        self.unpainted = true;
    }

    fn needs_render(&self) -> bool {
        self.unpainted || self.stale.any() || self.repaint_at.is_some_and(|at| at <= Instant::now())
    }

    fn open(&mut self, has_game: bool) {
        self.unpainted = true;
        self.menu.open(has_game);
    }

    fn start(&mut self, has_game: bool) {
        self.unpainted = true;
        // Decoded for the splash this opens on, and dropped again by the frame that
        // follows it: no other screen shows the asset.
        self.views.logo = logo();
        self.menu.start(has_game);
    }

    fn take_cmd(&mut self) -> Option<AppCmd> {
        self.pending.pop_front()
    }

    /// The frame this builds is presented after it returns, so anything done here is time
    /// the window spends showing the frame before. The splash reads none of the views, so
    /// its first frame is built without them and the library is walked from the second on
    /// — behind the splash rather than in front of it, which is what a blank window
    /// waiting on a walk looks like.
    fn render<FS: PlatformFileSystem>(
        &mut self,
        video: &mut AppVideo,
        fb: &mut FrameBuffer,
        ctx: FrontendCtx<'_, FS>,
    ) {
        let splash = self.menu.on_splash();
        // Cleared before the frame is built, so whatever the frame itself moves —
        // pointer input reaching a view, a command it produces — stands for the next.
        self.unpainted = false;

        if self.splash_drawn || !splash {
            self.refresh(&ctx);
            self.refresh_shot(&ctx);
            self.refresh_cover_states();
        }

        self.splash_drawn |= splash;

        // Once it is off, the pixels behind it have been drawn for the last time.
        if !splash {
            self.views.logo = None;
        }

        video.draw_backdrop(fb);

        let views = self.views.ui(
            into_sort(ctx.config.library_sort),
            into_layout(ctx.config.library_layout),
        );
        let menu = &mut self.menu;
        let mut cmds = Vec::new();
        video.draw_ui(&mut |root| menu.show(root, &views, &mut cmds));

        for cmd in cmds {
            if let Some(cmd) = self.app_cmd(cmd, ctx.config) {
                self.pending.push_back(cmd);
            }
        }

        let delay = video.ui_frame_delay();
        self.frame_period = delay.clamp(MIN_FRAME_PERIOD, MAX_FRAME_PERIOD);
        // egui reports `Duration::MAX` while nothing animates, which no instant can
        // hold: nothing to come back for until something else moves the UI.
        self.repaint_at = Instant::now().checked_add(delay);
    }

    fn frame_period(&self) -> Duration {
        self.frame_period
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
                self.stale.settings = true;
            }

            return Capture::Took(None);
        }

        match self.capturing.first {
            None => {
                self.capturing.first = Some(code);
                self.stale.settings = true;

                Capture::Took(None)
            }
            // The pad repeating the button it is already holding is not a pair.
            Some(first) if first == code => Capture::Took(None),
            Some(first) => {
                self.capturing = settings::Capturing::default();
                self.stale.settings = true;

                Capture::Took(settings::bind_combo(id, first, code))
            }
        }
    }

    /// Every view is a read-only snapshot, so each is rebuilt only when the app says
    /// the state behind that one moved.
    fn refresh<FS: PlatformFileSystem>(&mut self, ctx: &FrontendCtx<'_, FS>) {
        if self.stale.library {
            self.stale.library = false;
            self.load_library(ctx);
            self.loaded = ctx.roms.last_path().cloned();
            // The shelf's own name for it, sidecar and all, so the pause overlay and
            // the cart agree on what the game is called.
            self.views.playing = self
                .loaded
                .as_deref()
                .map(|path| title_of(path, &rom_meta(path)))
                .unwrap_or_default();
            // A reshelved cart is at another index, which is what the cover states
            // were read by.
            self.cover_rom = None;
        }

        if self.stale.settings {
            self.stale.settings = false;
            self.views.settings = settings::view(ctx.config, ctx.palettes, self.capturing);
        }

        if self.stale.states {
            self.stale.states = false;
            self.views.states = states::view(ctx, self.views.states.version + 1);
            // The rebuilt view dropped the screen read into it.
            self.shot_slot = None;
        }
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
        let version = self.views.rom_states.version + 1;
        self.views.rom_states = open
            .and_then(|index| self.paths.get(index))
            .and_then(|path| path.file_name())
            .map(|name| states::choices_for(&name.to_string_lossy(), version))
            .unwrap_or_else(|| ui::StatesView {
                version,
                ..Default::default()
            });
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
        let Some(state) = self.views.states.slots.iter_mut().find(|s| s.slot == slot) else {
            return;
        };

        if state.shot.is_none() {
            state.shot = states::load_shot(ctx, slot);
        }
    }

    /// The shelf is what has been played plus whatever else is in the chosen ROMs
    /// directory: played first and most recent leading, then the rest by name, so
    /// the order is the same on every run — the scan itself is unordered. That is
    /// also [`LibrarySort::Recent`]; the other orders are a sort over the result.
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

        let shelved: Vec<PathBuf> = played
            .chain(unplayed)
            .filter(|path| match path.file_name() {
                Some(name) => seen.insert(name.to_owned()),
                None => false,
            })
            .collect();
        let mut cards = self.cards.cards_for(ctx.roms, shelved);
        sort_cards(&mut cards, ctx.config.library_sort);
        self.paths = cards.iter().map(|card| card.path.clone()).collect();
        let entries: Vec<ui::RomEntry> = cards.into_iter().map(|card| card.entry).collect();

        if covers_moved(&self.views.entries, &entries) {
            self.views.library_version += 1;
        }

        self.views.entries = entries;
    }

    /// A folder only moves the walk along; a file ends it, and what it ends as
    /// depends on what the walk was for.
    fn browse_enter(&mut self, index: usize) -> Option<AppCmd> {
        let walk = self.walk.as_mut()?;
        let picked = browse::enter(walk, index);
        self.views.browse = browse::view(self.walk.as_ref(), &self.walk_target);

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
                self.stale.settings = true;

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
            ui::UiCmd::AddRomsDir => AppCmd::SelectRomsDir,
            ui::UiCmd::SortLibrary(sort) => {
                AppCmd::ChangeConfig(ChangeConfigCmd::LibrarySort(from_sort(sort)))
            }
            ui::UiCmd::ToggleLibraryLayout => {
                AppCmd::ChangeConfig(ChangeConfigCmd::ToggleLibraryLayout)
            }
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

/// A cart on its way to the shelf: what the screen shows of it, and the keys the
/// shelf can be ordered by.
struct Card {
    path: PathBuf,
    entry: ui::RomEntry,
    /// Lowercased title, so ordering by name compares without allocating per pair.
    name_key: String,
    playtime_secs: u64,
}

fn card_of(roms: &RomsState, path: PathBuf, name: &str, cached: &CachedCard) -> Card {
    let title = title_of(&path, &cached.meta);
    let playtime_secs = roms.playtime(name);

    Card {
        name_key: title.to_lowercase(),
        entry: ui::RomEntry {
            title,
            kind: kind_of(cached.meta.cgb),
            cover: cached.cover.clone(),
            played: states::played(playtime_secs),
        },
        playtime_secs,
        path,
    }
}

/// The shelf as it was last built. Rebuilding it costs a sidecar read and a cover
/// decode per cart, none of which is worth repeating for a cart whose files have not
/// moved since.
#[derive(Default)]
struct CardCache {
    cards: HashMap<String, CachedCard>,
}

impl CardCache {
    /// Cards for the carts in `paths`, in that order. Whatever is no longer shelved
    /// is dropped along with the map it was cached in.
    fn cards_for(&mut self, roms: &RomsState, paths: Vec<PathBuf>) -> Vec<Card> {
        let mut kept = HashMap::with_capacity(paths.len());
        let cards = paths
            .into_iter()
            .map(|path| {
                let name = file_name(&path);
                let cached = match self.cards.remove(&name) {
                    Some(mut cached) => {
                        cached.reread_moved(&path, &name);

                        cached
                    }
                    None => CachedCard::read(&path, &name),
                };
                let card = card_of(roms, path, &name, &cached);
                kept.insert(name, cached);

                card
            })
            .collect();
        self.cards = kept;

        cards
    }
}

/// One cart's parts, each with the state of the files it was read from.
#[derive(Default)]
struct CachedCard {
    meta: RomMeta,
    /// The sidecar the metadata came out of, and the ROM it is checked against:
    /// [`RomMeta::load_or_create`] reads the header again once the two disagree.
    meta_at: (FileStamp, FileStamp),
    cover: Option<Arc<ui::RgbImage>>,
    cover_at: FileStamp,
}

impl CachedCard {
    fn read(path: &Path, name: &str) -> Self {
        let mut card = Self::default();
        card.read_meta(path, name);
        card.read_cover(name);

        card
    }

    fn reread_moved(&mut self, path: &Path, name: &str) {
        if self.meta_at != meta_stamp(path, name) {
            self.read_meta(path, name);
        }

        if self.cover_at != FileStamp::of(&cover::path(name)) {
            self.read_cover(name);
        }
    }

    fn read_meta(&mut self, path: &Path, name: &str) {
        self.meta = RomMeta::load_or_create(path, name);
        // Stamped after the read: a ROM seen for the first time has its sidecar
        // written, which would otherwise make the stamp stale the moment it was taken.
        self.meta_at = meta_stamp(path, name);
    }

    fn read_cover(&mut self, name: &str) {
        self.cover_at = FileStamp::of(&cover::path(name));
        self.cover = cover_of(name).map(Arc::new);
    }
}

fn meta_stamp(path: &Path, name: &str) -> (FileStamp, FileStamp) {
    (FileStamp::of(&RomMeta::path(name)), FileStamp::of(path))
}

/// Enough of a file to notice it being replaced. Defaults to what a file that is not
/// there stamps as — most carts have no cover.
#[derive(Default, Eq, PartialEq)]
struct FileStamp {
    modified: Option<SystemTime>,
    len: u64,
}

impl FileStamp {
    fn of(path: &Path) -> Self {
        let Ok(meta) = fs::metadata(path) else {
            return Self::default();
        };

        Self {
            modified: meta.modified().ok(),
            len: meta.len(),
        }
    }
}

/// Whether any shelf position now holds a different cover, which is what the UI keys
/// its uploaded textures by.
fn covers_moved(old: &[ui::RomEntry], new: &[ui::RomEntry]) -> bool {
    old.len() != new.len()
        || old
            .iter()
            .zip(new)
            .any(|(old, new)| match (&old.cover, &new.cover) {
                (Some(old), Some(new)) => !Arc::ptr_eq(old, new),
                (None, None) => false,
                _ => true,
            })
}

/// What every sidecar of a cart goes by.
fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// A stable sort, so cards a key cannot tell apart keep the order the merge put
/// them in — which is itself the same on every run.
fn sort_cards(cards: &mut [Card], by: LibrarySort) {
    match by {
        // Exactly what the merge produced.
        LibrarySort::Recent => {}
        LibrarySort::Name => cards.sort_by(|a, b| a.name_key.cmp(&b.name_key)),
        // Longest first, with the never-played tail alphabetical rather than left
        // in play order.
        LibrarySort::Playtime => cards.sort_by(|a, b| {
            b.playtime_secs
                .cmp(&a.playtime_secs)
                .then_with(|| a.name_key.cmp(&b.name_key))
        }),
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

/// The logo as pixels. A build whose asset failed to decode still runs — the splash
/// falls back to the name in type.
fn logo() -> Option<ui::RgbImage> {
    let logo = image::load_from_memory(LOGO_PNG)
        .inspect_err(|err| log::warn!("Failed to decode the logo: {err}"))
        .ok()?
        .to_rgb8();

    Some(ui::RgbImage {
        width: logo.width() as usize,
        height: logo.height() as usize,
        rgb: logo.into_raw(),
    })
}

/// A few KB of PNG per cart, read while the shelf is built; most games have none.
fn cover_of(name: &str) -> Option<ui::RgbImage> {
    let cover = cover::load(name).ok()?;

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

/// The shelf's order crosses the seam both ways: out, so the sheet opens on the
/// order in force, and back in, as what the config is set to.
fn into_sort(sort: LibrarySort) -> ui::SortBy {
    match sort {
        LibrarySort::Recent => ui::SortBy::Recent,
        LibrarySort::Name => ui::SortBy::Name,
        LibrarySort::Playtime => ui::SortBy::Playtime,
    }
}

/// Crosses the seam one way only: the screen needs the layout in force to draw it and
/// to offer the other one, and asks for the switch as a flip this side carries out.
fn into_layout(layout: LibraryLayout) -> ui::LibraryLayout {
    match layout {
        LibraryLayout::Shelf => ui::LibraryLayout::Shelf,
        LibraryLayout::List => ui::LibraryLayout::List,
    }
}

fn from_sort(sort: ui::SortBy) -> LibrarySort {
    match sort {
        ui::SortBy::Recent => LibrarySort::Recent,
        ui::SortBy::Name => LibrarySort::Name,
        ui::SortBy::Playtime => LibrarySort::Playtime,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cover() -> Arc<ui::RgbImage> {
        Arc::new(ui::RgbImage {
            rgb: vec![0; 3],
            width: 1,
            height: 1,
        })
    }

    fn entry(cover: Option<Arc<ui::RgbImage>>) -> ui::RomEntry {
        ui::RomEntry {
            title: String::new(),
            kind: ui::CartKind::Dmg,
            cover,
            played: String::new(),
        }
    }

    /// The trap of skipping idle frames: an overlay line rebuilds no view, and asking
    /// the views alone would leave it off the screen for as long as the menu sits still.
    #[test]
    fn an_update_no_view_is_built_from_still_asks_for_a_frame() {
        let mut frontend = ModernFrontend::default();
        assert!(!frontend.needs_render());

        frontend.request_update(UiUpdate::Overlay);
        assert!(frontend.needs_render());
    }

    /// The cache hands a rebuild the pixels it already had, which is what lets the
    /// uploaded textures stand.
    #[test]
    fn a_shelf_rebuilt_from_the_same_covers_keeps_its_textures() {
        let kept = cover();

        assert!(!covers_moved(
            &[entry(Some(kept.clone())), entry(None)],
            &[entry(Some(kept)), entry(None)]
        ));
    }

    #[test]
    fn a_position_taking_another_cover_drops_them() {
        assert!(covers_moved(
            &[entry(Some(cover()))],
            &[entry(Some(cover()))]
        ));
        assert!(covers_moved(&[entry(Some(cover()))], &[entry(None)]));
        assert!(covers_moved(&[entry(None)], &[entry(Some(cover()))]));
    }

    /// Positions are the texture keys, so a shelf of another length has none of them.
    #[test]
    fn a_cart_arriving_or_leaving_drops_them() {
        assert!(covers_moved(&[entry(None)], &[entry(None), entry(None)]));
        assert!(covers_moved(&[entry(None), entry(None)], &[entry(None)]));
    }
}
