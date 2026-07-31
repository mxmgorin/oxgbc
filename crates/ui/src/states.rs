//! The save-state screens: the game's slots as a list, and what one slot can be
//! used for once it is picked.
//!
//! An empty slot has a single use, so picking it writes a state straight away.
//! A slot with a state in it has three, which is what the action sheet is for.

use crate::menu::UiCmd;
use crate::nav::GridFocus;
use crate::overlay::{self, ROW_GAP, ROW_HEIGHT};
use egui::{
    Align, ColorImage, Layout, Rect, ScrollArea, Sense, TextureFilter, TextureHandle,
    TextureOptions, Ui, UiBuilder, Vec2,
};
use std::collections::HashMap;

/// What the platform found on disk; how it reads is this side's business.
#[derive(Default)]
pub struct StatesView {
    /// Occupied slots, in slot order.
    pub slots: Vec<StateSlot>,
    /// Lowest slot holding nothing, `None` once every slot is taken.
    pub free: Option<usize>,
    /// Bumped every time the platform rebuilds this view, which is the signal to
    /// throw away textures uploaded from the screens it replaced.
    pub version: u64,
}

pub struct StateSlot {
    pub slot: usize,
    /// What the user called it; empty leaves the slot going by its number.
    pub name: String,
    /// Platform-formatted age of the state, e.g. `"3 min ago"`.
    pub saved: String,
    /// The screen this state was saved with, when the platform had one to hand.
    pub shot: Option<StateShot>,
}

/// A saved screen as tightly packed RGB8 — raw pixels rather than a texture, since
/// only this side has an egui context to upload through.
pub struct StateShot {
    /// `width * height * 3` bytes long.
    pub rgb: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

/// Screens uploaded to the GPU, kept between frames: uploading per frame would
/// leave a texture behind every frame. Dropped wholesale when the view changes,
/// since that is when what a slot holds can have changed too.
#[derive(Default)]
pub struct ShotCache {
    version: u64,
    textures: HashMap<usize, TextureHandle>,
}

impl ShotCache {
    fn texture(&mut self, ui: &Ui, slot: usize, shot: &StateShot) -> &TextureHandle {
        self.textures.entry(slot).or_insert_with(|| {
            let image = ColorImage::from_rgb([shot.width, shot.height], &shot.rgb);

            // Crisp when blown up for the sheet, smooth when shrunk into a row:
            // nearest-neighbour minification of pixel art just drops pixels.
            let options = TextureOptions {
                magnification: TextureFilter::Nearest,
                minification: TextureFilter::Linear,
                ..Default::default()
            };

            ui.ctx()
                .load_texture(format!("{SHOT_TEXTURE}-{slot}"), image, options)
        })
    }

    fn sync(&mut self, version: u64) {
        if self.version != version {
            self.version = version;
            self.textures.clear();
        }
    }
}

/// Where picking a list row leads.
#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum RowPick {
    /// An empty slot: writing a state is the only thing to do with it.
    Create(usize),
    /// A slot to open the action sheet for.
    Open(usize),
}

impl RowPick {
    pub fn slot(self) -> usize {
        match self {
            RowPick::Create(slot) | RowPick::Open(slot) => slot,
        }
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum SlotAction {
    Load,
    Overwrite,
    Rename,
    Delete,
}

/// The sheet's rows: what each says and what it does to the slot. Delete sits
/// last, furthest from the actions that keep the state.
const ACTIONS: [(&str, SlotAction); 4] = [
    ("Load", SlotAction::Load),
    ("Overwrite", SlotAction::Overwrite),
    ("Rename", SlotAction::Rename),
    ("Delete", SlotAction::Delete),
];

const LIST_WIDTH: f32 = 380.0;
const SHEET_WIDTH: f32 = 220.0;
const SHOT_TEXTURE: &str = "save-state-shot";
/// Pixel art only survives whole-number scaling, so the preview is drawn at 1x or
/// 2x — never in between — and only goes to 2x while it stays under this much of
/// the window's height.
const SHOT_MAX_SCALE: f32 = 2.0;
const SHOT_MAX_SCREEN: f32 = 0.4;
/// A list row is taller than a plain one to make room for a thumbnail.
const LIST_ROW_HEIGHT: f32 = 48.0;
const THUMB_PAD: f32 = 4.0;
/// Rows shown before the list starts scrolling.
const MAX_ROWS: usize = 6;
/// Room reserved for one line of title, which sits outside the row list.
const TITLE_HEIGHT: f32 = ROW_HEIGHT + ROW_GAP;
const ROW_ROUNDING: f32 = 4.0;
const ROW_PAD: f32 = 8.0;

/// A row of the list: where it leads, and what the slot holds.
struct ListRow<'a> {
    pick: RowPick,
    /// The state in the slot; `None` for the empty one.
    state: Option<&'a StateSlot>,
}

/// A named state goes by its name, an unnamed one by its slot number.
fn label_of(state: &StateSlot) -> String {
    if state.name.is_empty() {
        format!("Slot {}", state.slot)
    } else {
        state.name.clone()
    }
}

pub fn row_count(view: &StatesView) -> usize {
    rows(view).count()
}

/// Where row `index` leads; the same order [`show`] paints.
pub fn pick(view: &StatesView, index: usize) -> Option<RowPick> {
    rows(view).nth(index).map(|row| row.pick)
}

pub fn action_count() -> usize {
    ACTIONS.len()
}

pub fn action_at(index: usize) -> Option<SlotAction> {
    ACTIONS.get(index).map(|(_, action)| *action)
}

/// `None` for [`SlotAction::Rename`], which opens a screen of its own rather than
/// asking the platform for anything.
pub fn action_cmd(action: SlotAction, slot: usize) -> Option<UiCmd> {
    match action {
        SlotAction::Load => Some(UiCmd::LoadState(slot)),
        SlotAction::Overwrite => Some(UiCmd::SaveState(slot)),
        SlotAction::Delete => Some(UiCmd::DeleteState(slot)),
        SlotAction::Rename => None,
    }
}

/// The free slot leads, so saving a new state stays one press away.
fn rows(view: &StatesView) -> impl Iterator<Item = ListRow<'_>> {
    let fresh = view.free.map(|slot| ListRow {
        pick: RowPick::Create(slot),
        state: None,
    });

    fresh
        .into_iter()
        .chain(view.slots.iter().map(|state| ListRow {
            pick: RowPick::Open(state.slot),
            state: Some(state),
        }))
}

fn slot_state(view: &StatesView, slot: usize) -> Option<&StateSlot> {
    view.slots.iter().find(|state| state.slot == slot)
}

/// What the slot's state is called, empty when it is unnamed or gone.
pub fn slot_name(view: &StatesView, slot: usize) -> &str {
    slot_state(view, slot).map_or("", |state| state.name.as_str())
}

pub fn show(
    root: &mut Ui,
    view: &StatesView,
    focus: &mut GridFocus,
    shots: &mut ShotCache,
) -> Option<RowPick> {
    let count = row_count(view);
    focus.sync(count, 1);
    shots.sync(view.version);
    let follow_focus = focus.take_moved();
    let rows_height = |count| overlay::rows_height_of(count, LIST_ROW_HEIGHT);
    let list = rows_height(count.max(1)).min(rows_height(MAX_ROWS));
    let mut picked = None;

    overlay::popup(root, Vec2::new(LIST_WIDTH, TITLE_HEIGHT + list), |ui| {
        ui.heading("Save states");

        if count == 0 {
            ui.label("No save states yet.");
            return;
        }

        ScrollArea::vertical().show(ui, |ui| {
            for (index, row) in rows(view).enumerate() {
                if show_row(ui, &row, index, focus, follow_focus, shots) {
                    picked = Some(row.pick);
                }
            }
        });
    });

    picked
}

/// Returns whether the pointer clicked the row.
fn show_row(
    ui: &mut Ui,
    row: &ListRow,
    index: usize,
    focus: &mut GridFocus,
    follow_focus: bool,
    shots: &mut ShotCache,
) -> bool {
    let focused = focus.is_focused(index);
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), LIST_ROW_HEIGHT),
        Sense::click(),
    );

    // Pointer and directional input drive the same highlight, so the two never
    // disagree about what is selected.
    if response.hovered() {
        focus.focus(index);
    }

    if focused {
        ui.painter()
            .rect_filled(rect, ROW_ROUNDING, ui.visuals().selection.bg_fill);

        // Directional input can walk the highlight out of the list; bring it back.
        if follow_focus {
            ui.scroll_to_rect(rect, None);
        }
    }

    let (label, detail) = match row.state {
        Some(state) => (label_of(state), state.saved.clone()),
        None => ("New state".to_owned(), format!("slot {}", row.pick.slot())),
    };
    let mut text = rect.shrink2(egui::vec2(ROW_PAD, 0.0));

    // The thumbnail takes the left of the row and the text starts after it, so
    // rows without one still line their labels up with the rest.
    if let Some((state, shot)) = row
        .state
        .and_then(|s| s.shot.as_ref().map(|shot| (s, shot)))
    {
        let thumb = thumb_rect(rect, shot);
        let texture = shots.texture(ui, state.slot, shot);
        ui.painter().image(
            texture.id(),
            thumb,
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        text.set_left(thumb.right() + ROW_PAD);
    }

    let mut row_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(text)
            .layout(Layout::left_to_right(Align::Center)),
    );
    row_ui.label(label);
    row_ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.weak(detail));

    response.clicked()
}

/// The thumbnail sits inside the row's left edge, at the screen's own aspect.
fn thumb_rect(row: Rect, shot: &StateShot) -> Rect {
    let height = row.height() - THUMB_PAD * 2.0;
    let width = height * shot.width as f32 / shot.height as f32;
    let min = egui::pos2(row.left() + THUMB_PAD, row.top() + THUMB_PAD);

    Rect::from_min_size(min, Vec2::new(width, height))
}

pub fn show_actions(
    root: &mut Ui,
    view: &StatesView,
    slot: usize,
    focus: &mut GridFocus,
    shots: &mut ShotCache,
) -> Option<SlotAction> {
    focus.sync(action_count(), 1);
    shots.sync(view.version);
    let state = slot_state(view, slot);
    let shot = state.and_then(|state| state.shot.as_ref());
    let preview = shot.map(|shot| shot_size(root.ctx().content_rect().size(), shot));
    // Title plus, when the slot was found, the line of detail under it.
    let titles = 1 + usize::from(state.is_some());
    let height = TITLE_HEIGHT * titles as f32
        + preview.map_or(0.0, |size| size.y + ROW_GAP)
        + overlay::rows_height(action_count());
    // Wide enough for the preview, since the sheet is otherwise narrower than one.
    let width = preview.map_or(SHEET_WIDTH, |size| size.x.max(SHEET_WIDTH));
    let mut clicked = None;

    overlay::popup(root, Vec2::new(width, height), |ui| {
        match state {
            // A named state says which slot it is in, since the name replaced it.
            Some(state) if !state.name.is_empty() => {
                ui.heading(&state.name);
                ui.weak(format!("Slot {slot} · {}", state.saved));
            }
            Some(state) => {
                ui.heading(format!("Slot {slot}"));
                ui.weak(&state.saved);
            }
            None => {
                ui.heading(format!("Slot {slot}"));
            }
        }

        if let (Some(shot), Some(size)) = (shot, preview) {
            let texture = shots.texture(ui, slot, shot);
            ui.image(egui::load::SizedTexture::new(texture.id(), size));
        }

        clicked = overlay::rows(ui, ACTIONS.iter().map(|(label, _)| *label), focus);
    });

    clicked.and_then(action_at)
}

/// The name being typed, plus whether the field still has to take focus — egui
/// only routes the keyboard to a widget that has it.
#[derive(Default)]
pub struct RenameEdit {
    text: String,
    grab: bool,
}

impl RenameEdit {
    /// Starts from the slot's current name, so a rename is an edit rather than
    /// retyping.
    pub fn start(&mut self, name: &str) {
        self.text = name.to_owned();
        self.grab = true;
    }

    pub fn take_text(&mut self) -> String {
        std::mem::take(&mut self.text)
    }
}

pub enum RenameEvent {
    Commit,
    Cancel,
}

pub fn show_rename(root: &mut Ui, slot: usize, edit: &mut RenameEdit) -> Option<RenameEvent> {
    let height = TITLE_HEIGHT * 2.0 + overlay::rows_height(1);
    let mut event = None;

    overlay::popup(root, Vec2::new(LIST_WIDTH, height), |ui| {
        ui.heading(format!("Name slot {slot}"));
        let field = ui.add_sized(
            [ui.available_width(), ROW_HEIGHT],
            egui::TextEdit::singleline(&mut edit.text).hint_text("Leave empty to go by number"),
        );

        if std::mem::take(&mut edit.grab) {
            field.request_focus();
        }

        // Enter commits. Losing focus any other way — Escape, a click elsewhere —
        // leaves the name alone, so nothing is renamed by walking away.
        if field.lost_focus() {
            event = Some(if ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                RenameEvent::Commit
            } else {
                RenameEvent::Cancel
            });
        }

        ui.weak("Enter to save, Esc to cancel");
    });

    event
}

/// Whole-number scaling only; 2x when the window can spare the height, 1x when it
/// cannot.
fn shot_size(screen: Vec2, shot: &StateShot) -> Vec2 {
    let native = Vec2::new(shot.width as f32, shot.height as f32);
    let fits = (screen.y * SHOT_MAX_SCREEN / native.y).floor();

    native * fits.clamp(1.0, SHOT_MAX_SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> StatesView {
        StatesView {
            slots: vec![
                StateSlot {
                    slot: 0,
                    name: String::new(),
                    saved: "2 min ago".to_owned(),
                    shot: None,
                },
                StateSlot {
                    slot: 3,
                    name: "before the boss".to_owned(),
                    saved: "1 d ago".to_owned(),
                    shot: None,
                },
            ],
            free: Some(1),
            version: 1,
        }
    }

    #[test]
    fn the_free_slot_leads_the_list() {
        assert_eq!(row_count(&view()), 3);
        assert_eq!(pick(&view(), 0), Some(RowPick::Create(1)));
        assert_eq!(pick(&view(), 1), Some(RowPick::Open(0)));
        assert_eq!(pick(&view(), 2), Some(RowPick::Open(3)));
        assert_eq!(pick(&view(), 3), None);
    }

    #[test]
    fn a_full_shelf_of_slots_lists_only_states() {
        let full = StatesView {
            free: None,
            ..view()
        };

        assert_eq!(row_count(&full), 2);
        assert_eq!(pick(&full, 0), Some(RowPick::Open(0)));
    }

    #[test]
    fn nothing_saved_yet_offers_just_the_first_slot() {
        let empty = StatesView {
            slots: Vec::new(),
            free: Some(0),
            version: 1,
        };

        assert_eq!(row_count(&empty), 1);
        assert_eq!(pick(&empty, 0), Some(RowPick::Create(0)));
    }

    #[test]
    fn every_action_maps_to_its_own_command() {
        assert_eq!(action_at(0), Some(SlotAction::Load));
        assert_eq!(action_at(3), Some(SlotAction::Delete));
        assert_eq!(action_at(action_count()), None);
        assert_eq!(action_cmd(SlotAction::Load, 4), Some(UiCmd::LoadState(4)));
        assert_eq!(
            action_cmd(SlotAction::Overwrite, 4),
            Some(UiCmd::SaveState(4))
        );
        assert_eq!(
            action_cmd(SlotAction::Delete, 4),
            Some(UiCmd::DeleteState(4))
        );
    }

    /// Renaming is the menu's own business, so it asks the platform for nothing.
    #[test]
    fn renaming_has_no_command_of_its_own() {
        assert_eq!(action_cmd(SlotAction::Rename, 4), None);
    }

    #[test]
    fn a_missing_slot_has_no_name_to_edit() {
        assert_eq!(slot_name(&view(), 3), "before the boss");
        assert_eq!(slot_name(&view(), 0), "");
        assert_eq!(slot_name(&view(), 7), "");
    }

    #[test]
    fn the_sheet_looks_up_the_slot_it_is_for() {
        assert_eq!(
            slot_state(&view(), 3).map(label_of).as_deref(),
            Some("before the boss")
        );
        assert!(slot_state(&view(), 1).is_none());
    }

    #[test]
    fn an_unnamed_state_goes_by_its_slot_number() {
        let slots = &view().slots;

        assert_eq!(label_of(&slots[0]), "Slot 0");
        assert_eq!(label_of(&slots[1]), "before the boss");
    }
}
