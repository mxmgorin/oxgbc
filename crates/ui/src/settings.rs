//! The settings page. The rows are described by the platform rather than known
//! here: this crate can't see the app's config types, and a data-driven page also
//! lets each platform drop the controls it has no backend for.

use crate::menu::UiCmd;
use crate::nav::GridFocus;
use crate::theme::{self, ROW_PAD, SETTINGS_ROW_HEIGHT, WIDTH_PAGE};
use egui::{Align, Layout, Ui};

/// Identifies a row to the platform that produced it; opaque on this side.
pub type SettingId = u16;

/// Owned rather than borrowed: the platform rebuilds this only when the config
/// changes, and owning it keeps both sides free of lifetime plumbing.
#[derive(Default)]
pub struct SettingsView {
    pub sections: Vec<Section>,
}

pub struct Section {
    pub title: String,
    pub rows: Vec<Row>,
}

pub struct Row {
    pub id: SettingId,
    pub label: String,
    pub control: Control,
}

pub enum Control {
    Toggle(bool),
    /// `◀ value ▶`: the platform formats the value and applies the step.
    Stepper(String),
    Action,
}

/// Room kept for a stepper's value, wide enough for the longest one a row shows.
const VALUE_WIDTH: f32 = theme::UNIT * 38.0;
/// The whole control column: the value plus the two steppers around it. A row's
/// label is cut at this, so a long one can never run under its own control.
const CONTROL_WIDTH: f32 = VALUE_WIDTH + theme::UNIT * 20.0;

/// Flattened row count, which is what the focus model moves through.
pub fn row_count(view: &SettingsView) -> usize {
    view.sections.iter().map(|s| s.rows.len()).sum()
}

pub fn row_at(view: &SettingsView, index: usize) -> Option<&Row> {
    view.sections.iter().flat_map(|s| s.rows.iter()).nth(index)
}

pub fn settings(root: &mut Ui, view: &SettingsView, focus: &mut GridFocus, out: &mut Vec<UiCmd>) {
    focus.sync(row_count(view), 1);
    let follow_focus = focus.take_moved();

    egui::CentralPanel::default().show(root, |ui| {
        theme::page(ui);
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
            .show(ui, |ui| {
                // A fixed-width column, centred: rows read badly stretched across a
                // fullscreen window, and left-aligned they hug one edge of it.
                // Centring by a leading `horizontal` instead would give the column a
                // single row's height budget, and the page would stop scrolling.
                ui.vertical_centered(|ui| {
                    ui.set_max_width(WIDTH_PAGE);
                    theme::heading(ui, "Settings");
                    let mut index = 0;

                    for section in &view.sections {
                        ui.add_space(SETTINGS_ROW_HEIGHT * 0.5);
                        ui.label(egui::RichText::new(&section.title).strong());
                        ui.separator();

                        for row in &section.rows {
                            show_row(ui, row, index, focus, follow_focus, out);
                            index += 1;
                        }
                    }
                });
            });
    });
}

fn show_row(
    ui: &mut Ui,
    row: &Row,
    index: usize,
    focus: &mut GridFocus,
    follow_focus: bool,
    out: &mut Vec<UiCmd>,
) {
    let focused = focus.is_focused(index);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), SETTINGS_ROW_HEIGHT),
        egui::Sense::hover(),
    );

    if response.hovered() {
        focus.focus(index);
    }

    let bloom = theme::paint_focus(ui, response.id, rect, focused);

    // Directional input can walk the highlight off-screen; bring it back.
    if focused && follow_focus {
        ui.scroll_to_rect(rect, Some(Align::Center));
    }

    // The label is painted and the controls stay widgets, so the label's room has to
    // be reserved rather than negotiated: it ends where the control column starts.
    let mut label_rect = rect.shrink2(egui::vec2(ROW_PAD, 0.0));
    label_rect.set_right((label_rect.right() - CONTROL_WIDTH).max(label_rect.left()));
    theme::label(
        ui,
        label_rect,
        egui::Align2::LEFT_CENTER,
        &row.label,
        theme::label_color(ui, bloom),
    );

    let mut controls = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(egui::vec2(ROW_PAD, 0.0)))
            .layout(Layout::right_to_left(Align::Center)),
    );
    show_control(&mut controls, row, out);
}

fn show_control(ui: &mut Ui, row: &Row, out: &mut Vec<UiCmd>) {
    match &row.control {
        Control::Toggle(on) => {
            if ui.button(if *on { "On" } else { "Off" }).clicked() {
                out.push(UiCmd::Setting {
                    id: row.id,
                    step: 1,
                });
            }
        }
        Control::Stepper(value) => {
            if ui.small_button("▶").clicked() {
                out.push(UiCmd::Setting {
                    id: row.id,
                    step: 1,
                });
            }

            ui.allocate_ui_with_layout(
                egui::vec2(VALUE_WIDTH, ui.available_height()),
                Layout::centered_and_justified(egui::Direction::LeftToRight),
                |ui| ui.label(value),
            );

            if ui.small_button("◀").clicked() {
                out.push(UiCmd::Setting {
                    id: row.id,
                    step: -1,
                });
            }
        }
        Control::Action => {
            if ui.button("Apply").clicked() {
                out.push(UiCmd::Setting {
                    id: row.id,
                    step: 1,
                });
            }
        }
    }
}
