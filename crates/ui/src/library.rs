//! The library screen: the game collection as a shelf of cartridges, and what can
//! be done with one besides playing it.

use crate::cart::{self, CartKind};
use crate::image::{RgbImage, TextureCache};
use crate::menu::UiCmd;
use crate::nav::{FocusEvent, GridFocus, NavAction};
use crate::overlay;
use crate::theme::{self, ROW_PAD, WIDTH_SHEET};
use egui::{Rect, Sense, Ui, Vec2};

/// Read-model the platform fills in; the screen never reaches into app state.
pub struct LibraryView<'a> {
    pub entries: &'a [RomEntry],
    /// Bumped every time the platform rebuilds this view, which is the signal to
    /// throw away textures uploaded from the covers it replaced.
    pub version: u64,
}

pub struct RomEntry {
    pub title: String,
    pub kind: CartKind,
    /// Cover art for the cart's label, when the game has any.
    pub cover: Option<RgbImage>,
}

/// What the cart's own sheet offers. Playing it is Confirm on the shelf, so it is
/// not repeated here.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum RomAction {
    Rename,
    Cover,
}

const ACTIONS: [(&str, RomAction); 2] =
    [("Rename", RomAction::Rename), ("Cover", RomAction::Cover)];

pub fn action_count() -> usize {
    ACTIONS.len()
}

pub fn action_at(index: usize) -> Option<RomAction> {
    ACTIONS.get(index).map(|(_, action)| *action)
}

pub fn show_actions(root: &mut Ui, title: &str, focus: &mut GridFocus) -> Option<RomAction> {
    focus.sync(action_count(), 1);
    let height = overlay::title_height() + overlay::rows_height(action_count());
    let mut clicked = None;

    overlay::popup(root, Vec2::new(WIDTH_SHEET, height), |ui| {
        theme::heading(ui, title);
        clicked = overlay::rows(ui, ACTIONS.iter().map(|(label, _)| *label), focus);
    });

    clicked.and_then(action_at)
}

const ICON_SIZE: f32 = 18.0;
const TILE_WIDTH: f32 = 132.0;
/// Smallest gap between carts; the shelf widens it to use up the row's slack,
/// but never past `MAX_GAP_TILES` of a cart's width.
const MIN_GAP: f32 = 14.0;
const MAX_GAP_TILES: f32 = 0.5;
const FOCUS_SCALE: f32 = 1.08;
/// Room the focus ring needs outside the cart, as a fraction of its width.
const RING: f32 = 0.06;

/// What the header's buttons ask for.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum LibraryEvent {
    AddRom,
    OpenSettings,
}

/// The header, left to right. Icons alone: a gear and a plus need no caption, and
/// the words are in the tooltip for whoever has a pointer.
const HEADER: [(&str, &str, LibraryEvent); 2] = [
    ("\u{2795}", "Add game…", LibraryEvent::AddRom),
    ("\u{2699}", "Settings", LibraryEvent::OpenSettings),
];

/// What activating something on this screen leads to.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum LibraryPick {
    /// A cart of the shelf, to play.
    Rom(usize),
    Header(LibraryEvent),
    Back,
}

/// Focus across the screen's two zones. [`GridFocus`] models one grid, and the
/// header is a short row over a wide one, so the zones are kept apart and vertical
/// movement hands the focus between them — otherwise nothing in the header could be
/// reached without a pointer.
#[derive(Default)]
pub struct LibraryFocus {
    on_header: bool,
    header: GridFocus,
    shelf: GridFocus,
}

impl LibraryFocus {
    /// The shelf reports the shape its layout produced; the header's never changes.
    pub fn sync(&mut self, entries: usize, columns: usize) {
        self.header.sync(HEADER.len(), HEADER.len());
        self.shelf.sync(entries, columns);

        // With nothing on the shelf the header is all there is, and adding a game
        // is exactly what an empty library needs.
        if self.shelf.is_empty() {
            self.on_header = true;
        }
    }

    pub fn nav(&mut self, action: NavAction) -> Option<LibraryPick> {
        if self.on_header {
            return self.nav_header(action);
        }

        // Up out of the top row reaches the header rather than wrapping to the
        // bottom of the shelf.
        if action == NavAction::Up && self.shelf.on_top_row() {
            self.on_header = true;

            return None;
        }

        match self.shelf.nav(action)? {
            FocusEvent::Activate(index) => Some(LibraryPick::Rom(index)),
            FocusEvent::Back => Some(LibraryPick::Back),
        }
    }

    fn nav_header(&mut self, action: NavAction) -> Option<LibraryPick> {
        // Either way out of the header is the shelf, which keeps the cart it was
        // left on.
        if matches!(action, NavAction::Up | NavAction::Down) {
            self.on_header = self.shelf.is_empty();

            return None;
        }

        match self.header.nav(action)? {
            FocusEvent::Activate(index) => HEADER
                .get(index)
                .map(|(_, _, event)| LibraryPick::Header(*event)),
            FocusEvent::Back => Some(LibraryPick::Back),
        }
    }

    /// The cart the shelf is on, for the things that are about one cart.
    pub fn rom(&self) -> Option<usize> {
        (!self.on_header && !self.shelf.is_empty()).then(|| self.shelf.index())
    }

    /// Moves the focus to what the pointer is over, so the two never disagree.
    fn point_at_header(&mut self, index: usize) {
        self.on_header = true;
        self.header.focus(index);
    }

    fn point_at_shelf(&mut self, index: usize) {
        self.on_header = false;
        self.shelf.focus(index);
    }
}

/// Takes egui's root [`Ui`] (what panels are shown into), so the same screen
/// works under any backend that can run egui. Returns what the header asked for —
/// switching screen is the menu's business, not this one's.
pub fn library(
    root: &mut Ui,
    view: &LibraryView,
    focus: &mut LibraryFocus,
    covers: &mut TextureCache,
    out: &mut Vec<UiCmd>,
) -> Option<LibraryEvent> {
    let mut event = None;
    covers.sync(view.version);

    egui::CentralPanel::default().show(root, |ui| {
        theme::page(ui);
        // The title and the buttons share one groove: the page's header is a single
        // band cut into the surface, not a title with controls floating beside it.
        let band = theme::heading_band(ui);
        theme::heading_in(ui, band, egui::Align2::LEFT_CENTER, "Library");
        // Right to left, so the group sits at the far end; reversed, so the
        // buttons still read in the order the focus walks them.
        let mut header = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(band.shrink2(egui::vec2(ROW_PAD, 0.0)))
                .layout(egui::Layout::right_to_left(egui::Align::Center)),
        );
        for (index, (glyph, hint, asked)) in HEADER.iter().enumerate().rev() {
            let focused = focus.on_header && focus.header.is_focused(index);
            let icon = egui::RichText::new(*glyph).size(ICON_SIZE);
            let response = header.add(egui::Button::selectable(focused, icon));

            if response.hovered() {
                focus.point_at_header(index);
            }

            if response.on_hover_text(*hint).clicked() {
                event = Some(*asked);
            }
        }
        ui.add_space(MIN_GAP * 0.5);
        focus.sync(view.entries.len(), 1);

        if view.entries.is_empty() {
            ui.label("No ROMs yet.");
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| shelf(ui, view, focus, covers, out));
    });

    event
}

fn shelf(
    ui: &mut Ui,
    view: &LibraryView,
    focus: &mut LibraryFocus,
    covers: &mut TextureCache,
    out: &mut Vec<UiCmd>,
) {
    let follow_focus = focus.shelf.take_moved();
    let tile = Vec2::new(TILE_WIDTH, TILE_WIDTH * cart::ASPECT);
    // Every cell reserves what the focused cart needs, so growing one never
    // overflows the row — at the panel's edges that overflow is clipped away.
    let cell = tile * FOCUS_SCALE + Vec2::splat(TILE_WIDTH * RING * 2.0);
    let shelf_width = ui.available_width();
    let fit = (((shelf_width + MIN_GAP) / (cell.x + MIN_GAP)).floor() as usize).max(1);
    // A library smaller than one row still spreads across the shelf, so the row is
    // as wide as it can be before the slack is shared out.
    let columns = fit.min(view.entries.len()).max(1);
    // Space-evenly: the same gap at both ends as between the carts, until it grows
    // wider than half a cart and the shelf would just look sparse.
    let gap = ((shelf_width - columns as f32 * cell.x) / (columns + 1) as f32)
        .clamp(MIN_GAP, TILE_WIDTH * MAX_GAP_TILES);
    // Every row starts at the same offset — one computed from a *full* row — so
    // columns line up and a short last row trails off into empty space instead of
    // sitting between its neighbours.
    let full_width = columns as f32 * cell.x + (columns + 1) as f32 * gap;
    let leading = (shelf_width - full_width) * 0.5 + gap;
    focus.sync(view.entries.len(), columns);
    let on_shelf = !focus.on_header;

    for (row, entries) in view.entries.chunks(columns).enumerate() {
        ui.horizontal(|ui| {
            ui.add_space(leading);
            ui.spacing_mut().item_spacing.x = gap;

            for (column, entry) in entries.iter().enumerate() {
                let index = row * columns + column;
                let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                let focused = on_shelf && focus.shelf.is_focused(index);

                // Pointer and directional input drive the same highlight, so the
                // two never disagree about what is selected.
                if response.hovered() {
                    focus.point_at_shelf(index);
                }

                if response.clicked() {
                    out.push(UiCmd::LaunchRom(index));
                }

                // Directional input can walk the highlight off-screen; bring it back.
                if focused && follow_focus {
                    ui.scroll_to_rect(rect, None);
                }

                let size = if focused { tile * FOCUS_SCALE } else { tile };
                let cart = Rect::from_center_size(rect.center(), size);
                let cover = entry
                    .cover
                    .as_ref()
                    .map(|cover| covers.texture(ui, index, cover).clone());
                cart::paint(ui, cart, &entry.title, entry.kind, focused, cover.as_ref());
            }
        });
        ui.add_space(MIN_GAP);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two carts on one row, so Up from either reaches the header.
    fn shelved() -> LibraryFocus {
        let mut focus = LibraryFocus::default();
        focus.sync(2, 2);

        focus
    }

    #[test]
    fn the_shelf_has_the_focus_while_there_is_one() {
        let mut focus = shelved();

        assert_eq!(focus.rom(), Some(0));
        assert_eq!(focus.nav(NavAction::Confirm), Some(LibraryPick::Rom(0)));
    }

    #[test]
    fn up_reaches_the_header_and_down_gives_it_back() {
        let mut focus = shelved();

        assert_eq!(focus.nav(NavAction::Up), None);
        assert_eq!(focus.rom(), None, "the header has it now");
        assert_eq!(
            focus.nav(NavAction::Confirm),
            Some(LibraryPick::Header(LibraryEvent::AddRom))
        );

        assert_eq!(focus.nav(NavAction::Down), None);
        assert_eq!(focus.rom(), Some(0), "and the shelf keeps its cart");
    }

    #[test]
    fn the_header_is_walked_sideways() {
        let mut focus = shelved();
        focus.nav(NavAction::Up);

        assert_eq!(focus.nav(NavAction::Right), None);
        assert_eq!(
            focus.nav(NavAction::Confirm),
            Some(LibraryPick::Header(LibraryEvent::OpenSettings))
        );
    }

    /// Adding a game is the one thing an empty library needs, so it cannot be a
    /// place the focus refuses to go.
    #[test]
    fn an_empty_shelf_leaves_the_focus_on_the_header() {
        let mut focus = LibraryFocus::default();
        focus.sync(0, 1);

        assert_eq!(focus.rom(), None);
        assert_eq!(
            focus.nav(NavAction::Confirm),
            Some(LibraryPick::Header(LibraryEvent::AddRom))
        );
        assert_eq!(focus.nav(NavAction::Down), None, "nowhere to go");
        assert_eq!(
            focus.nav(NavAction::Confirm),
            Some(LibraryPick::Header(LibraryEvent::AddRom))
        );
    }

    #[test]
    fn either_zone_backs_out_of_the_screen() {
        let mut focus = shelved();
        assert_eq!(focus.nav(NavAction::Back), Some(LibraryPick::Back));

        focus.nav(NavAction::Up);
        assert_eq!(focus.nav(NavAction::Back), Some(LibraryPick::Back));
    }
}
