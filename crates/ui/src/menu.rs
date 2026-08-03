//! Which screen the UI is on and what it asks the platform to do.
//!
//! The screens are the UI's own business — the app only knows it is paused — so
//! moving between them never leaves this crate. Everything the platform has to
//! act on comes back as a [`UiCmd`].

use crate::browse::BrowseView;
use crate::browse::{self, BrowsePick};
use crate::cover::{self, CoverAction, CoverOffer};
use crate::library::{
    self, library, LibraryEvent, LibraryFocus, LibraryPick, LibraryView, SortBy,
};
use crate::nav::{FocusEvent, GridFocus, NavAction};
use crate::overlay;
use crate::rename::{self, RenameEdit, RenameEvent};
use crate::settings::{row_at, settings, Control, PageId, SettingId, SettingsView, ROOT_PAGE};
use crate::states::{self, RowPick, StatesView};
use crate::theme::WIDTH_SHEET;
use egui::Vec2;

/// A request only the platform can carry out. Not `Copy`: a rename carries the
/// name the user typed.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum UiCmd {
    LaunchRom(usize),
    Resume,
    /// Write the running game's state into this slot.
    SaveState(usize),
    /// Restore the state kept in this slot.
    LoadState(usize),
    /// Throw away the state kept in this slot.
    DeleteState(usize),
    /// Call this slot's state something; an empty name clears it.
    RenameState(usize, String),
    /// Call this cartridge something; an empty name goes back to its file name.
    RenameRom(usize, String),
    /// Ask for a game to be picked off the disk and added to the shelf.
    AddRom,
    /// Ask for a folder to be picked; its games are what the shelf lists from then
    /// on, on top of everything already played.
    AddRomsDir,
    /// Read the shelf in this order from now on.
    SortLibrary(SortBy),
    /// Walk into what the browser's row `0`-based index holds: a folder to open, or
    /// a game to take.
    BrowseEnter(usize),
    /// Take the folder the browser is standing in.
    BrowseChooseDir,
    /// Ask for a cover picture for this cartridge.
    SetRomCover(usize),
    /// Take this cartridge's cover away.
    RemoveRomCover(usize),
    /// Make a state's screen a game's cover: the cart by shelf index, or the
    /// loaded game when there is no index to give — the slot sheet has none.
    SetCoverFromState {
        rom: Option<usize>,
        slot: usize,
    },
    RestartRom,
    /// Move a settings row by `step` (`-1`/`1`); toggles and actions ignore it.
    Setting {
        id: SettingId,
        step: i8,
    },
    Quit,
}

/// What every screen reads, gathered up so a new screen doesn't have to thread
/// another argument through both entry points.
pub struct Views<'a> {
    pub library: LibraryView<'a>,
    pub settings: &'a SettingsView,
    pub states: &'a StatesView,
    /// Where the storage walk is, empty unless one is open.
    pub browse: &'a BrowseView,
    /// The save states of the cart whose cover is being worked on, empty the rest
    /// of the time: reading them costs a stat per slot, so the platform only does
    /// it once a cover screen is open.
    pub rom_states: &'a StatesView,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
enum Screen {
    #[default]
    Library,
    /// Over the dimmed game, when the UI is opened mid-play.
    Pause,
    /// One page of the settings; the rest are reached from its rows.
    Settings(PageId),
    /// The game's save-state slots.
    States,
    /// What one slot, picked from the list, can be used for.
    StateActions(usize),
    /// Naming the state in one slot.
    StateRename(usize),
    /// The ways of putting games on the shelf, which is what the plus opens.
    AddRom,
    /// The orders the shelf can be read in.
    Sort,
    /// What can be done with one cartridge of the library, besides playing it.
    RomActions(usize),
    /// Naming one cartridge.
    RomRename(usize),
    /// What can be done with one cartridge's cover.
    RomCover(usize),
    /// Which of the game's states to take a cover from.
    CoverFromState(usize),
    /// Walking storage, because this platform's own picker cannot be reached the
    /// way the app is driven.
    Browse,
}

/// What picking a pause row does: hand the platform a command, or move on to the
/// screen behind the row.
#[derive(Clone)]
enum PauseAction {
    Cmd(UiCmd),
    Open(Screen),
}

/// What the shelf calls the cart at `index`, which is also what renaming starts
/// from; empty when the shelf has no such cart.
fn rom_title<'a>(views: &'a Views<'_>, index: usize) -> &'a str {
    views
        .library
        .entries
        .get(index)
        .map_or("", |entry| entry.title.as_str())
}

/// A row of the pause overlay: what it says and what it does.
const PAUSE_ITEMS: [(&str, PauseAction); 6] = [
    ("Resume", PauseAction::Cmd(UiCmd::Resume)),
    ("Save states", PauseAction::Open(Screen::States)),
    ("Restart", PauseAction::Cmd(UiCmd::RestartRom)),
    ("Library", PauseAction::Open(Screen::Library)),
    ("Settings", PauseAction::Open(Screen::Settings(ROOT_PAGE))),
    ("Quit", PauseAction::Cmd(UiCmd::Quit)),
];

#[derive(Default)]
pub struct Menu {
    screen: Screen,
    /// Where Back returns to, so settings can be reached from either screen.
    opener: Screen,
    library: LibraryFocus,
    pause: GridFocus,
    settings: GridFocus,
    /// The plus's sheet, whose rows are neither the cart's nor the pause screen's.
    add: GridFocus,
    sort: GridFocus,
    /// The settings pages walked through to reach the one on screen, each with the
    /// row it was left on, so backing out lands where it was opened from. Always
    /// empty outside the settings: the last Back is what leaves them.
    settings_trail: Vec<(PageId, GridFocus)>,
    states: GridFocus,
    actions: GridFocus,
    /// The cart action sheet, which is a different list from the slot one.
    rom_actions: GridFocus,
    cover_actions: GridFocus,
    /// The slot picker for a cover keeps its own focus and textures: it lists
    /// another game's slots than the pause screens do, and one cache keyed by slot
    /// number would hand out the wrong picture.
    cover_states: GridFocus,
    browse: GridFocus,
    /// A link Confirm landed on, waiting for a frame to be opened from: `nav` runs
    /// with no egui context, and the request has to be made through one.
    open_url: Option<String>,
    cover_shots: crate::image::TextureCache,
    shots: crate::image::TextureCache,
    /// Covers uploaded for the shelf, which is a different set from the shots.
    covers: crate::image::TextureCache,
    rename: RenameEdit,
}

impl Menu {
    /// Entering with a game loaded pauses over it; otherwise the library is home.
    pub fn open(&mut self, has_game: bool) {
        let screen = if has_game {
            Screen::Pause
        } else {
            Screen::Library
        };

        self.enter(screen, has_game);
    }

    /// A session opens on the shelf, with the loaded game one Back away.
    pub fn start(&mut self, has_game: bool) {
        self.enter(Screen::Library, has_game);
    }

    fn enter(&mut self, screen: Screen, has_game: bool) {
        self.screen = screen;
        // A game to go back to is what makes the overlay reachable.
        self.opener = if has_game {
            Screen::Pause
        } else {
            Screen::Library
        };

        // Closing the UI from a settings page leaves a trail nothing will pop, since
        // the settings open at their first page again.
        if !self.settings_trail.is_empty() {
            self.settings_trail.clear();
            self.settings = GridFocus::default();
        }
    }

    /// The slot the UI is working on, so the platform knows which screen to read.
    /// Renaming counts, so a detour through it doesn't throw the screen away.
    pub fn open_slot(&self) -> Option<usize> {
        match self.screen {
            Screen::StateActions(slot) | Screen::StateRename(slot) => Some(slot),
            _ => None,
        }
    }

    /// Shows the storage walk. Only the platform can decide it is needed: it is the
    /// one that knows whether this device has a picker of its own.
    pub fn open_browse(&mut self) {
        self.browse = GridFocus::default();
        self.screen = Screen::Browse;
    }

    /// Called when the walk is over — a game taken, or a folder chosen.
    pub fn close_browse(&mut self) {
        if self.screen == Screen::Browse {
            self.screen = Screen::Library;
        }
    }

    /// Whether the walk is what is on screen, so the platform knows to keep its
    /// listing up to date.
    pub fn browsing(&self) -> bool {
        self.screen == Screen::Browse
    }

    /// The cart whose cover is being worked on, so the platform knows whose save
    /// states to look up.
    pub fn open_cover_rom(&self) -> Option<usize> {
        match self.screen {
            Screen::RomCover(index) | Screen::CoverFromState(index) => Some(index),
            _ => None,
        }
    }

    pub fn nav(&mut self, action: NavAction, views: &Views<'_>) -> Option<UiCmd> {
        match self.screen {
            Screen::Library if action == NavAction::Options => {
                self.open_rom_actions();

                None
            }
            Screen::Library => match self.library.nav(action)? {
                LibraryPick::Rom(index) => Some(UiCmd::LaunchRom(index)),
                LibraryPick::Header(event) => self.library_event(event, views),
                // Backing out of the library only makes sense with a game to
                // return to, and then the overlay is where we came from.
                LibraryPick::Back => (self.opener == Screen::Pause).then(|| {
                    self.screen = Screen::Pause;
                    UiCmd::Resume
                }),
            },
            Screen::Pause => match self.pause.nav(action)? {
                FocusEvent::Activate(index) => self.activate_pause(index),
                FocusEvent::Back => Some(UiCmd::Resume),
            },
            Screen::Settings(page) => self.nav_settings(action, page, views.settings),
            Screen::States => match self.states.nav(action)? {
                FocusEvent::Activate(index) => {
                    let pick = states::pick(views.states, index)?;

                    self.pick_state(pick)
                }
                FocusEvent::Back => {
                    self.screen = Screen::Pause;
                    None
                }
            },
            Screen::StateActions(slot) => match self.actions.nav(action)? {
                FocusEvent::Activate(index) => {
                    let action = states::action_at(index)?;

                    self.act_on_slot(action, slot, views)
                }
                FocusEvent::Back => {
                    self.screen = Screen::States;
                    None
                }
            },
            // Typing belongs to the text field, which egui drives itself off the
            // keyboard; the Save and Cancel buttons are here so a gamepad — whose
            // events never reach egui — can still finish or drop the rename.
            Screen::StateRename(slot) => {
                let event = self.rename.nav(action)?;

                self.finish_rename(slot, event)
            }
            Screen::AddRom => match self.add.nav(action)? {
                FocusEvent::Activate(row) => {
                    let action = library::add_at(row)?;

                    Some(self.act_on_add(action))
                }
                FocusEvent::Back => {
                    self.screen = Screen::Library;
                    None
                }
            },
            Screen::Sort => match self.sort.nav(action)? {
                FocusEvent::Activate(row) => {
                    let sort = library::sort_at(row)?;

                    Some(self.act_on_sort(sort))
                }
                FocusEvent::Back => {
                    self.screen = Screen::Library;
                    None
                }
            },
            Screen::RomActions(index) => match self.rom_actions.nav(action)? {
                FocusEvent::Activate(row) => {
                    let action = library::action_at(row)?;

                    self.act_on_rom(action, index, views)
                }
                FocusEvent::Back => {
                    self.screen = Screen::Library;
                    None
                }
            },
            Screen::RomRename(index) => {
                let event = self.rename.nav(action)?;

                self.finish_rom_rename(index, event)
            }
            Screen::RomCover(index) => match self.cover_actions.nav(action)? {
                FocusEvent::Activate(row) => {
                    let action = cover::action_at(self.cover_offer(views, index), row)?;

                    self.act_on_cover(action, index)
                }
                FocusEvent::Back => {
                    self.screen = Screen::RomActions(index);
                    None
                }
            },
            Screen::Browse => match self.browse.nav(action)? {
                FocusEvent::Activate(row) => {
                    let pick = browse::pick(views.browse, row)?;

                    Some(match pick {
                        BrowsePick::Enter(index) => UiCmd::BrowseEnter(index),
                        BrowsePick::ChooseDir => UiCmd::BrowseChooseDir,
                    })
                }
                // Backing out leaves the walk; the way up is a row of its own.
                FocusEvent::Back => {
                    self.screen = Screen::Library;
                    None
                }
            },
            Screen::CoverFromState(index) => match self.cover_states.nav(action)? {
                FocusEvent::Activate(row) => {
                    let pick = states::pick(views.rom_states, row)?;

                    self.pick_cover_state(index, pick)
                }
                FocusEvent::Back => {
                    self.screen = Screen::RomCover(index);
                    None
                }
            },
        }
    }

    /// What the cart at `index` makes possible: a cover it already has can be taken
    /// away, and a state it has can be taken a screen from.
    fn cover_offer(&self, views: &Views<'_>, index: usize) -> CoverOffer {
        CoverOffer {
            has_cover: views
                .library
                .entries
                .get(index)
                .is_some_and(|entry| entry.cover.is_some()),
            has_states: !views.rom_states.slots.is_empty(),
        }
    }

    fn act_on_cover(&mut self, action: CoverAction, index: usize) -> Option<UiCmd> {
        match action {
            CoverAction::UseState => {
                self.cover_states = GridFocus::default();
                self.screen = Screen::CoverFromState(index);

                None
            }
            // Both are done with the screen: the dialog takes over for one, and the
            // other is over as soon as it is asked for.
            CoverAction::UseFile => {
                self.screen = Screen::Library;

                Some(UiCmd::SetRomCover(index))
            }
            CoverAction::Remove => {
                self.screen = Screen::Library;

                Some(UiCmd::RemoveRomCover(index))
            }
        }
    }

    fn pick_cover_state(&mut self, index: usize, pick: RowPick) -> Option<UiCmd> {
        match pick {
            RowPick::Open(slot) => {
                self.screen = Screen::Library;

                Some(UiCmd::SetCoverFromState {
                    rom: Some(index),
                    slot,
                })
            }
            // The platform leaves no free slot in a picker's view, since writing a
            // state is not one of the things being picked between.
            RowPick::Create(_) => None,
        }
    }

    /// The focused cart's own sheet. Nothing to open one for when the focus is on
    /// the header, or the shelf is empty.
    fn open_rom_actions(&mut self) {
        let Some(index) = self.library.rom() else {
            return;
        };

        self.rom_actions = GridFocus::default();
        self.screen = Screen::RomActions(index);
    }

    /// What the header asks for, however it was asked — a press or a click.
    fn library_event(&mut self, event: LibraryEvent, views: &Views<'_>) -> Option<UiCmd> {
        match event {
            LibraryEvent::OpenSettings => {
                self.screen = Screen::Settings(ROOT_PAGE);

                None
            }
            LibraryEvent::Add => {
                self.add = GridFocus::default();
                self.screen = Screen::AddRom;

                None
            }
            // Opened on the order the shelf is already in, so the sheet says which
            // one that is without a mark of its own.
            LibraryEvent::Sort => {
                self.sort = GridFocus::default();
                self.sort.sync(library::sort_count(), 1);
                self.sort.focus(library::sort_row(views.library.sort));
                self.screen = Screen::Sort;

                None
            }
        }
    }

    fn act_on_sort(&mut self, sort: SortBy) -> UiCmd {
        self.screen = Screen::Library;

        UiCmd::SortLibrary(sort)
    }

    /// Either row is done with the sheet: the platform takes over from here, with a
    /// picker of its own or the storage walk.
    fn act_on_add(&mut self, action: library::AddAction) -> UiCmd {
        self.screen = Screen::Library;

        match action {
            library::AddAction::OpenRom => UiCmd::AddRom,
            library::AddAction::ScanDir => UiCmd::AddRomsDir,
        }
    }

    fn act_on_rom(
        &mut self,
        action: library::RomAction,
        index: usize,
        views: &Views<'_>,
    ) -> Option<UiCmd> {
        match action {
            library::RomAction::Rename => {
                self.rename.start(rom_title(views, index));
                self.screen = Screen::RomRename(index);

                None
            }
            library::RomAction::Cover => {
                self.cover_actions = GridFocus::default();
                self.screen = Screen::RomCover(index);

                None
            }
        }
    }

    fn finish_rom_rename(&mut self, index: usize, event: RenameEvent) -> Option<UiCmd> {
        match event {
            RenameEvent::Commit => {
                self.screen = Screen::Library;

                Some(UiCmd::RenameRom(index, self.rename.take_text()))
            }
            RenameEvent::Cancel => {
                self.screen = Screen::RomActions(index);

                None
            }
        }
    }

    /// An empty slot has one use, so it is written at once; a slot with a state
    /// in it gets the sheet.
    fn pick_state(&mut self, pick: RowPick) -> Option<UiCmd> {
        match pick {
            RowPick::Create(slot) => Some(UiCmd::SaveState(slot)),
            RowPick::Open(slot) => {
                self.screen = Screen::StateActions(slot);
                self.actions = GridFocus::default();

                None
            }
        }
    }

    /// Renaming moves on to its own screen; every other action is done with the
    /// sheet and hands the platform its command.
    fn act_on_slot(
        &mut self,
        action: states::SlotAction,
        slot: usize,
        views: &Views<'_>,
    ) -> Option<UiCmd> {
        if action == states::SlotAction::Rename {
            self.rename.start(states::slot_name(views.states, slot));
            self.screen = Screen::StateRename(slot);

            return None;
        }

        self.screen = Screen::States;

        states::action_cmd(action, slot)
    }

    /// Saving lands back in the list, where the new name shows; cancelling goes
    /// back to the sheet the rename was started from.
    fn finish_rename(&mut self, slot: usize, event: RenameEvent) -> Option<UiCmd> {
        match event {
            RenameEvent::Commit => {
                self.screen = Screen::States;

                Some(UiCmd::RenameState(slot, self.rename.take_text()))
            }
            RenameEvent::Cancel => {
                self.screen = Screen::StateActions(slot);

                None
            }
        }
    }

    /// Left/Right step the focused row's value instead of moving the highlight.
    fn nav_settings(
        &mut self,
        action: NavAction,
        page: PageId,
        view: &SettingsView,
    ) -> Option<UiCmd> {
        let step = match action {
            NavAction::Left => -1,
            NavAction::Right => 1,
            _ => 0,
        };

        if step != 0 {
            let row = row_at(view, page, self.settings.index())?;

            // A row leading somewhere has no value to step, and a row that only shows
            // something has none to change.
            if matches!(
                row.control,
                Control::Page(_) | Control::Text(_) | Control::Link { .. }
            ) {
                return None;
            }

            return Some(UiCmd::Setting { id: row.id, step });
        }

        match self.settings.nav(action)? {
            FocusEvent::Activate(index) => {
                let row = row_at(view, page, index)?;

                match &row.control {
                    Control::Page(next) => {
                        self.open_settings_page(page, *next);

                        return None;
                    }
                    // Nothing to open it with until a frame is being drawn.
                    Control::Link { url, .. } => {
                        self.open_url = Some(url.clone());

                        return None;
                    }
                    Control::Text(_) => return None,
                    _ => {}
                }

                Some(UiCmd::Setting {
                    id: row.id,
                    step: 1,
                })
            }
            FocusEvent::Back => {
                self.leave_settings_page();
                None
            }
        }
    }

    fn open_settings_page(&mut self, from: PageId, to: PageId) {
        self.settings_trail
            .push((from, std::mem::take(&mut self.settings)));
        self.screen = Screen::Settings(to);
    }

    /// Back to the page this one was opened from, or out of the settings when it
    /// was opened from another screen.
    fn leave_settings_page(&mut self) {
        match self.settings_trail.pop() {
            Some((page, focus)) => {
                self.settings = focus;
                self.screen = Screen::Settings(page);
            }
            None => self.screen = self.opener,
        }
    }

    pub fn show(&mut self, root: &mut egui::Ui, views: &Views<'_>, out: &mut Vec<UiCmd>) {
        match self.screen {
            Screen::Library => {
                let covers = &mut self.covers;
                let asked = library(root, &views.library, &mut self.library, covers, out);

                if let Some(event) = asked {
                    out.extend(self.library_event(event, views));
                }
            }
            Screen::Pause => self.pause_overlay(root, out),
            Screen::Settings(page) => {
                if let Some(url) = self.open_url.take() {
                    root.ctx().open_url(egui::OpenUrl::new_tab(url));
                }

                let opened = settings(root, views.settings, page, &mut self.settings, out);

                if let Some(next) = opened {
                    self.open_settings_page(page, next);
                }
            }
            Screen::States => {
                let picked = states::show(root, views.states, &mut self.states, &mut self.shots);

                if let Some(pick) = picked {
                    out.extend(self.pick_state(pick));
                }
            }
            Screen::StateActions(slot) => {
                let picked = states::show_actions(
                    root,
                    views.states,
                    slot,
                    &mut self.actions,
                    &mut self.shots,
                );

                if let Some(action) = picked {
                    out.extend(self.act_on_slot(action, slot, views));
                }
            }
            Screen::StateRename(slot) => {
                let title = format!("Name slot {slot}");
                let event = rename::show(
                    root,
                    &title,
                    "Leave empty to go by number",
                    &mut self.rename,
                );

                if let Some(event) = event {
                    out.extend(self.finish_rename(slot, event));
                }
            }
            Screen::AddRom => {
                if let Some(action) = library::show_add(root, &mut self.add) {
                    out.push(self.act_on_add(action));
                }
            }
            Screen::Sort => {
                if let Some(sort) = library::show_sort(root, &mut self.sort) {
                    out.push(self.act_on_sort(sort));
                }
            }
            Screen::RomActions(index) => {
                let picked =
                    library::show_actions(root, rom_title(views, index), &mut self.rom_actions);

                if let Some(action) = picked {
                    out.extend(self.act_on_rom(action, index, views));
                }
            }
            Screen::RomCover(index) => {
                let offer = self.cover_offer(views, index);
                let picked = cover::show_actions(
                    root,
                    rom_title(views, index),
                    offer,
                    &mut self.cover_actions,
                );

                if let Some(action) = picked {
                    out.extend(self.act_on_cover(action, index));
                }
            }
            Screen::Browse => {
                if let Some(pick) = browse::show(root, views.browse, &mut self.browse) {
                    out.push(match pick {
                        BrowsePick::Enter(index) => UiCmd::BrowseEnter(index),
                        BrowsePick::ChooseDir => UiCmd::BrowseChooseDir,
                    });
                }
            }
            Screen::CoverFromState(index) => {
                let picked = states::show(
                    root,
                    views.rom_states,
                    &mut self.cover_states,
                    &mut self.cover_shots,
                );

                if let Some(pick) = picked {
                    out.extend(self.pick_cover_state(index, pick));
                }
            }
            Screen::RomRename(index) => {
                let event = rename::show(
                    root,
                    "Name cartridge",
                    "Leave empty to use the file name",
                    &mut self.rename,
                );

                if let Some(event) = event {
                    out.extend(self.finish_rom_rename(index, event));
                }
            }
        }
    }

    fn activate_pause(&mut self, index: usize) -> Option<UiCmd> {
        match &PAUSE_ITEMS.get(index)?.1 {
            PauseAction::Cmd(cmd) => return Some(cmd.clone()),
            PauseAction::Open(screen) => {
                self.screen = *screen;

                // The slot list is about what is on disk now, so it opens at the
                // top rather than wherever it was left.
                if *screen == Screen::States {
                    self.states = GridFocus::default();
                }
            }
        }

        None
    }

    #[cfg(test)]
    fn on_settings_page(&self) -> Option<PageId> {
        match self.screen {
            Screen::Settings(page) => Some(page),
            _ => None,
        }
    }

    /// How many pages deep into the settings the screen is.
    #[cfg(test)]
    fn settings_depth(&self) -> usize {
        self.settings_trail.len()
    }

    fn pause_overlay(&mut self, root: &mut egui::Ui, out: &mut Vec<UiCmd>) {
        self.pause.sync(PAUSE_ITEMS.len(), 1);
        let size = Vec2::new(WIDTH_SHEET, overlay::rows_height(PAUSE_ITEMS.len()));
        let pause = &mut self.pause;
        let mut clicked = None;

        overlay::popup(root, size, |ui| {
            clicked = overlay::rows(ui, PAUSE_ITEMS.iter().map(|(label, _)| *label), pause);
        });

        if let Some(index) = clicked {
            out.extend(self.activate_pause(index));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Page, Row, Section};

    const CHILD_PAGE: PageId = 1;
    /// What the child page's only row asks the platform for.
    const CHILD_ROW: SettingId = 7;

    /// A root whose one row leads to a page with one row of its own.
    fn nested() -> SettingsView {
        SettingsView {
            pages: vec![
                Page {
                    title: "Settings".to_owned(),
                    sections: vec![Section {
                        title: "Input".to_owned(),
                        rows: vec![Row {
                            id: 0,
                            label: "Keyboard".to_owned(),
                            control: Control::Page(CHILD_PAGE),
                        }],
                    }],
                },
                Page {
                    title: "Keyboard".to_owned(),
                    sections: vec![Section {
                        title: "Buttons".to_owned(),
                        rows: vec![Row {
                            id: CHILD_ROW,
                            label: "Up".to_owned(),
                            control: Control::Binding {
                                current: "W".to_owned(),
                                capturing: false,
                            },
                        }],
                    }],
                },
            ],
        }
    }

    fn views(settings: &SettingsView) -> Views<'_> {
        static EMPTY_STATES: std::sync::LazyLock<StatesView> =
            std::sync::LazyLock::new(StatesView::default);
        static EMPTY_BROWSE: std::sync::LazyLock<BrowseView> =
            std::sync::LazyLock::new(BrowseView::default);

        Views {
            library: LibraryView {
                entries: &[],
                version: 0,
                sort: SortBy::default(),
            },
            settings,
            states: &EMPTY_STATES,
            rom_states: &EMPTY_STATES,
            browse: &EMPTY_BROWSE,
        }
    }

    /// Opens the settings the way the pause overlay does, from a fresh menu whose
    /// overlay focus is still on the first row.
    fn on_settings(view: &SettingsView) -> Menu {
        let mut menu = Menu::default();
        menu.open(true);
        // A screen's row count reaches its focus as the screen is drawn, and these
        // tests draw nothing.
        menu.pause.sync(PAUSE_ITEMS.len(), 1);
        let settings = PAUSE_ITEMS
            .iter()
            .position(|(label, _)| *label == "Settings")
            .expect("the overlay offers the settings");

        for _ in 0..settings {
            menu.nav(NavAction::Down, &views(view));
        }

        menu.nav(NavAction::Confirm, &views(view));

        menu
    }

    /// The same for whichever settings page is up.
    fn sync_page(menu: &mut Menu, view: &SettingsView) {
        let page = menu.on_settings_page().expect("a settings page is up");
        menu.settings
            .sync(crate::settings::row_count(view, page), 1);
    }

    #[test]
    fn a_row_leads_to_its_page_and_back_to_the_row() {
        let view = nested();
        let mut menu = on_settings(&view);
        assert_eq!(menu.on_settings_page(), Some(ROOT_PAGE));
        sync_page(&mut menu, &view);

        assert_eq!(menu.nav(NavAction::Confirm, &views(&view)), None);
        assert_eq!(menu.on_settings_page(), Some(CHILD_PAGE));
        sync_page(&mut menu, &view);

        // The rows being read are the open page's, not the root's.
        assert_eq!(
            menu.nav(NavAction::Confirm, &views(&view)),
            Some(UiCmd::Setting {
                id: CHILD_ROW,
                step: 1,
            })
        );

        assert_eq!(menu.nav(NavAction::Back, &views(&view)), None);
        assert_eq!(menu.on_settings_page(), Some(ROOT_PAGE));
    }

    /// Only the last Back leaves; the ones before it walk back up the pages.
    #[test]
    fn the_settings_are_left_from_their_first_page() {
        let view = nested();
        let mut menu = on_settings(&view);
        sync_page(&mut menu, &view);
        menu.nav(NavAction::Confirm, &views(&view));
        menu.nav(NavAction::Back, &views(&view));

        assert_eq!(menu.nav(NavAction::Back, &views(&view)), None);
        assert_eq!(menu.on_settings_page(), None);
        // Back out of the overlay the settings were opened from.
        assert_eq!(
            menu.nav(NavAction::Back, &views(&view)),
            Some(UiCmd::Resume)
        );
    }

    /// The remembered game is loaded at startup, but nothing has been played yet, so
    /// the shelf comes up rather than a pause over it — and Back resumes.
    #[test]
    fn a_session_starts_on_the_shelf_over_the_loaded_game() {
        let view = nested();
        let mut menu = Menu::default();
        menu.start(true);
        menu.library.sync(0, 1);

        assert_eq!(menu.screen, Screen::Library);
        assert_eq!(
            menu.nav(NavAction::Back, &views(&view)),
            Some(UiCmd::Resume)
        );
    }

    /// Opens the library with an empty shelf, which leaves the focus on the plus.
    /// A screen's shape reaches its focus as it is drawn, and these tests draw
    /// nothing.
    fn on_the_plus() -> Menu {
        let mut menu = Menu::default();
        menu.open(false);
        menu.library.sync(0, 1);

        menu
    }

    /// The plus asks which way, rather than opening one picker of the two.
    #[test]
    fn the_plus_offers_a_game_and_a_folder() {
        let view = nested();
        let mut menu = on_the_plus();

        assert_eq!(menu.nav(NavAction::Confirm, &views(&view)), None);
        menu.add.sync(library::add_count(), 1);
        assert_eq!(
            menu.nav(NavAction::Confirm, &views(&view)),
            Some(UiCmd::AddRom)
        );

        // Back on the shelf, and the plus is still what the focus is on.
        menu.nav(NavAction::Confirm, &views(&view));
        menu.add.sync(library::add_count(), 1);
        menu.nav(NavAction::Down, &views(&view));
        assert_eq!(
            menu.nav(NavAction::Confirm, &views(&view)),
            Some(UiCmd::AddRomsDir)
        );
    }

    #[test]
    fn backing_out_of_the_sheet_asks_for_nothing() {
        let view = nested();
        let mut menu = on_the_plus();
        menu.nav(NavAction::Confirm, &views(&view));
        menu.add.sync(library::add_count(), 1);

        assert_eq!(menu.nav(NavAction::Back, &views(&view)), None);
        // On the shelf again: Back there is what leaves, and with no game to return
        // to there is nowhere to go.
        assert_eq!(menu.nav(NavAction::Back, &views(&view)), None);
    }

    /// Walks the header to the sort button and opens its sheet.
    fn on_the_sort_sheet(view: &SettingsView) -> Menu {
        let mut menu = on_the_plus();
        menu.nav(NavAction::Right, &views(view));
        menu.nav(NavAction::Confirm, &views(view));

        menu
    }

    /// The sheet is presented already standing on the order in force, which is what
    /// tells the user which one that is.
    #[test]
    fn the_sort_sheet_opens_on_the_order_in_force() {
        let view = nested();
        let mut menu = on_the_sort_sheet(&view);

        assert_eq!(
            menu.nav(NavAction::Confirm, &views(&view)),
            Some(UiCmd::SortLibrary(SortBy::Recent)),
            "the view's own order, which the fixture leaves at the default"
        );
    }

    #[test]
    fn another_order_is_asked_for_by_walking_to_it() {
        let view = nested();
        let mut menu = on_the_sort_sheet(&view);

        assert_eq!(menu.nav(NavAction::Up, &views(&view)), None);
        assert_eq!(
            menu.nav(NavAction::Confirm, &views(&view)),
            Some(UiCmd::SortLibrary(SortBy::Name))
        );
    }

    /// The UI can be closed from anywhere, so a page left open must not leave a trail
    /// for the next visit to back out through.
    #[test]
    fn reopening_the_ui_drops_a_page_left_open() {
        let view = nested();
        let mut menu = on_settings(&view);
        sync_page(&mut menu, &view);
        menu.nav(NavAction::Confirm, &views(&view));
        assert_eq!(menu.on_settings_page(), Some(CHILD_PAGE));
        assert_eq!(menu.settings_depth(), 1);

        menu.open(true);

        assert_eq!(menu.settings_depth(), 0);
    }
}
