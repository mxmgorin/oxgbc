//! Which screen the UI is on and what it asks the platform to do.
//!
//! The screens are the UI's own business — the app only knows it is paused — so
//! moving between them never leaves this crate. Everything the platform has to
//! act on comes back as a [`UiCmd`].

use crate::library::{self, library, LibraryView};
use crate::nav::{FocusEvent, GridFocus, NavAction};
use crate::overlay;
use crate::rename::{self, RenameEdit, RenameEvent};
use crate::settings::{row_at, settings, SettingId, SettingsView};
use crate::states::{self, RowPick, StatesView};
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
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
enum Screen {
    #[default]
    Library,
    /// Over the dimmed game, when the UI is opened mid-play.
    Pause,
    Settings,
    /// The game's save-state slots.
    States,
    /// What one slot, picked from the list, can be used for.
    StateActions(usize),
    /// Naming the state in one slot.
    StateRename(usize),
    /// What can be done with one cartridge of the library, besides playing it.
    RomActions(usize),
    /// Naming one cartridge.
    RomRename(usize),
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
    ("Settings", PauseAction::Open(Screen::Settings)),
    ("Quit", PauseAction::Cmd(UiCmd::Quit)),
];
const OVERLAY_WIDTH: f32 = 260.0;

#[derive(Default)]
pub struct Menu {
    screen: Screen,
    /// Where Back returns to, so settings can be reached from either screen.
    opener: Screen,
    library: GridFocus,
    pause: GridFocus,
    settings: GridFocus,
    states: GridFocus,
    actions: GridFocus,
    /// The cart action sheet, which is a different list from the slot one.
    rom_actions: GridFocus,
    shots: states::ShotCache,
    rename: RenameEdit,
}

impl Menu {
    /// Entering with a game loaded pauses over it; otherwise the library is home.
    pub fn open(&mut self, has_game: bool) {
        self.screen = if has_game {
            Screen::Pause
        } else {
            Screen::Library
        };
        self.opener = self.screen;
    }

    /// The slot the UI is working on, so the platform knows which screen to read.
    /// Renaming counts, so a detour through it doesn't throw the screen away.
    pub fn open_slot(&self) -> Option<usize> {
        match self.screen {
            Screen::StateActions(slot) | Screen::StateRename(slot) => Some(slot),
            _ => None,
        }
    }

    pub fn nav(&mut self, action: NavAction, views: &Views<'_>) -> Option<UiCmd> {
        match self.screen {
            Screen::Library if action == NavAction::Options => {
                self.open_rom_actions(views);

                None
            }
            Screen::Library => match self.library.nav(action)? {
                FocusEvent::Activate(index) => Some(UiCmd::LaunchRom(index)),
                // Backing out of the library only makes sense with a game to
                // return to, and then the overlay is where we came from.
                FocusEvent::Back => (self.opener == Screen::Pause).then(|| {
                    self.screen = Screen::Pause;
                    UiCmd::Resume
                }),
            },
            Screen::Pause => match self.pause.nav(action)? {
                FocusEvent::Activate(index) => self.activate_pause(index),
                FocusEvent::Back => Some(UiCmd::Resume),
            },
            Screen::Settings => self.nav_settings(action, views.settings),
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
        }
    }

    /// The focused cart's own sheet. An empty shelf has nothing to open one for.
    fn open_rom_actions(&mut self, views: &Views<'_>) {
        let index = self.library.index();

        if views.library.entries.get(index).is_none() {
            return;
        }

        self.rom_actions = GridFocus::default();
        self.screen = Screen::RomActions(index);
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
    fn nav_settings(&mut self, action: NavAction, view: &SettingsView) -> Option<UiCmd> {
        let step = match action {
            NavAction::Left => -1,
            NavAction::Right => 1,
            _ => 0,
        };

        if step != 0 {
            let row = row_at(view, self.settings.index())?;

            return Some(UiCmd::Setting { id: row.id, step });
        }

        match self.settings.nav(action)? {
            FocusEvent::Activate(index) => {
                let row = row_at(view, index)?;

                Some(UiCmd::Setting {
                    id: row.id,
                    step: 1,
                })
            }
            FocusEvent::Back => {
                self.screen = self.opener;
                None
            }
        }
    }

    pub fn show(&mut self, root: &mut egui::Ui, views: &Views<'_>, out: &mut Vec<UiCmd>) {
        match self.screen {
            Screen::Library => {
                if library(root, &views.library, &mut self.library, out) {
                    self.screen = Screen::Settings;
                }
            }
            Screen::Pause => self.pause_overlay(root, out),
            Screen::Settings => settings(root, views.settings, &mut self.settings, out),
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
            Screen::RomActions(index) => {
                let picked =
                    library::show_actions(root, rom_title(views, index), &mut self.rom_actions);

                if let Some(action) = picked {
                    out.extend(self.act_on_rom(action, index, views));
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

    fn pause_overlay(&mut self, root: &mut egui::Ui, out: &mut Vec<UiCmd>) {
        self.pause.sync(PAUSE_ITEMS.len(), 1);
        let size = Vec2::new(OVERLAY_WIDTH, overlay::rows_height(PAUSE_ITEMS.len()));
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
