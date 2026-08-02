//! What can be done with a cartridge's cover.
//!
//! The rows depend on what the cart has: there is nothing to remove without a
//! cover, and nothing to pick from without a save state. Any state will do — one
//! saved before screens were kept beside them still has its screen inside it.

use crate::nav::GridFocus;
use crate::overlay;
use crate::theme::{self, WIDTH_SHEET};
use egui::{Ui, Vec2};

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum CoverAction {
    /// Pick a picture off the disk — the platform owns the dialog.
    UseFile,
    /// Go on to choose which of the game's states to take the screen from.
    UseState,
    Remove,
}

/// What this cart makes possible, which is what decides the rows.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Default)]
pub struct CoverOffer {
    pub has_cover: bool,
    pub has_states: bool,
}

fn rows(offer: CoverOffer) -> impl Iterator<Item = (&'static str, CoverAction)> {
    [
        Some(("Use file…", CoverAction::UseFile)),
        offer
            .has_states
            .then_some(("Use save state", CoverAction::UseState)),
        offer.has_cover.then_some(("Remove", CoverAction::Remove)),
    ]
    .into_iter()
    .flatten()
}

pub fn action_count(offer: CoverOffer) -> usize {
    rows(offer).count()
}

pub fn action_at(offer: CoverOffer, index: usize) -> Option<CoverAction> {
    rows(offer).nth(index).map(|(_, action)| action)
}

pub fn show_actions(
    root: &mut Ui,
    title: &str,
    offer: CoverOffer,
    focus: &mut GridFocus,
) -> Option<CoverAction> {
    let count = action_count(offer);
    focus.sync(count, 1);
    let height = overlay::title_height() + overlay::rows_height(count);
    let mut clicked = None;

    overlay::popup(root, Vec2::new(WIDTH_SHEET, height), |ui| {
        theme::heading(ui, title);
        clicked = overlay::rows(ui, rows(offer).map(|(label, _)| label), focus);
    });

    clicked.and_then(|index| action_at(offer, index))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(offer: CoverOffer) -> Vec<&'static str> {
        rows(offer).map(|(label, _)| label).collect()
    }

    #[test]
    fn a_bare_cart_can_only_be_given_a_file() {
        let bare = CoverOffer::default();

        assert_eq!(labels(bare), ["Use file…"]);
        assert_eq!(action_at(bare, 0), Some(CoverAction::UseFile));
        assert_eq!(action_at(bare, 1), None);
    }

    #[test]
    fn a_cover_is_what_makes_removing_possible() {
        let covered = CoverOffer {
            has_cover: true,
            has_states: false,
        };

        assert_eq!(labels(covered), ["Use file…", "Remove"]);
        assert_eq!(action_at(covered, 1), Some(CoverAction::Remove));
    }

    #[test]
    fn a_state_of_any_age_adds_the_middle_row() {
        let both = CoverOffer {
            has_cover: true,
            has_states: true,
        };

        assert_eq!(labels(both), ["Use file…", "Use save state", "Remove"]);
        assert_eq!(action_at(both, 1), Some(CoverAction::UseState));
        assert_eq!(action_at(both, 2), Some(CoverAction::Remove));
    }
}
