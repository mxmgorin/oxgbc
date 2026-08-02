//! Naming something: a save state, a cartridge. Typing needs a keyboard, so the
//! two ways out are buttons a gamepad can reach — its events never pass through
//! egui, and so are unaffected by the text field holding the keyboard.

use crate::nav::{FocusEvent, GridFocus, NavAction};
use crate::overlay;
use crate::theme::{self, ROW_GAP, ROW_HEIGHT, WIDTH_PANEL};
use egui::{Ui, Vec2};

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum RenameEvent {
    Commit,
    Cancel,
}

/// Save sits on the right, where the action that goes through belongs.
const ACTIONS: [(&str, RenameEvent); 2] = [
    ("Cancel", RenameEvent::Cancel),
    ("Save", RenameEvent::Commit),
];

/// Room for the heading and the hint under the buttons.
const TITLE_HEIGHT: f32 = ROW_HEIGHT + ROW_GAP;

/// The name being typed, which of the two buttons is picked, and whether the field
/// still has to take focus — egui only routes the keyboard to a widget that has it.
#[derive(Default)]
pub struct RenameEdit {
    text: String,
    focus: GridFocus,
    grab: bool,
}

impl RenameEdit {
    /// Starts from the current name, so a rename is an edit rather than retyping.
    pub fn start(&mut self, name: &str) {
        self.text = name.to_owned();
        self.grab = true;
        self.focus = GridFocus::default();
        // One row of buttons, so Left/Right walk it.
        self.focus.sync(ACTIONS.len(), ACTIONS.len());
        // Start on the action that keeps what was typed, wherever it sits.
        self.focus.focus(commit_index());
    }

    /// Directional input from a gamepad, which egui never sees.
    pub fn nav(&mut self, action: NavAction) -> Option<RenameEvent> {
        match self.focus.nav(action)? {
            FocusEvent::Activate(index) => action_at(index),
            FocusEvent::Back => Some(RenameEvent::Cancel),
        }
    }

    pub fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
}

fn action_at(index: usize) -> Option<RenameEvent> {
    ACTIONS.get(index).map(|(_, event)| *event)
}

fn commit_index() -> usize {
    ACTIONS
        .iter()
        .position(|(_, event)| *event == RenameEvent::Commit)
        .expect("ACTIONS always offers a way to commit")
}

/// `title` heads the screen and `hint` says what an empty name falls back to —
/// both differ between the things that can be named.
pub fn show(root: &mut Ui, title: &str, hint: &str, edit: &mut RenameEdit) -> Option<RenameEvent> {
    let height = TITLE_HEIGHT * 2.0 + overlay::rows_height(ACTIONS.len());
    let mut event = None;

    overlay::popup(root, Vec2::new(WIDTH_PANEL, height), |ui| {
        theme::heading(ui, title);
        let field = ui.add_sized(
            [ui.available_width(), ROW_HEIGHT],
            egui::TextEdit::singleline(&mut edit.text).hint_text(hint),
        );

        if std::mem::take(&mut edit.grab) {
            field.request_focus();
        }

        // The field has already handled the keystroke by the time we get here, so
        // both keys read as focus going away. Losing it any other way — a click on
        // one of the buttons, or outside — leaves the screen up for them to answer.
        if field.lost_focus() {
            let (enter, escape) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::Enter),
                    i.key_pressed(egui::Key::Escape),
                )
            });

            if enter {
                event = Some(RenameEvent::Commit);
            } else if escape {
                event = Some(RenameEvent::Cancel);
            }
        }

        edit.focus.sync(ACTIONS.len(), ACTIONS.len());

        ui.horizontal(|ui| {
            let width = (ui.available_width() - ROW_GAP) / ACTIONS.len() as f32;

            for (index, (label, action)) in ACTIONS.iter().enumerate() {
                let focused = edit.focus.is_focused(index);
                let response = ui.add_sized(
                    [width, ROW_HEIGHT],
                    egui::Button::selectable(focused, *label),
                );

                if response.hovered() {
                    edit.focus.focus(index);
                }

                // A click wins over the focus the field just gave up because of it.
                if response.clicked() {
                    event = Some(*action);
                }
            }
        });

        ui.weak("Enter to save, Esc to cancel");
    });

    event
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gamepad can't type, so both ways out have to be reachable by moving. Save
    /// is focused to begin with, whichever side of the row it is drawn on.
    #[test]
    fn the_buttons_answer_directional_input() {
        let mut edit = RenameEdit::default();
        edit.start("before the boss");

        assert_eq!(edit.nav(NavAction::Confirm), Some(RenameEvent::Commit));
        assert_eq!(edit.nav(NavAction::Left), None);
        assert_eq!(edit.nav(NavAction::Confirm), Some(RenameEvent::Cancel));
        assert_eq!(edit.nav(NavAction::Back), Some(RenameEvent::Cancel));
    }

    #[test]
    fn the_field_starts_from_the_current_name() {
        let mut edit = RenameEdit::default();
        edit.start("Ninja Gaiden");

        assert_eq!(edit.take_text(), "Ninja Gaiden");
        assert_eq!(edit.take_text(), "");
    }
}
