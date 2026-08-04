//! Walking the device's own storage, for platforms whose own picker cannot be
//! reached the way the app is driven: a desktop dialog wants a pointer, and a build
//! without one has no picker at all. Where the system picker *is* a screen like any
//! other — Android's — the platform uses that instead and this is never opened.
//!
//! The walking itself stays on the platform side; this only draws what it reports
//! and says which row was picked.

use crate::nav::GridFocus;
use crate::overlay;
use crate::theme::{self, ROW_GAP, ROW_HEIGHT, ROW_PAD, WIDTH_PANEL};
use egui::{Align2, ScrollArea, Sense, Ui, Vec2};

/// What is being picked, which is all this side needs to know: a folder walk has a
/// row for taking the one it is standing in.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub enum BrowseTarget {
    #[default]
    File,
    Dir,
}

/// Where the walk is and what is in it.
#[derive(Default)]
pub struct BrowseView {
    pub target: BrowseTarget,
    /// Where this is, for the heading.
    pub path: String,
    /// In the order they are shown; the platform leads with the way up.
    pub entries: Vec<BrowseEntry>,
}

pub struct BrowseEntry {
    pub name: String,
    pub is_dir: bool,
}

/// What activating a row leads to. Going up is entering the row the platform put
/// first, so it needs no case of its own.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum BrowsePick {
    Enter(usize),
    ChooseDir,
}

const MAX_ROWS: usize = 10;
const TITLE_HEIGHT: f32 = ROW_HEIGHT + ROW_GAP;
const CHOOSE_DIR: &str = "Use this folder";

pub fn row_count(view: &BrowseView) -> usize {
    view.entries.len() + usize::from(view.target == BrowseTarget::Dir)
}

/// Where row `index` leads; the same order [`show`] paints.
pub fn pick(view: &BrowseView, index: usize) -> Option<BrowsePick> {
    if index < view.entries.len() {
        return Some(BrowsePick::Enter(index));
    }

    (index < row_count(view)).then_some(BrowsePick::ChooseDir)
}

pub fn show(root: &mut Ui, view: &BrowseView, focus: &mut GridFocus) -> Option<BrowsePick> {
    let count = row_count(view);
    focus.sync(count, 1);
    let follow_focus = focus.take_moved();
    let list = overlay::rows_height(count.max(1)).min(overlay::rows_height(MAX_ROWS));
    let mut picked = None;

    overlay::popup(root, Vec2::new(WIDTH_PANEL, TITLE_HEIGHT + list), |ui| {
        theme::heading(ui, &view.path);

        ScrollArea::vertical().show(ui, |ui| {
            for (index, entry) in view.entries.iter().enumerate() {
                // A trailing slash is the only thing that tells a folder from a
                // game here, and it is what a path looks like anyway.
                let label = if entry.is_dir {
                    format!("{}/", entry.name)
                } else {
                    entry.name.clone()
                };

                if show_row(ui, &label, index, focus, follow_focus) {
                    picked = Some(BrowsePick::Enter(index));
                }
            }

            if view.target == BrowseTarget::Dir
                && show_row(ui, CHOOSE_DIR, view.entries.len(), focus, follow_focus)
            {
                picked = Some(BrowsePick::ChooseDir);
            }
        });
    });

    picked
}

/// Returns whether the pointer clicked the row.
fn show_row(
    ui: &mut Ui,
    label: &str,
    index: usize,
    focus: &mut GridFocus,
    follow_focus: bool,
) -> bool {
    let focused = focus.is_focused(index);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click());

    // Pointer and directional input drive the same highlight, so the two never
    // disagree about what is selected.
    if response.hovered() {
        focus.focus(index);
    }

    let bloom = theme::paint_focus(ui, response.id, rect, focused);

    // Directional input can walk the highlight out of the list; bring it back.
    if focused && follow_focus {
        ui.scroll_to_rect(rect, None);
    }

    theme::label(
        ui,
        rect.shrink2(egui::vec2(ROW_PAD, 0.0)),
        Align2::LEFT_CENTER,
        label,
        theme::label_color(ui, bloom),
    );

    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view(target: BrowseTarget) -> BrowseView {
        BrowseView {
            target,
            path: "/roms".to_owned(),
            entries: vec![
                BrowseEntry {
                    name: "..".to_owned(),
                    is_dir: true,
                },
                BrowseEntry {
                    name: "Zelda.gbc".to_owned(),
                    is_dir: false,
                },
            ],
        }
    }

    #[test]
    fn picking_a_rom_offers_only_the_entries() {
        let roms = view(BrowseTarget::File);

        assert_eq!(row_count(&roms), 2);
        assert_eq!(pick(&roms, 0), Some(BrowsePick::Enter(0)));
        assert_eq!(pick(&roms, 1), Some(BrowsePick::Enter(1)));
        assert_eq!(pick(&roms, 2), None);
    }

    #[test]
    fn picking_a_folder_adds_a_row_for_taking_this_one() {
        let dirs = view(BrowseTarget::Dir);

        assert_eq!(row_count(&dirs), 3);
        assert_eq!(pick(&dirs, 2), Some(BrowsePick::ChooseDir));
        assert_eq!(pick(&dirs, 3), None);
    }
}
