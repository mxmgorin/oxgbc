//! The library screen: the game collection as a shelf of cartridges, and what can
//! be done with one besides playing it.

use crate::cart::{self, CartKind};
use crate::image::{RgbImage, TextureCache};
use crate::menu::UiCmd;
use crate::nav::GridFocus;
use crate::overlay;
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
const SHEET_WIDTH: f32 = 260.0;

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

    overlay::popup(root, Vec2::new(SHEET_WIDTH, height), |ui| {
        ui.heading(title);
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

/// What the header's buttons asked for. Both are pointer-only — the shelf's focus
/// walks carts, not chrome — so a gamepad reaches the same things through the pause
/// overlay and the settings page.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum LibraryEvent {
    OpenSettings,
    AddRom,
}

/// Takes egui's root [`Ui`] (what panels are shown into), so the same screen
/// works under any backend that can run egui. Returns what the header asked for —
/// switching screen is the menu's business, not this one's.
pub fn library(
    root: &mut Ui,
    view: &LibraryView,
    focus: &mut GridFocus,
    covers: &mut TextureCache,
    out: &mut Vec<UiCmd>,
) -> Option<LibraryEvent> {
    let mut event = None;
    covers.sync(view.version);

    egui::CentralPanel::default().show(root, |ui| {
        ui.horizontal(|ui| {
            ui.heading("Library");
            // Right to left, so the first one added sits furthest right.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, "\u{2699}", "Settings") {
                    event = Some(LibraryEvent::OpenSettings);
                }

                if icon_button(ui, "\u{2795}", "Add game…") {
                    event = Some(LibraryEvent::AddRom);
                }
            });
        });
        ui.add_space(MIN_GAP * 0.5);

        if view.entries.is_empty() {
            ui.label("No ROMs yet.");
            focus.sync(0, 1);
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| shelf(ui, view, focus, covers, out));
    });

    event
}

/// The glyph carries the meaning, so the words go in the tooltip. Both icons come
/// from the font egui ships with — this crate has no assets of its own.
fn icon_button(ui: &mut Ui, glyph: &str, hint: &str) -> bool {
    let icon = egui::RichText::new(glyph).size(ICON_SIZE);

    ui.button(icon).on_hover_text(hint).clicked()
}

fn shelf(
    ui: &mut Ui,
    view: &LibraryView,
    focus: &mut GridFocus,
    covers: &mut TextureCache,
    out: &mut Vec<UiCmd>,
) {
    let follow_focus = focus.take_moved();
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

    for (row, entries) in view.entries.chunks(columns).enumerate() {
        ui.horizontal(|ui| {
            ui.add_space(leading);
            ui.spacing_mut().item_spacing.x = gap;

            for (column, entry) in entries.iter().enumerate() {
                let index = row * columns + column;
                let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                let focused = focus.is_focused(index);

                // Pointer and directional input drive the same highlight, so the
                // two never disagree about what is selected.
                if response.hovered() {
                    focus.focus(index);
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
