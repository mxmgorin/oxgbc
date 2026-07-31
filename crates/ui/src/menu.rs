//! Which screen the UI is on and what it asks the platform to do.
//!
//! The screens are the UI's own business — the app only knows it is paused — so
//! moving between them never leaves this crate. Everything the platform has to
//! act on comes back as a [`UiCmd`].

use crate::library::{library, LibraryView};
use crate::nav::{FocusEvent, GridFocus, NavAction};
use egui::{Align, Color32, Frame, Layout, Rect, UiBuilder, Vec2};

/// A request only the platform can carry out.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum UiCmd {
    LaunchRom(usize),
    Resume,
    SaveState,
    LoadState,
    RestartRom,
    Quit,
}

#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
enum Screen {
    #[default]
    Library,
    /// Over the dimmed game, when the UI is opened mid-play.
    Pause,
}

/// A row of the pause overlay: what it says and what it does.
const PAUSE_ITEMS: [(&str, UiCmd); 6] = [
    ("Resume", UiCmd::Resume),
    ("Save state", UiCmd::SaveState),
    ("Load state", UiCmd::LoadState),
    ("Restart", UiCmd::RestartRom),
    ("Library", UiCmd::Resume),
    ("Quit", UiCmd::Quit),
];
/// Index of the row that goes back to the library instead of emitting its cmd.
const PAUSE_LIBRARY: usize = 4;
const OVERLAY_WIDTH: f32 = 260.0;
const OVERLAY_DIM: Color32 = Color32::from_black_alpha(0xb4);
const ROW_HEIGHT: f32 = 32.0;
const ROW_GAP: f32 = 6.0;

#[derive(Default)]
pub struct Menu {
    screen: Screen,
    library: GridFocus,
    pause: GridFocus,
}

impl Menu {
    /// Entering with a game loaded pauses over it; otherwise the library is home.
    pub fn open(&mut self, has_game: bool) {
        self.screen = if has_game {
            Screen::Pause
        } else {
            Screen::Library
        };
    }

    pub fn nav(&mut self, action: NavAction) -> Option<UiCmd> {
        match self.screen {
            Screen::Library => match self.library.nav(action)? {
                FocusEvent::Activate(index) => Some(UiCmd::LaunchRom(index)),
                // Backing out of the library only makes sense with a game to
                // return to, and then the overlay is where we came from.
                FocusEvent::Back => None,
            },
            Screen::Pause => match self.pause.nav(action)? {
                FocusEvent::Activate(index) => self.activate_pause(index),
                FocusEvent::Back => Some(UiCmd::Resume),
            },
        }
    }

    pub fn show(&mut self, root: &mut egui::Ui, view: &LibraryView, out: &mut Vec<UiCmd>) {
        match self.screen {
            Screen::Library => library(root, view, &mut self.library, out),
            Screen::Pause => self.pause_overlay(root, out),
        }
    }

    fn activate_pause(&mut self, index: usize) -> Option<UiCmd> {
        if index == PAUSE_LIBRARY {
            self.screen = Screen::Library;
            return None;
        }

        PAUSE_ITEMS.get(index).map(|(_, cmd)| *cmd)
    }

    fn pause_overlay(&mut self, root: &mut egui::Ui, out: &mut Vec<UiCmd>) {
        self.pause.sync(PAUSE_ITEMS.len(), 1);
        let screen = root.ctx().content_rect();
        root.painter().rect_filled(screen, 0.0, OVERLAY_DIM);

        // Sized here rather than left to egui: an auto-sized panel takes a second
        // layout pass to settle, and both passes land on screen as a ghosted
        // double image.
        let height = PAUSE_ITEMS.len() as f32 * (ROW_HEIGHT + ROW_GAP) + ROW_GAP;
        let panel = Rect::from_center_size(screen.center(), Vec2::new(OVERLAY_WIDTH, height));
        let mut ui = root.new_child(
            UiBuilder::new()
                .max_rect(panel)
                .layout(Layout::top_down(Align::Center)),
        );

        Frame::popup(ui.style()).show(&mut ui, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;

            for (index, (label, _)) in PAUSE_ITEMS.iter().enumerate() {
                let focused = self.pause.is_focused(index);
                let row = egui::Button::selectable(focused, *label);
                let response = ui.add_sized([ui.available_width(), ROW_HEIGHT], row);

                if response.hovered() {
                    self.pause.focus(index);
                }

                if response.clicked() {
                    out.extend(self.activate_pause(index));
                }
            }
        });
    }
}
