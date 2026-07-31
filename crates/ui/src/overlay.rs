//! The chrome every overlay screen shares: a centred popup over the dimmed
//! game, and the plain row list that goes inside it.

use crate::nav::GridFocus;
use egui::{Align, Color32, Frame, Layout, Rect, Ui, UiBuilder, Vec2};

pub(crate) const OVERLAY_DIM: Color32 = Color32::from_black_alpha(0xb4);
pub(crate) const ROW_HEIGHT: f32 = 32.0;
pub(crate) const ROW_GAP: f32 = 6.0;

/// Height a popup has to reserve for `count` plain rows, heading excluded.
pub(crate) fn rows_height(count: usize) -> f32 {
    rows_height_of(count, ROW_HEIGHT)
}

/// Same, for a screen whose rows are taller than a plain one.
pub(crate) fn rows_height_of(count: usize, row_height: f32) -> f32 {
    count as f32 * (row_height + ROW_GAP) + ROW_GAP
}

/// Dims the game and centres a popup of exactly `size` on it.
///
/// Sized by the caller rather than left to egui: an auto-sized panel settles a
/// layout pass late, and both passes land on screen as a ghosted double image.
pub(crate) fn popup(root: &mut Ui, size: Vec2, add: impl FnOnce(&mut Ui)) {
    let screen = root.ctx().content_rect();
    root.painter().rect_filled(screen, 0.0, OVERLAY_DIM);
    let panel = Rect::from_center_size(screen.center(), size);
    let mut ui = root.new_child(
        UiBuilder::new()
            .max_rect(panel)
            .layout(Layout::top_down(Align::Center)),
    );

    Frame::popup(ui.style()).show(&mut ui, |ui| {
        ui.spacing_mut().item_spacing.y = ROW_GAP;
        add(ui);
    });
}

/// Full-width rows with the focused one highlighted; returns the row the pointer
/// clicked, if any.
pub(crate) fn rows<'a>(
    ui: &mut Ui,
    labels: impl Iterator<Item = &'a str>,
    focus: &mut GridFocus,
) -> Option<usize> {
    let mut clicked = None;

    for (index, label) in labels.enumerate() {
        let row = egui::Button::selectable(focus.is_focused(index), label);
        let response = ui.add_sized([ui.available_width(), ROW_HEIGHT], row);

        // Pointer and directional input drive the same highlight, so the two
        // never disagree about what is selected.
        if response.hovered() {
            focus.focus(index);
        }

        if response.clicked() {
            clicked = Some(index);
        }
    }

    clicked
}
