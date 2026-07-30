//! The cartridge tile: a Game Boy cart drawn from scratch, so a library needs no
//! assets at all. Cover art and screenshots go on the label later; the title text
//! is the bottom of that fallback chain.

use egui::{Align2, Color32, CornerRadius, FontId, Pos2, Rect, Stroke, Ui, Vec2};

/// Taller than wide, like the real cart.
pub const ASPECT: f32 = 1.18;
const SHELL_DMG: Color32 = Color32::from_rgb(0x9d, 0x9c, 0x92);
const SHELL_CGB: Color32 = Color32::from_rgb(0x6b, 0x5c, 0x8f);
const LABEL: Color32 = Color32::from_rgb(0xe4, 0xe1, 0xd4);
const LABEL_TEXT: Color32 = Color32::from_rgb(0x2b, 0x2a, 0x28);
const GRIP: Color32 = Color32::from_black_alpha(0x22);
const FOCUS: Color32 = Color32::from_rgb(0x7f, 0xc4, 0xff);
/// Fractions of the tile the shell's parts take up.
const NOTCH: f32 = 0.16;
const LABEL_INSET: f32 = 0.1;
const LABEL_TOP: f32 = 0.16;
const LABEL_HEIGHT: f32 = 0.52;
const GRIP_LINES: usize = 3;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum CartKind {
    Dmg,
    Cgb,
}

pub fn paint(ui: &Ui, rect: Rect, title: &str, kind: CartKind, focused: bool) {
    let painter = ui.painter();
    let unit = rect.width();
    let shell = match kind {
        CartKind::Dmg => SHELL_DMG,
        CartKind::Cgb => SHELL_CGB,
    };
    let radius = CornerRadius::same((unit * 0.06) as u8);
    painter.rect_filled(rect, radius, shell);

    // The clipped top-right corner that keeps a cart from going in backwards.
    let notch = unit * NOTCH;
    painter.add(egui::Shape::convex_polygon(
        vec![
            Pos2::new(rect.right() - notch, rect.top()),
            Pos2::new(rect.right(), rect.top()),
            Pos2::new(rect.right(), rect.top() + notch),
        ],
        ui.visuals().panel_fill,
        Stroke::NONE,
    ));

    let inset = unit * LABEL_INSET;
    let label = Rect::from_min_size(
        rect.min + Vec2::new(inset, unit * LABEL_TOP),
        Vec2::new(rect.width() - inset * 2.0, rect.height() * LABEL_HEIGHT),
    );
    painter.rect_filled(label, radius, LABEL);
    painter.text(
        label.center(),
        Align2::CENTER_CENTER,
        elide(title, label.width(), unit),
        FontId::proportional(unit * 0.11),
        LABEL_TEXT,
    );

    let grip_top = label.bottom() + unit * 0.12;
    let spacing = unit * 0.09;
    for line in 0..GRIP_LINES {
        let y = grip_top + spacing * line as f32;
        if y > rect.bottom() - inset {
            break;
        }
        painter.hline(
            (rect.left() + inset)..=(rect.right() - inset),
            y,
            Stroke::new(unit * 0.02, GRIP),
        );
    }

    if focused {
        painter.rect_stroke(
            rect.expand(unit * 0.03),
            radius,
            Stroke::new(unit * 0.035, FOCUS),
            egui::StrokeKind::Outside,
        );
    }
}

/// Rough character budget from the label width — the real fit is measured by egui
/// when the text is laid out, this only keeps long names from spilling.
fn elide(title: &str, label_width: f32, unit: f32) -> String {
    let max_chars = (label_width / (unit * 0.062)).floor() as usize;

    if title.chars().count() <= max_chars {
        return title.to_owned();
    }

    let mut out: String = title.chars().take(max_chars.saturating_sub(1)).collect();
    out.push('…');

    out
}
