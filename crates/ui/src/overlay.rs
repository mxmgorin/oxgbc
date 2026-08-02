//! The chrome every overlay screen shares: a centred popup over the dimmed
//! game, and the plain row list that goes inside it.

use crate::nav::GridFocus;
use crate::theme::{self, OVERLAY_DIM, POPUP_RADIUS, ROW_GAP, ROW_HEIGHT, ROW_PAD};
use egui::{Align, Align2, Layout, Rect, Sense, StrokeKind, Ui, UiBuilder, Vec2};

/// Height a popup has to reserve for one line of title above its rows.
pub(crate) fn title_height() -> f32 {
    ROW_HEIGHT + ROW_GAP
}

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
/// Painted by hand rather than through `Frame::popup`: a frame takes one fill, and
/// the popup is a plate — a fill under a ramp and two lit edges.
pub(crate) fn popup(root: &mut Ui, size: Vec2, add: impl FnOnce(&mut Ui)) {
    let screen = root.ctx().content_rect();
    // `size` is what the caller's rows add up to, so the plate grows *around* it —
    // shrinking the content into `size` instead would leave the last row hanging
    // over the bottom edge by the padding.
    let content = Rect::from_center_size(screen.center(), size);
    let panel = content.expand(ROW_PAD);
    let visuals = root.visuals();
    let (shadow, face, stroke) = (
        visuals.popup_shadow,
        visuals.window_fill,
        visuals.window_stroke,
    );
    let painter = root.painter();
    painter.rect_filled(screen, 0.0, OVERLAY_DIM);
    painter.add(shadow.as_shape(panel, POPUP_RADIUS));
    theme::plate(painter, panel, POPUP_RADIUS, face);
    painter.rect_stroke(panel, POPUP_RADIUS, stroke, StrokeKind::Inside);

    let mut ui = root.new_child(
        UiBuilder::new()
            .max_rect(content)
            .layout(Layout::top_down(Align::Center)),
    );
    ui.spacing_mut().item_spacing.y = ROW_GAP;
    add(&mut ui);
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
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::click());
        let focused = focus.is_focused(index);

        // Pointer and directional input drive the same highlight, so the two
        // never disagree about what is selected.
        if response.hovered() {
            focus.focus(index);
        }

        if response.clicked() {
            clicked = Some(index);
        }

        let bloom = theme::paint_focus(ui, response.id, rect, focused);
        let text = rect.shrink2(Vec2::new(ROW_PAD, 0.0));
        theme::label(
            ui,
            text,
            Align2::CENTER_CENTER,
            label,
            theme::label_color(ui, bloom),
        );
    }

    clicked
}
