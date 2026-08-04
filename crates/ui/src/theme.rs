//! The one place the look is decided: the palette, the length scale and the text
//! ramp, installed on the context once so no screen has to pick a colour or a size
//! of its own.
//!
//! The colours are the app's own art rather than anything invented here: the
//! neutrals and the accent come from the icon's oxide palette, and the text is the
//! cream of a cartridge label in [`crate::cart`] — so the shelf and the chrome
//! around it read as one object.

use egui::style::{Selection, WidgetVisuals};
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Color32, ColorImage, CornerRadius, FontId, Id, Margin, Mesh, Painter, Pos2, Rect,
    Sense, Shadow, Shape, Stroke, Style, TextStyle, TextureFilter, TextureHandle, TextureOptions,
    Ui, Vec2, Visuals,
};

pub struct Palette {
    /// The faceplate the whole app is machined out of — see [`page`]. Dark enough
    /// that the shelf's light grey cartridges keep their 7:1 against it.
    pub bg: Color32,
    /// Pages and popups over [`Palette::bg`].
    pub surface: Color32,
    /// Widgets on a surface: buttons, fields, rows.
    pub raised: Color32,
    pub hover: Color32,
    pub outline: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    /// Focus and selection, the one thing on screen that is not a neutral.
    ///
    /// Lifted out of the icon's rust until dark text over it clears 4.5:1: this app
    /// is driven from a gamepad across a room as often as from a pointer at a desk,
    /// and the focused row is what has to be found from there.
    pub accent: Color32,
    /// The rust as the icon has it, dark enough to sit under the accent as the
    /// pressed state.
    pub accent_low: Color32,
    /// Text over [`Palette::accent`], which is dark — cream over rust reaches only
    /// 3.7:1.
    pub on_accent: Color32,
}

pub const OXIDE: Palette = Palette {
    bg: Color32::from_rgb(0x1e, 0x1a, 0x18),
    surface: Color32::from_rgb(0x26, 0x22, 0x20),
    raised: Color32::from_rgb(0x33, 0x2e, 0x2a),
    hover: Color32::from_rgb(0x44, 0x3e, 0x39),
    outline: Color32::from_rgb(0x57, 0x4a, 0x41),
    text: Color32::from_rgb(0xef, 0xeb, 0xe0),
    text_weak: Color32::from_rgb(0x9a, 0x8f, 0x84),
    accent: Color32::from_rgb(0xc9, 0x74, 0x3f),
    accent_low: Color32::from_rgb(0x5f, 0x3d, 0x2d),
    on_accent: Color32::from_rgb(0x12, 0x10, 0x0f),
};

/// What the game behind an overlay is taken down to.
pub const OVERLAY_DIM: Color32 = Color32::from_black_alpha(0xb4);

/// Every length on screen is a multiple of this, so the screens keep one rhythm
/// and scaling the UI moves all of it together.
pub const UNIT: f32 = 4.0;

/// A plain row of an overlay or a list.
pub const ROW_HEIGHT: f32 = UNIT * 8.0;
/// A settings row, denser because a page of them scrolls.
pub const SETTINGS_ROW_HEIGHT: f32 = UNIT * 7.0;
/// A row with a thumbnail in it.
pub const THUMB_ROW_HEIGHT: f32 = UNIT * 12.0;
pub const ROW_GAP: f32 = UNIT * 1.5;
/// A row's text inset from its edge.
pub const ROW_PAD: f32 = UNIT * 2.0;
pub const ROW_RADIUS: f32 = UNIT;
pub const POPUP_RADIUS: f32 = UNIT * 2.0;

/// The three widths a screen may be: a list of short actions, a list of named
/// things, a page. Seven hand-picked widths disagreeing by a few pixels each read
/// as accidental rather than as a layout.
pub const WIDTH_SHEET: f32 = UNIT * 65.0;
pub const WIDTH_PANEL: f32 = UNIT * 100.0;
pub const WIDTH_PAGE: f32 = UNIT * 115.0;

/// Bigger than egui's stock 13/18: see [`Palette::accent`] on where this is read
/// from.
const HEADING: f32 = 24.0;
const BODY: f32 = 15.0;
const SMALL: f32 = 12.0;
const MONOSPACE: f32 = 14.0;

/// A popup sits over a game that is still visible behind it, and a dim alone left
/// the two on one plane.
const POPUP_SHADOW: Shadow = Shadow {
    offset: [0, 8],
    blur: 24,
    spread: 0,
    color: Color32::from_black_alpha(0x8c),
};

/// Installs `palette` on `ctx`. Called once per backend at startup, before any
/// screen is shown.
pub fn apply(ctx: &egui::Context, palette: &Palette) {
    let mut style = Style {
        visuals: visuals(palette),
        ..Default::default()
    };
    style.text_styles = [
        (TextStyle::Heading, FontId::proportional(HEADING)),
        (TextStyle::Body, FontId::proportional(BODY)),
        (TextStyle::Button, FontId::proportional(BODY)),
        (TextStyle::Small, FontId::proportional(SMALL)),
        (TextStyle::Monospace, FontId::monospace(MONOSPACE)),
    ]
    .into();
    style.spacing.item_spacing = Vec2::splat(ROW_GAP);
    style.spacing.button_padding = Vec2::new(ROW_PAD, UNIT);
    style.spacing.menu_margin = Margin::same(ROW_PAD as i8);
    // Nothing on these screens is text to select, and a selectable label takes
    // `Sense::click_and_drag()` — laid over a row it swallows the row's own clicks,
    // which is what a justified one did to the pause overlay's mouse.
    style.interaction.selectable_labels = false;

    // Both themes get the same style: the app has one look, and following the system
    // into a stock light theme would only undo it.
    ctx.all_styles_mut(|theme| *theme = style.clone());
}

/// How far a surface is lit and shaded across its own height, and how hard its two
/// edges catch the light. Kept low: the material has to be legible at the edges and
/// invisible in the middle, or the chrome turns into a skin.
const SHEEN: Color32 = white_alpha(0x12);
const FALL: Color32 = Color32::from_black_alpha(0x12);
const EDGE_LIT: Color32 = white_alpha(0x26);
const EDGE_DARK: Color32 = Color32::from_black_alpha(0x4a);
const EDGE_WIDTH: f32 = 1.0;
/// How far a groove is sunk below whatever it is cut into. An alpha rather than a
/// colour: a groove is "darker than here", and on the page there is nothing darker
/// than the background left to name.
const GROOVE_CUT: Color32 = Color32::from_black_alpha(0x4d);

/// `Color32::from_white_alpha` is not `const`, and these are.
const fn white_alpha(a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(a, a, a, a)
}

/// A raised surface: what the chrome is made of. Lit from above and shaded below,
/// the same rule [`crate::cart`] lights a cartridge shell by, so the shelf and the
/// panels around it are one material.
pub fn plate(painter: &Painter, rect: Rect, radius: f32, face: Color32) {
    plate_faded(painter, rect, radius, face, 1.0);
}

/// The same plate part-way in. Every one of its colours fades together, so a
/// half-bloomed chip is a fainter plate and not a plate with its edges already on.
pub(crate) fn plate_faded(painter: &Painter, rect: Rect, radius: f32, face: Color32, fade: f32) {
    painter.rect_filled(rect, radius, face.gamma_multiply(fade));
    ramp(
        painter,
        rect,
        SHEEN.gamma_multiply(fade),
        FALL.gamma_multiply(fade),
    );
    edges(
        painter,
        rect,
        radius,
        EDGE_LIT.gamma_multiply(fade),
        EDGE_DARK.gamma_multiply(fade),
    );
}

/// The same surface cut into rather than laid on: the light lands on the far wall,
/// so the plate's ramp and edges are upside down.
pub fn groove(painter: &Painter, rect: Rect, radius: f32) {
    painter.rect_filled(rect, radius, GROOVE_CUT);
    ramp(painter, rect, FALL, SHEEN);
    edges(painter, rect, radius, EDGE_DARK, EDGE_LIT);
}

/// Side of the noise tile, in texels, and how far its darkest and lightest texels
/// go. The tile is small enough to cost nothing and large enough that its repeat
/// has no structure to notice.
const GRAIN_TILE: usize = 128;
const GRAIN_DEPTH: u8 = 0x10;
/// A floor under the tile's on-screen size, so a display reporting an absurd scale
/// cannot turn the page into a million quads.
const GRAIN_MIN: f32 = 32.0;

/// The faceplate the screens sit on: the window as one large plate, lit down its
/// height and grained.
///
/// The grain is the load-bearing part. A ramp this size on its own reads as a
/// gradient; what makes a surface this large read as metal is the texture.
pub fn page(ui: &Ui) {
    let rect = ui.ctx().content_rect();
    ramp(ui.painter(), rect, SHEEN, FALL);
    paint_grain(ui, rect);
}

/// The tile, made once and kept on the context: it never changes, and it outlives
/// every view model the screens throw away.
fn grain(ctx: &egui::Context) -> TextureHandle {
    let id = egui::Id::new("theme::grain");

    if let Some(tile) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return tile;
    }

    let tile = ctx.load_texture(
        "theme::grain",
        grain_tile(),
        TextureOptions {
            magnification: TextureFilter::Nearest,
            minification: TextureFilter::Nearest,
            ..Default::default()
        },
    );
    ctx.data_mut(|data| data.insert_temp(id, tile.clone()));

    tile
}

fn grain_tile() -> ColorImage {
    let mut pixels = Vec::with_capacity(GRAIN_TILE * GRAIN_TILE);

    for y in 0..GRAIN_TILE {
        for x in 0..GRAIN_TILE {
            let noise = hash(x, y);
            // Half the texels lift and half sink, so the grain is texture in the
            // surface rather than a film over it.
            let depth = (noise >> 4) * GRAIN_DEPTH / 0xf;
            pixels.push(if noise & 1 == 0 {
                white_alpha(depth)
            } else {
                Color32::from_black_alpha(depth)
            });
        }
    }

    ColorImage::new([GRAIN_TILE, GRAIN_TILE], pixels)
}

/// A hash, not a random source: the grain has to come out the same on every run and
/// every platform, and an RNG dependency for 16 KB of noise is not worth it.
fn hash(x: usize, y: usize) -> u8 {
    let mut bits = (x as u32)
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add((y as u32).wrapping_mul(0x85eb_ca6b));
    bits ^= bits >> 15;
    bits = bits.wrapping_mul(0x2545_f491);
    bits ^= bits >> 13;

    bits as u8
}

/// One quad per tile, each reading the whole texture, rather than one quad whose uv
/// runs past 1: repeating is the renderer's business, and these screens have to come
/// out the same under every backend that can run egui — SDL2's canvas hands uv
/// straight to `SDL_RenderGeometry`, which does not wrap.
fn paint_grain(ui: &Ui, rect: Rect) {
    let tile = (GRAIN_TILE as f32 / ui.ctx().pixels_per_point()).max(GRAIN_MIN);
    let mut mesh = Mesh::with_texture(grain(ui.ctx()).id());
    let mut top = rect.top();

    while top < rect.bottom() {
        let bottom = (top + tile).min(rect.bottom());
        let mut left = rect.left();

        while left < rect.right() {
            let right = (left + tile).min(rect.right());
            let quad = Rect::from_min_max(Pos2::new(left, top), Pos2::new(right, bottom));
            // A tile cut off by the page's edge reads only the part of itself it got
            // room for.
            let uv = Rect::from_min_max(
                Pos2::ZERO,
                Pos2::new((right - left) / tile, (bottom - top) / tile),
            );
            mesh.add_rect_with_uv(quad, uv, Color32::WHITE);
            left = right;
        }

        top = bottom;
    }

    ui.painter().add(Shape::mesh(mesh));
}

/// A heading in its own groove.
///
/// The letters keep the full text colour: engraving them would mean cutting cream
/// into the surface, which lands near 1.1:1 and cannot be read. What carries the
/// material is the band, not the text in it.
pub fn heading(ui: &mut Ui, text: impl ToString) {
    let band = heading_band(ui);
    heading_in(ui, band, Align2::CENTER_CENTER, text);
}

/// The title of a band the caller has already cut, for a header sharing one with
/// buttons on its far end. Returns the width it took, so a header can set something
/// beside its title.
pub fn heading_in(ui: &Ui, band: Rect, anchor: Align2, text: impl ToString) -> f32 {
    let color = ui.visuals().text_color();

    heading_at(
        ui,
        band.shrink2(Vec2::new(ROW_PAD, 0.0)),
        anchor,
        text,
        color,
    )
}

/// A heading in a colour of the caller's, for one that fades.
pub fn heading_at(ui: &Ui, rect: Rect, anchor: Align2, text: impl ToString, color: Color32) -> f32 {
    let font = TextStyle::Heading.resolve(ui.style());

    label_with(ui, rect, anchor, text, font, color)
}

/// Small print in a row's second-line tone, for text set beside a heading.
pub fn detail(ui: &Ui, rect: Rect, anchor: Align2, text: impl ToString) {
    detail_at(ui, rect, anchor, text, detail_color(ui, 0.0));
}

/// Small print in a colour of the caller's, for one that fades.
pub fn detail_at(ui: &Ui, rect: Rect, anchor: Align2, text: impl ToString, color: Color32) {
    let font = TextStyle::Small.resolve(ui.style());

    label_with(ui, rect, anchor, text, font, color);
}

/// The band on its own, for a header carrying more than its title.
pub fn heading_band(ui: &mut Ui) -> Rect {
    let width = ui.available_width();
    let (band, _) = ui.allocate_exact_size(Vec2::new(width, ROW_HEIGHT), Sense::hover());
    groove(ui.painter(), band, ROW_RADIUS);

    band
}

/// The highlight a focused row gets: the accent as a plate, so focus is a piece of
/// the same material and not a sticker over it.
///
/// Blooms in over [`FOCUS_BLOOM`] rather than snapping, which is what makes it read
/// as the cell oxidising under the focus. Call it on every row, focused or not — a
/// row dropped from the focus has to be given its frames to rust back off.
///
/// Returns how far in the bloom is, which is also what the row's own text has to
/// follow: see [`label_color`].
pub fn paint_focus(ui: &Ui, id: Id, rect: Rect, focused: bool) -> f32 {
    let bloom = ui.ctx().animate_bool_with_time(id, focused, FOCUS_BLOOM);

    if bloom > 0.0 {
        let face = ui.visuals().selection.bg_fill;
        plate_faded(ui.painter(), rect, ROW_RADIUS, face, bloom);
    }

    bloom
}

/// A wash from `top` down to nothing and on to `bottom`. epaint interpolates vertex
/// colours, so one quad per half is the whole ramp — no bands and no texture.
fn ramp(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let middle = rect.center().y;
    let (upper, lower) = (
        Rect::from_x_y_ranges(rect.x_range(), rect.top()..=middle),
        Rect::from_x_y_ranges(rect.x_range(), middle..=rect.bottom()),
    );
    gradient(painter, upper, top, Color32::TRANSPARENT);
    gradient(painter, lower, Color32::TRANSPARENT, bottom);
}

fn gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(2, 1, 3);
    painter.add(Shape::mesh(mesh));
}

/// The hairlines along the top and bottom faces. Held back from the rounded ends by
/// the radius, so neither pokes out of a corner.
fn edges(painter: &Painter, rect: Rect, radius: f32, top: Color32, bottom: Color32) {
    let (left, right) = (rect.left() + radius, rect.right() - radius);
    let half = EDGE_WIDTH * 0.5;

    for (y, color) in [(rect.top() + half, top), (rect.bottom() - half, bottom)] {
        painter.line_segment(
            [Pos2::new(left, y), Pos2::new(right, y)],
            Stroke::new(EDGE_WIDTH, color),
        );
    }
}

/// How long the rust takes to bloom under a row the focus has just reached.
const FOCUS_BLOOM: f32 = 0.16;
/// How far a second line is let down from the first, on metal and on rust alike.
const DETAIL_FADE: f32 = 0.7;

/// A single line of text, painted rather than laid out as a widget.
///
/// Deliberately flat. A copy of the glyph offset by a pixel is optically a blur and
/// reads as one — relief needs a hard light/dark pair either side of the stroke, and
/// there is no room for that at these sizes. On this UI relief lives on the shapes,
/// [`plate`] and [`groove`] and the focus chip, whose edges are placed exactly.
///
/// `rect` is the room the text has; `anchor` places it in there and anything longer
/// is cut with an ellipsis rather than run past the edge. Returns the width it took,
/// so a row can put a second text beside the first without the two colliding.
pub fn label(ui: &Ui, rect: Rect, anchor: Align2, text: impl ToString, color: Color32) -> f32 {
    let font = TextStyle::Body.resolve(ui.style());

    label_with(ui, rect, anchor, text, font, color)
}

fn label_with(
    ui: &Ui,
    rect: Rect,
    anchor: Align2,
    text: impl ToString,
    font: FontId,
    color: Color32,
) -> f32 {
    let mut job = LayoutJob::simple_singleline(text.to_string(), font, Color32::PLACEHOLDER);
    job.wrap = TextWrapping {
        max_width: rect.width(),
        max_rows: 1,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let painter = ui.painter();
    let galley = painter.layout_job(job);
    let at = anchor.align_size_within_rect(galley.size(), rect).min;
    let width = galley.size().x;
    painter.galley(at, galley, color);

    width
}

/// What a row's text has to be at this much bloom: the plate's own text colour where
/// there is no rust yet, and the dark that reads over rust where there is. Lerped
/// rather than switched, so no frame of the transition is unreadable.
pub fn label_color(ui: &Ui, bloom: f32) -> Color32 {
    let visuals = ui.visuals();

    visuals
        .text_color()
        // Whatever `apply` installed as the palette's `on_accent`; egui keeps a
        // selected widget's text colour here.
        .lerp_to_gamma(visuals.selection.stroke.color, bloom)
}

/// The same for a row's second line, which stays quieter on either material.
pub fn detail_color(ui: &Ui, bloom: f32) -> Color32 {
    label_color(ui, bloom).gamma_multiply(DETAIL_FADE)
}

fn visuals(palette: &Palette) -> Visuals {
    let radius = CornerRadius::same(ROW_RADIUS as u8);
    let popup_radius = CornerRadius::same(POPUP_RADIUS as u8);
    let outline = Stroke::new(1.0, palette.outline);

    Visuals {
        panel_fill: palette.bg,
        window_fill: palette.surface,
        window_stroke: outline,
        window_corner_radius: popup_radius,
        menu_corner_radius: popup_radius,
        window_shadow: POPUP_SHADOW,
        popup_shadow: POPUP_SHADOW,
        // A text field reads as a well cut into the surface it sits on.
        extreme_bg_color: palette.bg,
        faint_bg_color: palette.raised,
        weak_text_color: Some(palette.text_weak),
        hyperlink_color: palette.accent,
        selection: Selection {
            bg_fill: palette.accent,
            // Not a border: egui takes `selection.stroke` as the *text* colour of a
            // selected widget (`widget_style.rs`, `SELECTED_CLASS`).
            stroke: Stroke::new(1.0, palette.on_accent),
        },
        widgets: egui::style::Widgets {
            noninteractive: WidgetVisuals {
                bg_fill: palette.surface,
                weak_bg_fill: palette.surface,
                bg_stroke: outline,
                fg_stroke: Stroke::new(1.0, palette.text),
                corner_radius: radius,
                expansion: 0.0,
            },
            inactive: WidgetVisuals {
                bg_fill: palette.raised,
                weak_bg_fill: palette.raised,
                bg_stroke: Stroke::NONE,
                fg_stroke: Stroke::new(1.0, palette.text),
                corner_radius: radius,
                expansion: 0.0,
            },
            hovered: WidgetVisuals {
                bg_fill: palette.hover,
                weak_bg_fill: palette.hover,
                bg_stroke: outline,
                fg_stroke: Stroke::new(1.0, palette.text),
                corner_radius: radius,
                expansion: 1.0,
            },
            active: WidgetVisuals {
                bg_fill: palette.accent_low,
                weak_bg_fill: palette.accent_low,
                bg_stroke: Stroke::new(1.0, palette.accent),
                fg_stroke: Stroke::new(1.0, palette.text),
                corner_radius: radius,
                expansion: 1.0,
            },
            open: WidgetVisuals {
                bg_fill: palette.raised,
                weak_bg_fill: palette.raised,
                bg_stroke: outline,
                fg_stroke: Stroke::new(1.0, palette.text),
                corner_radius: radius,
                expansion: 0.0,
            },
        },
        ..Visuals::dark()
    }
}
