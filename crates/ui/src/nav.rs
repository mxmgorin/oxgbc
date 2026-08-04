//! Directional focus for a grid. egui's own Tab focus is one-dimensional, so a
//! cartridge shelf needs its own model: the view reports the column count its
//! layout produced, and movement is expressed in that grid.

/// Directional intent from any bound input, already resolved from joypad buttons.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum NavAction {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    /// Whatever else can be done with the focused item, which is the screen's
    /// business rather than the focus model's.
    Options,
}

/// What a [`NavAction`] meant beyond moving the highlight.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum FocusEvent {
    Activate(usize),
    Back,
}

/// Focused cell of a grid whose width depends on the window, so the layout feeds
/// its item and column counts back in every frame ([`Self::sync`]).
#[derive(Default)]
pub struct GridFocus {
    index: usize,
    len: usize,
    columns: usize,
    /// Set when directional input moved the highlight, so the view can scroll it
    /// into sight. Pointer hovering doesn't set it — the pointer is already there.
    moved: bool,
}

impl GridFocus {
    pub fn sync(&mut self, len: usize, columns: usize) {
        self.len = len;
        self.columns = columns.max(1);
        self.index = self.index.min(len.saturating_sub(1));
    }

    pub fn index(&self) -> usize {
        self.index
    }

    pub fn is_focused(&self, index: usize) -> bool {
        self.len > 0 && self.index == index
    }

    /// Whether moving up would wrap around, which is a screen's cue to hand the
    /// focus to whatever sits above the grid instead.
    pub fn on_top_row(&self) -> bool {
        self.index < self.columns
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Whether the highlight moved since this was last asked.
    pub fn take_moved(&mut self) -> bool {
        std::mem::take(&mut self.moved)
    }

    /// Moves the highlight to a cell the pointer is over, keeping mixed
    /// mouse/gamepad input coherent.
    pub fn focus(&mut self, index: usize) {
        if index < self.len {
            self.index = index;
        }
    }

    pub fn nav(&mut self, action: NavAction) -> Option<FocusEvent> {
        if action == NavAction::Back {
            return Some(FocusEvent::Back);
        }

        // Options is about the focused item, not about which item that is, so the
        // screen handles it before ever asking the focus.
        if action == NavAction::Options {
            return None;
        }

        if self.len == 0 {
            return None;
        }

        self.moved = true;

        match action {
            NavAction::Confirm => return Some(FocusEvent::Activate(self.index)),
            // Horizontal movement runs along the whole shelf rather than stopping
            // at a row's edge, so no cart is more than a few presses away.
            NavAction::Right => self.index = (self.index + 1) % self.len,
            NavAction::Left => self.index = (self.index + self.len - 1) % self.len,
            NavAction::Down => self.index = self.below(),
            NavAction::Up => self.index = self.above(),
            NavAction::Back | NavAction::Options => unreachable!("returned above"),
        }

        None
    }

    /// Wraps to the top of the same column, and to the last row that has a cell
    /// in it when the bottom row is short.
    fn below(&self) -> usize {
        let next = self.index + self.columns;

        if next < self.len {
            next
        } else {
            self.index % self.columns
        }
    }

    fn above(&self) -> usize {
        if self.index >= self.columns {
            return self.index - self.columns;
        }

        let column = self.index % self.columns;
        let rows = self.len.div_ceil(self.columns);
        let bottom = column + (rows - 1) * self.columns;

        if bottom < self.len {
            bottom
        } else {
            bottom - self.columns
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 7 items in 3 columns:
    /// ```text
    /// 0 1 2
    /// 3 4 5
    /// 6
    /// ```
    fn ragged() -> GridFocus {
        let mut focus = GridFocus::default();
        focus.sync(7, 3);

        focus
    }

    fn after(from: usize, action: NavAction) -> usize {
        let mut focus = ragged();
        focus.focus(from);
        focus.nav(action);

        focus.index()
    }

    #[test]
    fn moves_within_the_grid() {
        assert_eq!(after(0, NavAction::Right), 1);
        assert_eq!(after(1, NavAction::Left), 0);
        assert_eq!(after(0, NavAction::Down), 3);
        assert_eq!(after(3, NavAction::Up), 0);
    }

    #[test]
    fn horizontal_wraps_across_the_whole_shelf() {
        assert_eq!(after(6, NavAction::Right), 0);
        assert_eq!(after(0, NavAction::Left), 6);
        assert_eq!(after(2, NavAction::Right), 3);
    }

    #[test]
    fn vertical_wraps_within_the_column() {
        assert_eq!(after(6, NavAction::Down), 0);
        assert_eq!(after(0, NavAction::Up), 6);
    }

    #[test]
    fn vertical_skips_missing_cells_in_a_short_row() {
        // Column 1's last cell is 4, not 7.
        assert_eq!(after(4, NavAction::Down), 1);
        assert_eq!(after(1, NavAction::Up), 4);
    }

    #[test]
    fn only_directional_input_asks_for_a_scroll() {
        let mut focus = ragged();
        focus.focus(3);
        assert!(!focus.take_moved());

        focus.nav(NavAction::Down);
        assert!(focus.take_moved());
        assert!(!focus.take_moved());
    }

    #[test]
    fn confirm_and_back_report_events() {
        let mut focus = ragged();
        focus.focus(5);

        assert_eq!(focus.nav(NavAction::Confirm), Some(FocusEvent::Activate(5)));
        assert_eq!(focus.nav(NavAction::Back), Some(FocusEvent::Back));
        assert_eq!(focus.index(), 5);
    }

    #[test]
    fn an_empty_library_only_reports_back() {
        let mut focus = GridFocus::default();
        focus.sync(0, 4);

        assert_eq!(focus.nav(NavAction::Down), None);
        assert_eq!(focus.nav(NavAction::Confirm), None);
        assert_eq!(focus.nav(NavAction::Back), Some(FocusEvent::Back));
    }

    #[test]
    fn a_shrinking_library_keeps_focus_in_range() {
        let mut focus = ragged();
        focus.focus(6);
        focus.sync(3, 3);

        assert_eq!(focus.index(), 2);
    }
}
