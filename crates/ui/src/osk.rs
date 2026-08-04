//! The on-screen keyboard: to type something with a gamepad

use crate::nav::NavAction;
use crate::theme::{self, UNIT};
use egui::{Ui, Vec2};

/// What pressing a key means to whatever is being typed into.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum OskEvent {
    Type(char),
    Backspace,
    Commit,
    Cancel,
}

/// The keys that type themselves, lower case; [`Osk::shift`] decides the case.
const KEYS: [&str; 5] = [
    "1234567890",
    "qwertyuiop",
    "asdfghjkl'",
    "zxcvbnm,.",
    "-_()!&+:?\"",
];

/// What a key of the bottom row does rather than types.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
enum Action {
    Shift,
    Backspace,
    Space,
    Cancel,
    Commit,
}

/// The bottom row: a face, what it does, and how many key widths it takes — which
/// add up to a letter row's.
const ACTIONS: [(&str, Action, f32); 5] = [
    ("Shift", Action::Shift, 2.0),
    ("Del", Action::Backspace, 1.0),
    ("Space", Action::Space, 3.0),
    ("Cancel", Action::Cancel, 2.0),
    ("Save", Action::Commit, 2.0),
];

const ROWS: usize = KEYS.len() + 1;
/// The row that acts rather than types.
const ACTION_ROW: usize = KEYS.len();
/// The widest row, which sets a key's width.
const COLUMNS: usize = 10;

const KEY_HEIGHT: f32 = UNIT * 7.0;
const KEY_GAP: f32 = UNIT;

/// Which key the highlight is on, and whether the letters are upper case.
///
/// Its own focus rather than [`crate::nav::GridFocus`]: the rows differ in length
/// and each keeps its own ends — right off `p` belongs at the start of that row,
/// not on the row below.
#[derive(Default)]
pub struct Osk {
    row: usize,
    col: usize,
    /// Where the last sideways move settled, so passing through a short row does
    /// not drag the highlight left.
    wanted: usize,
    shift: bool,
}

impl Osk {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Directional input from a gamepad, which egui never sees.
    pub fn nav(&mut self, action: NavAction) -> Option<OskEvent> {
        match action {
            NavAction::Back => return Some(OskEvent::Cancel),
            // A typed name is usually right, so taking it needs no walking to.
            NavAction::Options => return Some(OskEvent::Commit),
            NavAction::Confirm => return self.press(self.row, self.col),
            NavAction::Left => {
                self.col = (self.col + row_len(self.row) - 1) % row_len(self.row);
                self.wanted = self.col;
            }
            NavAction::Right => {
                self.col = (self.col + 1) % row_len(self.row);
                self.wanted = self.col;
            }
            NavAction::Down => self.row = (self.row + 1) % ROWS,
            NavAction::Up => self.row = (self.row + ROWS - 1) % ROWS,
        }

        self.col = self.wanted.min(row_len(self.row) - 1);

        None
    }

    fn press(&mut self, row: usize, col: usize) -> Option<OskEvent> {
        let Some((_, action, _)) = action_at(row, col) else {
            return Some(OskEvent::Type(self.cased(char_at(row, col)?)));
        };

        match action {
            Action::Shift => {
                self.shift = !self.shift;

                None
            }
            Action::Backspace => Some(OskEvent::Backspace),
            Action::Space => Some(OskEvent::Type(' ')),
            Action::Cancel => Some(OskEvent::Cancel),
            Action::Commit => Some(OskEvent::Commit),
        }
    }

    fn cased(&self, key: char) -> char {
        if self.shift {
            key.to_ascii_uppercase()
        } else {
            key
        }
    }
}

fn row_len(row: usize) -> usize {
    if row == ACTION_ROW {
        ACTIONS.len()
    } else {
        KEYS[row].chars().count()
    }
}

fn char_at(row: usize, col: usize) -> Option<char> {
    KEYS.get(row)?.chars().nth(col)
}

fn action_at(row: usize, col: usize) -> Option<&'static (&'static str, Action, f32)> {
    (row == ACTION_ROW).then(|| ACTIONS.get(col)).flatten()
}

/// Height to reserve, so the popup can be sized before the keyboard is drawn.
pub(crate) fn height() -> f32 {
    ROWS as f32 * (KEY_HEIGHT + KEY_GAP)
}

/// Draws the keyboard and reports what a click asked for; a gamepad comes in
/// through [`Osk::nav`] instead.
pub(crate) fn show(ui: &mut Ui, osk: &mut Osk) -> Option<OskEvent> {
    let key_width = (ui.available_width() - KEY_GAP * (COLUMNS - 1) as f32) / COLUMNS as f32;
    let mut event = None;

    ui.scope(|ui| {
        ui.spacing_mut().item_spacing = Vec2::splat(KEY_GAP);

        for row in 0..ROWS {
            ui.horizontal(|ui| {
                for col in 0..row_len(row) {
                    event = key(ui, osk, row, col, key_width).or(event);
                }
            });
        }
    });

    event
}

fn key(ui: &mut Ui, osk: &mut Osk, row: usize, col: usize, key_width: f32) -> Option<OskEvent> {
    let (label, span, on) = match action_at(row, col) {
        Some((label, action, span)) => (
            (*label).to_owned(),
            *span,
            // Shift stays lit while it is on, after the highlight has moved off it.
            *action == Action::Shift && osk.shift,
        ),
        None => (osk.cased(char_at(row, col)?).to_string(), 1.0, false),
    };
    let focused = osk.row == row && osk.col == col;
    let width = key_width * span + KEY_GAP * (span - 1.0);
    let face = egui::Button::selectable(focused || on, label).corner_radius(theme::ROW_RADIUS);
    let response = ui.add_sized([width, KEY_HEIGHT], face);

    // Pointer and directional input drive the same highlight.
    if response.hovered() {
        osk.row = row;
        osk.col = col;
        osk.wanted = col;
    }

    response.clicked().then(|| osk.press(row, col)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(osk: &Osk) -> (usize, usize) {
        (osk.row, osk.col)
    }

    #[test]
    fn a_row_keeps_its_own_ends() {
        let mut osk = Osk::default();
        osk.nav(NavAction::Left);

        assert_eq!(at(&osk), (0, COLUMNS - 1));

        osk.nav(NavAction::Right);
        assert_eq!(at(&osk), (0, 0));
    }

    #[test]
    fn a_short_row_does_not_drag_the_column_along() {
        let mut osk = Osk::default();

        for _ in 0..COLUMNS - 1 {
            osk.nav(NavAction::Right);
        }

        // `zxcvbnm,.` is a key shorter than the rows around it.
        for _ in 0..3 {
            osk.nav(NavAction::Down);
        }

        assert_eq!(at(&osk), (3, KEYS[3].chars().count() - 1));

        osk.nav(NavAction::Up);
        assert_eq!(at(&osk), (2, COLUMNS - 1));
    }

    #[test]
    fn a_letter_key_types_itself() {
        let mut osk = Osk::default();
        osk.nav(NavAction::Down);

        assert_eq!(osk.nav(NavAction::Confirm), Some(OskEvent::Type('q')));
    }

    /// Shift stays on until pressed again, so capitals take one press rather than
    /// one per letter.
    #[test]
    fn shift_moves_the_letters_and_leaves_the_rest() {
        let mut osk = Osk {
            row: ACTION_ROW,
            col: ACTIONS
                .iter()
                .position(|(_, action, _)| *action == Action::Shift)
                .expect("the keyboard offers a shift"),
            ..Default::default()
        };

        assert_eq!(osk.nav(NavAction::Confirm), None);
        assert!(osk.shift);

        (osk.row, osk.col) = (1, 0);
        assert_eq!(osk.nav(NavAction::Confirm), Some(OskEvent::Type('Q')));

        osk.row = 0;
        assert_eq!(osk.nav(NavAction::Confirm), Some(OskEvent::Type('1')));
    }

    #[test]
    fn the_two_ways_out_need_no_walking_to() {
        let mut osk = Osk::default();

        assert_eq!(osk.nav(NavAction::Back), Some(OskEvent::Cancel));
        assert_eq!(osk.nav(NavAction::Options), Some(OskEvent::Commit));
    }

    /// The action row divides the same width as a letter row, or the block comes
    /// out ragged.
    #[test]
    fn the_action_row_spans_the_keyboard() {
        let span: f32 = ACTIONS.iter().map(|(_, _, span)| span).sum();

        assert_eq!(span as usize, COLUMNS);
        assert_eq!(KEYS[0].chars().count(), COLUMNS);
    }
}
