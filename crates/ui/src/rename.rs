//! Naming something: a save state, a cartridge. The field takes a keyboard where
//! there is one, and the on-screen keyboard under for a gamepad.

use crate::nav::NavAction;
use crate::osk::{self, Osk, OskEvent};
use crate::overlay;
use crate::theme::{self, ROW_GAP, ROW_HEIGHT, WIDTH_PAGE};
use egui::text::CCursor;
use egui::text_edit::TextEditState;
use egui::text_selection::CCursorRange;
use egui::{Ui, Vec2};

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum RenameEvent {
    Commit,
    Cancel,
}

/// Room for the heading and the hint around the field.
const TITLE_HEIGHT: f32 = ROW_HEIGHT + ROW_GAP;

/// The name being typed, where the on-screen keyboard is, and whether the field
/// still has to take focus - egui only routes the keyboard to a widget that has it.
#[derive(Default)]
pub struct RenameEdit {
    text: String,
    osk: Osk,
    grab: bool,
    /// Where the next key lands, in characters. Kept here as well as in the field
    /// because the keyboard types between frames, when the field's own state is not
    /// there to be reached.
    caret: usize,
    /// Set when this side moved the caret, so the field is told rather than asked.
    moved: bool,
}

impl RenameEdit {
    /// Starts from the current name, so a rename is an edit rather than retyping.
    pub fn start(&mut self, name: &str) {
        self.text = name.to_owned();
        self.grab = true;
        self.caret = self.text.chars().count();
        self.moved = true;
        self.osk.reset();
    }

    /// Directional input from a gamepad, which egui never sees.
    pub fn nav(&mut self, action: NavAction) -> Option<RenameEvent> {
        let event = self.osk.nav(action)?;

        self.apply(event)
    }

    pub fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text)
    }

    /// Typing lands where the caret is, so the keyboard and the field edit the same
    /// name from the same place.
    fn apply(&mut self, event: OskEvent) -> Option<RenameEvent> {
        match event {
            OskEvent::Type(key) => {
                self.text.insert(self.byte_at(self.caret), key);
                self.caret += 1;
            }
            OskEvent::Backspace => {
                if self.caret == 0 {
                    return None;
                }

                self.caret -= 1;
                self.text.remove(self.byte_at(self.caret));
            }
            OskEvent::Commit => return Some(RenameEvent::Commit),
            OskEvent::Cancel => return Some(RenameEvent::Cancel),
        }

        self.moved = true;
        // The click that typed took the keyboard away from the field.
        self.grab = true;

        None
    }

    fn byte_at(&self, caret: usize) -> usize {
        self.text
            .char_indices()
            .nth(caret)
            .map_or(self.text.len(), |(at, _)| at)
    }
}

fn sync_caret(ui: &Ui, field: egui::Id, edit: &mut RenameEdit) {
    let Some(mut state) = TextEditState::load(ui.ctx(), field) else {
        return;
    };

    if std::mem::take(&mut edit.moved) {
        let at = CCursor::new(edit.caret);
        state.cursor.set_char_range(Some(CCursorRange::one(at)));
        state.store(ui.ctx(), field);
    } else if let Some(range) = state.cursor.char_range() {
        edit.caret = usize::from(range.primary.index).min(edit.text.chars().count());
    }
}

/// `title` heads the screen and `hint` says what an empty name falls back to —
/// both differ between the things that can be named.
pub fn show(root: &mut Ui, title: &str, hint: &str, edit: &mut RenameEdit) -> Option<RenameEvent> {
    let height = TITLE_HEIGHT * 2.0 + ROW_HEIGHT + osk::height();
    let mut event = None;

    overlay::popup(root, Vec2::new(WIDTH_PAGE, height), |ui| {
        theme::heading(ui, title);
        let field = ui.add_sized(
            [ui.available_width(), ROW_HEIGHT],
            egui::TextEdit::singleline(&mut edit.text).hint_text(hint),
        );

        if std::mem::take(&mut edit.grab) {
            field.request_focus();
        }

        sync_caret(ui, field.id, edit);

        // The field has already handled the keystroke by the time we get here, so
        // both keys read as focus going away. Losing it any other way — a click on
        // the keyboard, or outside — leaves the screen up.
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

        if let Some(clicked) = osk::show(ui, &mut edit.osk) {
            event = edit.apply(clicked).or(event);
        }

        ui.weak("Enter to save, Esc to cancel");
    });

    event
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gamepad types on the keyboard and leaves by its own keys, so no part of a
    /// rename needs a keyboard to reach.
    #[test]
    fn a_gamepad_types_and_gets_out() {
        let mut edit = RenameEdit::default();
        edit.start("NES");

        // Down to the letters, then along to `w`.
        edit.nav(NavAction::Down);
        edit.nav(NavAction::Right);
        assert_eq!(edit.nav(NavAction::Confirm), None);
        assert_eq!(edit.text, "NESw");

        assert_eq!(edit.nav(NavAction::Options), Some(RenameEvent::Commit));
        assert_eq!(edit.nav(NavAction::Back), Some(RenameEvent::Cancel));
    }

    /// The caret is the field's, so a key lands where it stands rather than on the
    /// end — and a name with a wide character in it counts in characters, not bytes.
    #[test]
    fn typing_lands_at_the_caret() {
        let mut edit = RenameEdit::default();
        edit.start("héro");
        edit.caret = 2;

        assert_eq!(edit.apply(OskEvent::Type('x')), None);
        assert_eq!(edit.text, "héxro");
        assert_eq!(edit.caret, 3);

        assert_eq!(edit.apply(OskEvent::Backspace), None);
        assert_eq!(edit.text, "héro");
        assert_eq!(edit.caret, 2);

        // Nothing before the start of the name to erase.
        edit.caret = 0;
        assert_eq!(edit.apply(OskEvent::Backspace), None);
        assert_eq!(edit.text, "héro");
    }

    #[test]
    fn the_field_starts_from_the_current_name() {
        let mut edit = RenameEdit::default();
        edit.start("Ninja Gaiden");

        assert_eq!(edit.take_text(), "Ninja Gaiden");
        assert_eq!(edit.take_text(), "");
    }
}
