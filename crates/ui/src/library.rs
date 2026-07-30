//! The library screen: the game collection as a shelf of cartridges.

use crate::cart::{self, CartKind};
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
const GAP: f32 = 18.0;
/// The focused cart grows into its gap instead of pushing the grid around.
const FOCUS_SCALE: f32 = 1.08;

/// Takes egui's root [`Ui`] (what panels are shown into), so the same screen
/// works under any backend that can run egui.
pub fn library(root: &mut Ui, view: &LibraryView, focus: &mut GridFocus) {
    egui::CentralPanel::default().show(root, |ui| {
        ui.heading("Library");
        ui.add_space(GAP * 0.5);

        if view.entries.is_empty() {
            ui.label("No ROMs yet.");
            focus.sync(0, 1);
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| shelf(ui, view, focus));
    });
}

fn shelf(ui: &mut Ui, view: &LibraryView, focus: &mut GridFocus) {
    let tile = Vec2::new(TILE_WIDTH, TILE_WIDTH * cart::ASPECT);
    let columns = (((ui.available_width() + GAP) / (tile.x + GAP)).floor() as usize).max(1);
    focus.sync(view.entries.len(), columns);

    for (row, entries) in view.entries.chunks(columns).enumerate() {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = GAP;

            for (column, entry) in entries.iter().enumerate() {
                let index = row * columns + column;
                let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
                let focused = focus.is_focused(index);

                // Pointer and directional input drive the same highlight, so the
                // two never disagree about what is selected.
                if response.hovered() {
                    focus.focus(index);
                }

                cart::paint(ui, cart_rect(rect, focused), &entry.title, entry.kind, focused);
            }
        });
        ui.add_space(GAP);
    }
}

fn cart_rect(rect: Rect, focused: bool) -> Rect {
    if focused {
        Rect::from_center_size(rect.center(), rect.size() * FOCUS_SCALE)
    } else {
        rect
    }
}
