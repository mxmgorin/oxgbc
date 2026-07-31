//! The library screen: the game collection as a shelf of cartridges.

use crate::cart::{self, CartKind};
use crate::menu::UiCmd;
use crate::nav::GridFocus;
use egui::{Rect, Sense, Ui, Vec2};

/// Read-model the platform fills in; the screen never reaches into app state.
pub struct LibraryView<'a> {
    pub entries: &'a [RomEntry],
}

pub struct RomEntry {
    pub title: String,
    pub kind: CartKind,
}

const TILE_WIDTH: f32 = 132.0;
/// Smallest gap between carts; the shelf widens it to use up the row's slack,
/// but never past `MAX_GAP_TILES` of a cart's width.
const MIN_GAP: f32 = 14.0;
const MAX_GAP_TILES: f32 = 0.5;
const FOCUS_SCALE: f32 = 1.08;
/// Room the focus ring needs outside the cart, as a fraction of its width.
const RING: f32 = 0.06;

/// Takes egui's root [`Ui`] (what panels are shown into), so the same screen
/// works under any backend that can run egui.
pub fn library(root: &mut Ui, view: &LibraryView, focus: &mut GridFocus, out: &mut Vec<UiCmd>) {
    egui::CentralPanel::default().show(root, |ui| {
        ui.heading("Library");
        ui.add_space(MIN_GAP * 0.5);

        if view.entries.is_empty() {
            ui.label("No ROMs yet.");
            focus.sync(0, 1);
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| shelf(ui, view, focus, out));
    });
}

fn shelf(ui: &mut Ui, view: &LibraryView, focus: &mut GridFocus, out: &mut Vec<UiCmd>) {
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

                let size = if focused { tile * FOCUS_SCALE } else { tile };
                let cart = Rect::from_center_size(rect.center(), size);
                cart::paint(ui, cart, &entry.title, entry.kind, focused);
            }
        });
        ui.add_space(MIN_GAP);
    }
}
