//! The library screen: the game collection as a shelf of cartridges or as a list of
//! rows, and what can be done with one game besides playing it.
//!
//! Both layouts are the same library under one focus model and one texture cache;
//! which of the two is drawn is a setting the platform keeps.

use crate::cart::{self, CartKind};
use crate::image::{RgbImage, TextureCache};
use crate::menu::UiCmd;
use crate::nav::{FocusEvent, GridFocus, NavAction};
use crate::overlay;
use crate::theme::{self, ROW_GAP, ROW_PAD, THUMB_PAD, THUMB_ROW_HEIGHT, WIDTH_SHEET};
use egui::{Align2, Rect, Sense, Ui, Vec2};
use std::sync::Arc;

/// Read-model the platform fills in; the screen never reaches into app state.
pub struct LibraryView<'a> {
    pub entries: &'a [RomEntry],
    /// Bumped when a position takes a different cover, which is the signal to throw
    /// away the textures uploaded from the ones it replaced.
    pub version: u64,
    /// The order the entries are already in, which the sort sheet opens on.
    pub sort: SortBy,
    /// How the entries are laid out, which is also what the header's button offers
    /// to switch away from.
    pub layout: LibraryLayout,
}

pub struct RomEntry {
    pub title: String,
    pub kind: CartKind,
    /// Cover art for the cart's label, when the game has any. Shared, so a rebuilt
    /// shelf holds the pixels the platform already had rather than a copy.
    pub cover: Option<Arc<RgbImage>>,
    /// Platform-formatted play time behind the game, e.g. `"2 h 14 min played"`;
    /// empty until there is a minute of it. Shown by the list, which has a column
    /// to spare — a cart has nowhere to put it.
    pub played: String,
}

/// The two ways the library reads. Mirrored on the platform side, which is what
/// persists the choice; this crate only names them and asks for the switch.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub enum LibraryLayout {
    /// Cartridges as tiles, as many across as the window fits.
    #[default]
    Shelf,
    /// One game per row, its cart shrunk to a thumbnail beside the title. Fits far
    /// more games on screen, and is the only layout that has room for a second
    /// column of words.
    List,
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
    /// Open the sheet of ways to put games on the shelf.
    Add,
    /// Open the sheet of orders the shelf can be read in.
    Sort,
    /// Read the library in the other layout from now on.
    ToggleLayout,
    OpenSettings,
}

/// The header, left to right. What each button looks like is [`header_icon`]: one of
/// them changes with the layout, so the faces cannot live in here.
const HEADER: [LibraryEvent; 4] = [
    LibraryEvent::Add,
    LibraryEvent::Sort,
    LibraryEvent::ToggleLayout,
    LibraryEvent::OpenSettings,
];

/// A button's glyph and the words behind it. Icons alone: a gear and a plus need no
/// caption, and the words are in the tooltip for whoever has a pointer.
///
/// The layout button shows the layout it switches *to* rather than the one in force —
/// what the button does is what it says.
fn header_icon(event: LibraryEvent, layout: LibraryLayout) -> (&'static str, &'static str) {
    match event {
        LibraryEvent::Add => ("\u{2795}", "Add games"),
        LibraryEvent::Sort => ("\u{21C5}", "Sort games"),
        LibraryEvent::ToggleLayout => match layout {
            LibraryLayout::Shelf => ("\u{25A4}", "List view"),
            LibraryLayout::List => ("\u{25A6}", "Shelf view"),
        },
        LibraryEvent::OpenSettings => ("\u{2699}", "Settings"),
    }
}

/// The orders the shelf can be read in. Mirrored on the platform side, which is
/// what persists the choice; this crate only names them and reports the pick.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub enum SortBy {
    /// Most recently played first, then everything else by name.
    #[default]
    Recent,
    Name,
    Playtime,
}

const SORT_TITLE: &str = "Sort by";
const SORT_ACTIONS: [(&str, SortBy); 3] = [
    ("Name", SortBy::Name),
    ("Recently played", SortBy::Recent),
    ("Most played", SortBy::Playtime),
];

pub fn sort_count() -> usize {
    SORT_ACTIONS.len()
}

pub fn sort_at(index: usize) -> Option<SortBy> {
    SORT_ACTIONS.get(index).map(|(_, sort)| *sort)
}

/// Where `sort` sits in the sheet, so it opens on the order the shelf is already
/// in rather than on its first row.
pub fn sort_row(sort: SortBy) -> usize {
    SORT_ACTIONS
        .iter()
        .position(|(_, candidate)| *candidate == sort)
        .unwrap_or_default()
}

pub fn show_sort(root: &mut Ui, focus: &mut GridFocus) -> Option<SortBy> {
    focus.sync(sort_count(), 1);
    let height = overlay::title_height() + overlay::rows_height(sort_count());
    let mut clicked = None;

    overlay::popup(root, Vec2::new(WIDTH_SHEET, height), |ui| {
        theme::heading(ui, SORT_TITLE);
        clicked = overlay::rows(ui, SORT_ACTIONS.iter().map(|(label, _)| *label), focus);
    });

    clicked.and_then(sort_at)
}

/// The two ways games reach the shelf, which is what the plus offers.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum AddAction {
    /// One game, picked by file and started right away.
    OpenRom,
    /// A folder, whose games are the ones the shelf lists from then on.
    ScanDir,
}

const ADD_TITLE: &str = "Add games";
const ADD_ACTIONS: [(&str, AddAction); 2] = [
    ("Open a game…", AddAction::OpenRom),
    ("Scan a folder…", AddAction::ScanDir),
];

pub fn add_count() -> usize {
    ADD_ACTIONS.len()
}

pub fn add_at(index: usize) -> Option<AddAction> {
    ADD_ACTIONS.get(index).map(|(_, action)| *action)
}

pub fn show_add(root: &mut Ui, focus: &mut GridFocus) -> Option<AddAction> {
    focus.sync(add_count(), 1);
    let height = overlay::title_height() + overlay::rows_height(add_count());
    let mut clicked = None;

    overlay::popup(root, Vec2::new(WIDTH_SHEET, height), |ui| {
        theme::heading(ui, ADD_TITLE);
        clicked = overlay::rows(ui, ADD_ACTIONS.iter().map(|(label, _)| *label), focus);
    });

    clicked.and_then(add_at)
}

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
///
/// The games are one grid either way: the list is a shelf one column wide, so both
/// layouts move the focus through the same model.
#[derive(Default)]
pub struct LibraryFocus {
    on_header: bool,
    header: GridFocus,
    games: GridFocus,
}

impl LibraryFocus {
    /// The layout reports the shape it produced; the header's never changes.
    pub fn sync(&mut self, entries: usize, columns: usize) {
        self.header.sync(HEADER.len(), HEADER.len());
        self.games.sync(entries, columns);

        // With nothing in the library the header is all there is, and adding a game
        // is exactly what an empty one needs.
        if self.games.is_empty() {
            self.on_header = true;
        }
    }

    pub fn nav(&mut self, action: NavAction) -> Option<LibraryPick> {
        if self.on_header {
            return self.nav_header(action);
        }

        // Up out of the top row reaches the header rather than wrapping to the
        // bottom of the library.
        if action == NavAction::Up && self.games.on_top_row() {
            self.on_header = true;

            return None;
        }

        match self.games.nav(action)? {
            FocusEvent::Activate(index) => Some(LibraryPick::Rom(index)),
            FocusEvent::Back => Some(LibraryPick::Back),
        }
    }

    fn nav_header(&mut self, action: NavAction) -> Option<LibraryPick> {
        // Either way out of the header is the library, which keeps the cart it was
        // left on.
        if matches!(action, NavAction::Up | NavAction::Down) {
            self.on_header = self.games.is_empty();

            return None;
        }

        match self.header.nav(action)? {
            FocusEvent::Activate(index) => {
                HEADER.get(index).map(|event| LibraryPick::Header(*event))
            }
            FocusEvent::Back => Some(LibraryPick::Back),
        }
    }

    /// The cart the library is on, for the things that are about one cart.
    pub fn rom(&self) -> Option<usize> {
        (!self.on_header && !self.games.is_empty()).then(|| self.games.index())
    }

    /// Moves the focus to what the pointer is over, so the two never disagree.
    fn point_at_header(&mut self, index: usize) {
        self.on_header = true;
        self.header.focus(index);
    }

    fn point_at_game(&mut self, index: usize) {
        self.on_header = false;
        self.games.focus(index);
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
        for (index, asked) in HEADER.iter().enumerate().rev() {
            let focused = focus.on_header && focus.header.is_focused(index);
            let (glyph, hint) = header_icon(*asked, view.layout);
            // Looked up in the monospace family, which starts at Hack: the arrows
            // and the two layout squares live there and nowhere in the proportional
            // chain. The emoji glyphs are in neither font, so they fall through to
            // the same emoji fonts as before and look no different.
            let icon = egui::RichText::new(glyph)
                .size(ICON_SIZE)
                .family(egui::FontFamily::Monospace);
            let response = header.add(egui::Button::selectable(focused, icon));

            if response.hovered() {
                focus.point_at_header(index);
            }

            if response.on_hover_text(hint).clicked() {
                event = Some(*asked);
            }
        }
        ui.add_space(MIN_GAP * 0.5);
        focus.sync(view.entries.len(), 1);

        if view.entries.is_empty() {
            ui.label("No ROMs yet.");
            return;
        }

        egui::ScrollArea::vertical().show_viewport(ui, |ui, viewport| match view.layout {
            LibraryLayout::Shelf => shelf(ui, viewport, view, focus, covers, out),
            LibraryLayout::List => list(ui, viewport, view, focus, covers, out),
        });
    });

    event
}

/// The rows a viewport shows, with one of slack past its bottom edge — a row half in
/// view is a row to build. `pitch` is what one row and its gap take.
fn rows_in_view(viewport: Rect, pitch: f32, rows: usize) -> std::ops::Range<usize> {
    let last = ((viewport.max.y / pitch).ceil() as usize + 1).min(rows);
    // A viewport left below a shelf that has since grown shorter would otherwise
    // start past its end.
    let first = ((viewport.min.y / pitch).floor().max(0.0) as usize).min(last);

    first..last
}

/// Only the rows the viewport shows are built: a shelf walked whole costs its shape
/// building, its galleys and — the part that outlives the frame — a texture upload
/// per cover, for every cart in the library at once.
fn shelf(
    ui: &mut Ui,
    viewport: Rect,
    view: &LibraryView,
    focus: &mut LibraryFocus,
    covers: &mut TextureCache,
    out: &mut Vec<UiCmd>,
) {
    let follow_focus = focus.games.take_moved();
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
    let on_games = !focus.on_header;

    // What a row and its trailing gap took when every one of them was laid out in
    // turn: the spacing egui puts between items lands twice over, once after the row
    // and once after the gap.
    let pitch = cell.y + MIN_GAP + 2.0 * ui.spacing().item_spacing.y;
    let rows = view.entries.len().div_ceil(columns);
    // The whole shelf's height, so the scrollbar spans the library rather than the
    // few rows actually built.
    ui.set_height(pitch * rows as f32);
    let top = ui.max_rect().top();
    let built_rows = rows_in_view(viewport, pitch, rows);
    let (first, last) = (built_rows.start, built_rows.end);

    // Directional input can walk the focus onto a row that was never built and so has
    // no rect of its own to scroll to. Its band is enough — the shelf only scrolls
    // vertically, and the band is where the cart would be.
    if follow_focus {
        let row = focus.games.index() / columns;
        let band = Rect::from_min_size(
            egui::pos2(ui.max_rect().left(), top + row as f32 * pitch),
            cell,
        );
        ui.scroll_to_rect(band, None);
    }

    let built = Rect::from_x_y_ranges(
        ui.max_rect().x_range(),
        top + first as f32 * pitch..=top + last as f32 * pitch,
    );

    ui.scope_builder(egui::UiBuilder::new().max_rect(built), |ui| {
        // Each cart is a widget of its own, so the ids of the built rows are the ones
        // they would have had with the whole shelf ahead of them.
        ui.skip_ahead_auto_ids(first * columns);

        for row in first..last {
            let entries =
                &view.entries[row * columns..((row + 1) * columns).min(view.entries.len())];

            ui.horizontal(|ui| {
                ui.add_space(leading);
                ui.spacing_mut().item_spacing.x = gap;

                for (column, entry) in entries.iter().enumerate() {
                    let index = row * columns + column;
                    let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                    let focused = on_games && focus.games.is_focused(index);

                    // Pointer and directional input drive the same highlight, so the
                    // two never disagree about what is selected.
                    if response.hovered() {
                        focus.point_at_game(index);
                    }

                    if response.clicked() {
                        out.push(UiCmd::LaunchRom(index));
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
    });
}

/// How much taller the cart on a list row is than the thumbnail a plain row holds.
/// The cart is the point of the row — its cover art is what a library is read by — so
/// it is drawn big enough to see and the row is sized from it, not the other way round.
const LIST_CART_SCALE: f32 = 3.0;
/// The cart, and the row it sets the height of. Inset by [`ROW_PAD`] on every side it
/// has one — the same as its distance from the row's left edge, so a cart this large
/// does not read as wedged into its row.
const LIST_CART_HEIGHT: f32 = (THUMB_ROW_HEIGHT - THUMB_PAD * 2.0) * LIST_CART_SCALE;
const LIST_ROW_HEIGHT: f32 = LIST_CART_HEIGHT + ROW_PAD * 2.0;

/// One game per row: the cart, its title, and the play time behind it. Only the rows
/// in view are built, for the same reasons the shelf does it — see [`shelf`].
fn list(
    ui: &mut Ui,
    viewport: Rect,
    view: &LibraryView,
    focus: &mut LibraryFocus,
    covers: &mut TextureCache,
    out: &mut Vec<UiCmd>,
) {
    let follow_focus = focus.games.take_moved();
    // One game per row, so the grid the focus moves through is a single column.
    focus.sync(view.entries.len(), 1);
    let on_games = !focus.on_header;
    let pitch = LIST_ROW_HEIGHT + ui.spacing().item_spacing.y;
    // The whole list's height, so the scrollbar spans the library rather than the
    // few rows actually built.
    ui.set_height(pitch * view.entries.len() as f32);
    // Full width, so the rows line up with the header band over them and the play
    // time sits at the same edge the window does.
    let band_x = ui.max_rect().x_range();
    let top = ui.max_rect().top();
    let row_rect = |row: usize| {
        Rect::from_x_y_ranges(
            band_x,
            top + row as f32 * pitch..=top + (row as f32 * pitch + LIST_ROW_HEIGHT),
        )
    };

    // Directional input can walk the focus onto a row that was never built and so has
    // no rect of its own to scroll to; where the row would be is enough.
    if follow_focus {
        ui.scroll_to_rect(row_rect(focus.games.index()), None);
    }

    let built = rows_in_view(viewport, pitch, view.entries.len());
    let band = Rect::from_x_y_ranges(
        band_x,
        top + built.start as f32 * pitch..=top + built.end as f32 * pitch,
    );

    ui.scope_builder(egui::UiBuilder::new().max_rect(band), |ui| {
        // Each row is a widget of its own, so the ids of the built ones are the ones
        // they would have had with the whole list ahead of them.
        ui.skip_ahead_auto_ids(built.start);

        for index in built {
            let entry = &view.entries[index];

            if list_row(ui, entry, index, on_games, focus, covers) {
                out.push(UiCmd::LaunchRom(index));
            }
        }
    });
}

/// Returns whether the pointer clicked the row.
fn list_row(
    ui: &mut Ui,
    entry: &RomEntry,
    index: usize,
    on_games: bool,
    focus: &mut LibraryFocus,
    covers: &mut TextureCache,
) -> bool {
    let focused = on_games && focus.games.is_focused(index);
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), LIST_ROW_HEIGHT),
        Sense::click(),
    );

    // Pointer and directional input drive the same highlight, so the two never
    // disagree about what is selected.
    if response.hovered() {
        focus.point_at_game(index);
    }

    let bloom = theme::paint_focus(ui, response.id, rect, focused);
    let cover = entry
        .cover
        .as_ref()
        .map(|cover| covers.texture(ui, index, cover).clone());
    let cart = cart_rect(rect);
    // The row says what the game is called, so the cart's own label is left blank
    // rather than printing the title twice over.
    cart::paint(ui, cart, "", entry.kind, false, cover.as_ref());

    let mut text = rect.shrink2(Vec2::new(ROW_PAD, 0.0));
    text.set_left(cart.right() + ROW_PAD);
    // The play time goes down first, so the title knows how much room it was left
    // and gets cut rather than running under it.
    let played = theme::label(
        ui,
        text,
        Align2::RIGHT_CENTER,
        entry.played.as_str(),
        theme::detail_color(ui, bloom),
    );
    let mut title = text;
    title.set_right((text.right() - played - ROW_GAP).max(text.left()));
    // Heading-sized: beside a cart this tall, a row's plain text would read as a
    // caption rather than as the name of the thing.
    theme::heading_at(
        ui,
        title,
        Align2::LEFT_CENTER,
        entry.title.as_str(),
        theme::label_color(ui, bloom),
    );

    response.clicked()
}

/// The cart in the row's left end, at its own aspect: what [`ROW_PAD`] leaves of the
/// row's height, which is [`LIST_CART_HEIGHT`] by construction.
fn cart_rect(row: Rect) -> Rect {
    let height = row.height() - ROW_PAD * 2.0;
    let min = egui::pos2(row.left() + ROW_PAD, row.top() + ROW_PAD);

    Rect::from_min_size(min, Vec2::new(height / cart::ASPECT, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A viewport two rows tall, `from` rows down a shelf of rows one unit high.
    fn viewport(from: f32) -> Rect {
        Rect::from_min_size(egui::pos2(0.0, from), Vec2::new(100.0, 2.0))
    }

    #[test]
    fn the_rows_in_view_are_built_with_one_to_spare() {
        assert_eq!(rows_in_view(viewport(0.0), 1.0, 10), 0..3);
        assert_eq!(rows_in_view(viewport(4.5), 1.0, 10), 4..8);
    }

    /// The last row is the last one there is, however far the viewport reaches past it.
    #[test]
    fn the_shelf_never_builds_a_row_it_does_not_have() {
        assert_eq!(rows_in_view(viewport(8.0), 1.0, 10), 8..10);
        assert_eq!(rows_in_view(viewport(0.0), 1.0, 2), 0..2);
    }

    /// A shelf that has just grown shorter — a game removed, the window widened —
    /// leaves the viewport past its end until egui clamps the offset.
    #[test]
    fn a_viewport_past_the_end_builds_nothing_out_of_range() {
        let built = rows_in_view(viewport(40.0), 1.0, 10);

        assert!(built.start <= built.end, "{built:?} would panic as a range");
        assert!(built.end <= 10);
    }

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
            Some(LibraryPick::Header(LibraryEvent::Add))
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
            Some(LibraryPick::Header(LibraryEvent::Sort))
        );

        assert_eq!(focus.nav(NavAction::Right), None);
        assert_eq!(
            focus.nav(NavAction::Confirm),
            Some(LibraryPick::Header(LibraryEvent::ToggleLayout))
        );

        assert_eq!(focus.nav(NavAction::Right), None);
        assert_eq!(
            focus.nav(NavAction::Confirm),
            Some(LibraryPick::Header(LibraryEvent::OpenSettings))
        );
    }

    /// The button offers whichever layout is not in force, so its face is the one
    /// thing on the header that changes.
    #[test]
    fn the_layout_button_shows_the_layout_it_switches_to() {
        let shelf = header_icon(LibraryEvent::ToggleLayout, LibraryLayout::Shelf);
        let list = header_icon(LibraryEvent::ToggleLayout, LibraryLayout::List);

        assert_eq!(shelf.1, "List view");
        assert_eq!(list.1, "Shelf view");
        assert_ne!(shelf.0, list.0, "and shows it with its own glyph");
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
            Some(LibraryPick::Header(LibraryEvent::Add))
        );
        assert_eq!(focus.nav(NavAction::Down), None, "nowhere to go");
        assert_eq!(
            focus.nav(NavAction::Confirm),
            Some(LibraryPick::Header(LibraryEvent::Add))
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
