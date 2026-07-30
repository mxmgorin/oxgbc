//! The cartridge tile: a Game Boy Game Pak drawn from scratch, so a library needs
//! no assets at all. Cover art and screenshots go on the label later; the title
//! text is the bottom of that fallback chain.
//!
//! Shell colour follows the cart header's CGB flag, the way Nintendo shipped them:
//! grey for monochrome games, black for CGB games that still run on a DMG, clear
//! for CGB-only ones.

use egui::text::LayoutJob;
use egui::{Align, Color32, FontId, Pos2, Rect, Shape, Stroke, Ui, Vec2};

/// Taller than wide, like the real cart.
pub const ASPECT: f32 = 1.17;

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub enum CartKind {
    /// Monochrome game: grey shell.
    Dmg,
    /// CGB game that still runs on a DMG: black shell.
    CgbCompatible,
    /// CGB-only game: clear shell over a green board.
    CgbOnly,
}

struct Shell {
    body: Color32,
    shade: Color32,
    highlight: Color32,
    emboss_text: Color32,
    board: Option<Color32>,
    brand: &'static str,
}

const LABEL: Color32 = Color32::from_rgb(0xef, 0xeb, 0xe0);
const LABEL_EDGE: Color32 = Color32::from_rgb(0xb9, 0xb4, 0xa8);
const LABEL_TEXT: Color32 = Color32::from_rgb(0x26, 0x25, 0x21);
const PINS: Color32 = Color32::from_rgb(0xc2, 0x9f, 0x3d);
const SHADOW: Color32 = Color32::from_black_alpha(0x4d);
const FOCUS: Color32 = Color32::from_rgb(0x7f, 0xc4, 0xff);

/// Everything below is a fraction of the tile's width, measured off photos of the
/// real carts, so the drawing scales with the shelf.
const CORNER_TOP: f32 = 0.07;
const CORNER_BOTTOM: f32 = 0.05;
/// The cut corner that stops a cart going in the wrong way round.
const CHAMFER: f32 = 0.11;
const RIDGES: usize = 5;
const RIDGE_TOP: f32 = 0.055;
const RIDGE_BOTTOM: f32 = 0.16;
const BRAND_TOP: f32 = 0.045;
const BRAND_BOTTOM: f32 = 0.17;
const BRAND_INSET: f32 = 0.20;
const RECESS_INSET: f32 = 0.10;
const RECESS_TOP: f32 = 0.28;
const RECESS_BOTTOM: f32 = 1.02;
const LABEL_MARGIN: f32 = 0.022;
const ARROW_HALF_WIDTH: f32 = 0.05;
const ARROW_BOTTOM: f32 = 1.11;
const BOARD_INSET: f32 = 0.055;
const PIN_STRIP_TOP: f32 = 1.05;
const PINS_COUNT: usize = 16;
const FOCUS_RING: f32 = 0.03;

pub fn paint(ui: &Ui, rect: Rect, title: &str, kind: CartKind, focused: bool) {
    let painter = ui.painter();
    let w = rect.width();
    let shell = shell_of(kind);
    let outline = shell_outline(rect, w);

    let shadow = shell_outline(rect.translate(Vec2::new(w * 0.015, w * 0.03)), w);
    painter.add(Shape::convex_polygon(shadow, SHADOW, Stroke::NONE));
    painter.add(Shape::convex_polygon(
        outline,
        shell.body,
        Stroke::new(w * 0.008, shell.shade),
    ));

    // Clear shells show the board and the edge connector through the plastic.
    if let Some(board) = shell.board {
        let board_rect = Rect::from_min_max(
            at(rect, w, BOARD_INSET, BOARD_INSET),
            at(rect, w, 1.0 - BOARD_INSET, PIN_STRIP_TOP),
        );
        painter.rect_filled(board_rect, w * 0.02, board);
        paint_pins(ui, rect, w);
    }

    // Lit from above, like the shelf photo: a bevel under the top edge and a
    // shaded one over the bottom keep the plastic from reading as flat.
    painter.line_segment(
        [
            at(rect, w, CORNER_TOP, 0.014),
            at(rect, w, 1.0 - CHAMFER, 0.014),
        ],
        Stroke::new(w * 0.012, shell.highlight),
    );
    painter.line_segment(
        [
            at(rect, w, CORNER_BOTTOM, ASPECT - 0.014),
            at(rect, w, 1.0 - CORNER_BOTTOM, ASPECT - 0.014),
        ],
        Stroke::new(w * 0.012, shell.shade),
    );

    paint_ridges(ui, rect, w, &shell);
    paint_brand(ui, rect, w, &shell);
    paint_label(ui, rect, w, &shell, title);

    // An opaque shell shows the moulded arrow where a clear one shows its pins.
    if shell.board.is_none() {
        paint_arrow(ui, rect, w, &shell);
    }

    if focused {
        let ring = shell_outline(rect.expand(w * FOCUS_RING), w);
        painter.add(Shape::convex_polygon(
            ring,
            Color32::TRANSPARENT,
            Stroke::new(w * 0.035, FOCUS),
        ));
    }
}

fn shell_of(kind: CartKind) -> Shell {
    match kind {
        CartKind::Dmg => Shell {
            body: Color32::from_rgb(0xa7, 0xa2, 0x9a),
            shade: Color32::from_rgb(0x87, 0x82, 0x7a),
            highlight: Color32::from_rgb(0xc2, 0xbd, 0xb4),
            emboss_text: Color32::from_rgb(0x8c, 0x88, 0x80),
            board: None,
            brand: "GAME BOY",
        },
        CartKind::CgbCompatible => Shell {
            body: Color32::from_rgb(0x2c, 0x2c, 0x2e),
            shade: Color32::from_rgb(0x16, 0x16, 0x18),
            highlight: Color32::from_rgb(0x4d, 0x4d, 0x51),
            emboss_text: Color32::from_rgb(0x55, 0x55, 0x59),
            board: None,
            brand: "GAME BOY",
        },
        CartKind::CgbOnly => Shell {
            body: Color32::from_rgb(0x6d, 0x66, 0x7c),
            shade: Color32::from_rgb(0x44, 0x3f, 0x52),
            highlight: Color32::from_rgb(0x94, 0x8d, 0xa6),
            emboss_text: Color32::from_rgb(0x8b, 0x84, 0x9c),
            board: Some(Color32::from_rgb(0x33, 0x52, 0x3f)),
            brand: "GAME BOY COLOR",
        },
    }
}

/// Rounded rectangle with the top-right corner cut off, as one convex path.
fn shell_outline(rect: Rect, w: f32) -> Vec<Pos2> {
    let top = w * CORNER_TOP;
    let bottom = w * CORNER_BOTTOM;
    let chamfer = w * CHAMFER;
    let mut path = Vec::new();

    arc(&mut path, rect.left_top() + Vec2::new(top, top), top, 180.0);
    path.push(Pos2::new(rect.right() - chamfer, rect.top()));
    path.push(Pos2::new(rect.right(), rect.top() + chamfer));
    arc(
        &mut path,
        rect.right_bottom() + Vec2::new(-bottom, -bottom),
        bottom,
        0.0,
    );
    arc(
        &mut path,
        rect.left_bottom() + Vec2::new(bottom, -bottom),
        bottom,
        90.0,
    );

    path
}

/// Quarter circle in screen space (y grows downwards), so 0° points right, 90°
/// down and 180° left, and the sweep runs clockwise around the outline.
fn arc(path: &mut Vec<Pos2>, center: Pos2, radius: f32, start_deg: f32) {
    const STEPS: usize = 6;

    for step in 0..=STEPS {
        let rad = (start_deg + 90.0 * step as f32 / STEPS as f32).to_radians();
        path.push(center + Vec2::new(rad.cos(), rad.sin()) * radius);
    }
}

/// Point at `(x, y)` given in fractions of the tile's width.
fn at(rect: Rect, w: f32, x: f32, y: f32) -> Pos2 {
    rect.min + Vec2::new(w * x, w * y)
}

fn paint_ridges(ui: &Ui, rect: Rect, w: f32, shell: &Shell) {
    const GROUPS: [(f32, f32); 2] = [(0.055, 0.15), (0.80, 0.90)];
    let painter = ui.painter();

    for (from, to) in GROUPS {
        let step = (to - from) / RIDGES as f32;

        for ridge in 0..RIDGES {
            let x = from + step * (ridge as f32 + 0.5);
            painter.line_segment(
                [at(rect, w, x, RIDGE_TOP), at(rect, w, x, RIDGE_BOTTOM)],
                Stroke::new(w * 0.012, shell.shade),
            );
            painter.line_segment(
                [
                    at(rect, w, x + step * 0.28, RIDGE_TOP),
                    at(rect, w, x + step * 0.28, RIDGE_BOTTOM),
                ],
                Stroke::new(w * 0.008, shell.highlight),
            );
        }
    }
}

/// The embossed oval with the console's name in it.
fn paint_brand(ui: &Ui, rect: Rect, w: f32, shell: &Shell) {
    let painter = ui.painter();
    let brand = Rect::from_min_max(
        at(rect, w, BRAND_INSET, BRAND_TOP),
        at(rect, w, 1.0 - BRAND_INSET, BRAND_BOTTOM),
    );
    let radius = brand.height() * 0.5;
    painter.rect_filled(brand, radius, shell.shade);
    painter.rect_stroke(
        brand.shrink(w * 0.008),
        radius,
        Stroke::new(w * 0.006, shell.highlight),
        egui::StrokeKind::Inside,
    );
    painter.text(
        brand.center(),
        egui::Align2::CENTER_CENTER,
        shell.brand,
        FontId::proportional(w * 0.055),
        shell.emboss_text,
    );
}

/// The recessed area and the paper label inside it, with the title as art of last
/// resort.
fn paint_label(ui: &Ui, rect: Rect, w: f32, shell: &Shell, title: &str) {
    let painter = ui.painter();
    let recess = Rect::from_min_max(
        at(rect, w, RECESS_INSET, RECESS_TOP),
        at(rect, w, 1.0 - RECESS_INSET, RECESS_BOTTOM),
    );
    painter.rect_filled(recess, w * 0.02, shell.shade);

    let label = recess.shrink(w * LABEL_MARGIN);
    painter.rect_filled(label, w * 0.012, LABEL);
    painter.rect_stroke(
        label,
        w * 0.012,
        Stroke::new(w * 0.006, LABEL_EDGE),
        egui::StrokeKind::Inside,
    );

    let text = label.shrink(w * 0.05);
    let mut job = LayoutJob::simple(
        title.to_owned(),
        FontId::proportional(w * 0.1),
        LABEL_TEXT,
        text.width(),
    );
    job.halign = Align::Center;
    job.wrap.max_rows = ((text.height() / (w * 0.13)) as usize).max(1);
    job.wrap.overflow_character = Some('…');
    job.wrap.break_anywhere = true;
    let galley = painter.layout_job(job);
    let pos = Pos2::new(text.center().x, text.center().y - galley.size().y * 0.5);
    painter.galley(pos, galley, LABEL_TEXT);
}

fn paint_arrow(ui: &Ui, rect: Rect, w: f32, shell: &Shell) {
    let tip = at(rect, w, 0.5, ARROW_BOTTOM);
    let half = w * ARROW_HALF_WIDTH;
    let top = tip.y - half * 0.75;

    ui.painter().add(Shape::convex_polygon(
        vec![
            Pos2::new(tip.x - half, top),
            Pos2::new(tip.x + half, top),
            tip,
        ],
        shell.shade,
        Stroke::NONE,
    ));
}

fn paint_pins(ui: &Ui, rect: Rect, w: f32) {
    let painter = ui.painter();
    let strip = Rect::from_min_max(
        at(rect, w, 0.17, PIN_STRIP_TOP),
        at(rect, w, 0.83, PIN_STRIP_TOP + 0.06),
    );
    painter.rect_filled(strip, 0.0, PINS.gamma_multiply(0.55));

    let step = strip.width() / PINS_COUNT as f32;
    for pin in 0..PINS_COUNT {
        let x = strip.left() + step * (pin as f32 + 0.5);
        painter.line_segment(
            [
                Pos2::new(x, strip.top()),
                Pos2::new(x, strip.bottom() - strip.height() * 0.2),
            ],
            Stroke::new(step * 0.45, PINS),
        );
    }
}
