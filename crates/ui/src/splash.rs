//! The brand on its own, which is all the first moment of a session shows.
//!
//! The logo is a wordmark on a plate, so it is set on the faceplate as one object: a
//! badge dropped on the same material every other screen is machined out of, with the
//! wordmark printed into it a block at a time and what the app is set under it once the
//! name is whole.

use crate::image::{RgbImage, TextureCache};
use crate::theme::{self, ROW_HEIGHT};
use egui::{Align2, Color32, Pos2, Rect, Sense, Ui, Vec2};

/// How long the splash lasts, and how much of that the brand spends fading; between
/// the two fades it holds. Short on purpose: this is an emulator someone opened to
/// play something.
const DURATION: f64 = 1.2;
const FADE: f64 = 0.15;
/// How long the wordmark takes to print, from its first block to its last. Kept well
/// inside the hold, so the finished logo gets a beat before the fade out.
const PRINT: f64 = 0.45;
/// The print, and the line that follows it, both have to be over while the brand still
/// holds — either one still arriving as the splash leaves is never seen whole.
const _: () = assert!(PRINT + FADE < DURATION - FADE);

/// How much of the window's width the logo takes, and how far its own pixels may be
/// blown up — the blocks it is drawn out of have to read as blocks.
const LOGO_WIDTH: f32 = 0.5;
const MAX_SCALE: f32 = 2.0;

/// One block of the logo, in the logo's own pixels: the unit the wordmark is drawn on.
/// It is printed in whole ones, and the plate it sits on is one of them wider all round.
const BLOCK: f32 = 20.0;

/// Shows the brand on the app's own material, and reports whether the splash is done —
/// its time up, or the pointer asking to get on with it.
///
/// The logo is the platform's to hand over, decoded; a platform that ships without one
/// gets the name set in type on the same material.
pub(crate) fn show(
    root: &mut Ui,
    logo: Option<&RgbImage>,
    cache: &mut TextureCache,
    started: f64,
) -> bool {
    let elapsed = root.input(|i| i.time) - started;
    let fade = alpha(elapsed) as f32;
    let mut skipped = false;

    egui::CentralPanel::default().show(root, |ui| {
        theme::page(ui);
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click());
        skipped = response.clicked();

        let under = match logo {
            Some(logo) => {
                let scale = scale_for(rect, logo);
                let at = logo_rect(rect, logo, scale);
                let plate = at.expand(BLOCK * scale);
                badge(ui, plate, fade);
                print(ui, at, logo, cache, elapsed, fade);

                plate
            }
            None => {
                let color = ui.visuals().text_color().gamma_multiply(fade);
                theme::heading_at(ui, rect, Align2::CENTER_CENTER, crate::BRAND, color);

                // The room the name took, standing in for the plate a logo would have
                // brought, so the line under it is set off the same edge either way.
                Rect::from_center_size(rect.center(), Vec2::new(rect.width(), ROW_HEIGHT))
            }
        };

        line(ui, band_under(rect, under), elapsed, fade);
        byline(ui, foot(rect), elapsed, fade);
    });

    // The app polls on a cadence of its own, which the fade must not be left to.
    root.ctx().request_repaint();

    skipped || elapsed >= DURATION
}

/// The plate the wordmark is printed into: a raised surface with the popup's shadow
/// under it, which is what makes the brand an object lying on the faceplate rather than
/// a picture of one printed into it.
fn badge(ui: &Ui, at: Rect, fade: f32) {
    let mut shadow = ui.visuals().window_shadow;
    shadow.color = shadow.color.gamma_multiply(fade);
    ui.painter().add(shadow.as_shape(at, theme::POPUP_RADIUS));
    theme::plate_faded(
        ui.painter(),
        at,
        theme::POPUP_RADIUS,
        ui.visuals().window_fill,
        fade,
    );
}

/// The wordmark arriving a block at a time, left to right. Only the printed part of the
/// asset is drawn, so what has not arrived yet is the bare plate the wordmark is coming
/// in on — nothing stands in for it, and nothing is missing out of a finished logo.
fn print(ui: &Ui, at: Rect, logo: &RgbImage, cache: &mut TextureCache, elapsed: f64, fade: f32) {
    let printed = printed(elapsed, logo);

    if printed == 0.0 {
        return;
    }

    // One texture under a key of its own: the asset never changes, so nothing has to
    // invalidate it.
    let texture = cache.texture(ui, 0, logo);
    let sized = egui::load::SizedTexture::new(texture.id(), at.size());
    let shown = Rect::from_min_size(at.min, Vec2::new(at.width() * printed, at.height()));

    egui::Image::from_texture(sized)
        .uv(Rect::from_min_max(Pos2::ZERO, Pos2::new(printed, 1.0)))
        // White fades to nothing, so the wordmark goes with the plate under it rather
        // than hanging on over a badge that has already left.
        .tint(Color32::WHITE.gamma_multiply(fade))
        .paint_at(ui, shown);
}

/// What the app is, set under what it is called. Held back until the wordmark is whole
/// — the name lands first, and the line is what it is read as afterwards.
///
/// Set in a row's second-line tone at body size: it is small print, but it is small
/// print read from wherever the sofa is.
fn line(ui: &Ui, band: Rect, elapsed: f64, fade: f32) {
    let color = theme::detail_color(ui, 0.0).gamma_multiply(fade * arrived(elapsed));

    theme::label(ui, band, Align2::CENTER_CENTER, crate::BRAND_LINE, color);
}

/// The credit, signed in the margin: the brand's own block is the name and what it is.
fn byline(ui: &Ui, band: Rect, elapsed: f64, fade: f32) {
    let color = theme::detail_color(ui, 0.0).gamma_multiply(fade * arrived(elapsed));
    let credit = format!("{} {}", crate::SIGNED_BY, crate::AUTHOR);

    theme::detail_at(ui, band, Align2::CENTER_CENTER, credit, color);
}

/// The band at the window's foot, a row's gap off the edge.
fn foot(rect: Rect) -> Rect {
    let at = Pos2::new(rect.left(), rect.bottom() - ROW_HEIGHT - theme::ROW_GAP);

    Rect::from_min_size(at, Vec2::new(rect.width(), ROW_HEIGHT))
}

/// How far up the line under the brand is: it waits out the print, then comes in over
/// the same fade the splash itself opens with.
fn arrived(elapsed: f64) -> f32 {
    ((elapsed - PRINT) / FADE).clamp(0.0, 1.0) as f32
}

/// The band that line is set in: as wide as the window, so a long one is never cut down
/// to the plate's width, and clear of the plate by the gap that separates any two rows.
fn band_under(rect: Rect, under: Rect) -> Rect {
    let at = Pos2::new(rect.left(), under.bottom() + theme::ROW_GAP);

    Rect::from_min_size(at, Vec2::new(rect.width(), ROW_HEIGHT))
}

/// How much of the wordmark is printed, as a share of its width: whole blocks of it, so
/// the column arriving is a block wide and never a sliver of one.
fn printed(elapsed: f64, logo: &RgbImage) -> f32 {
    let progress = (elapsed / PRINT).clamp(0.0, 1.0) as f32;
    let blocks = (logo.width as f32 / BLOCK).ceil();

    (progress * blocks).floor() / blocks
}

/// How big the logo is drawn: as wide as its share of the window allows, in whole
/// multiples of its own pixels so its blocks stay square — and whatever fits, in a
/// window too narrow for even one.
fn scale_for(rect: Rect, logo: &RgbImage) -> f32 {
    let fit = rect.width() * LOGO_WIDTH / logo.width as f32;

    if fit < 1.0 {
        fit
    } else {
        fit.floor().min(MAX_SCALE)
    }
}

/// Where the logo goes: centred, on whole points. Its pixels are blown up by whole
/// numbers, and from a half-point origin the renderer would round some of the blocks a
/// pixel wider than their neighbours.
fn logo_rect(rect: Rect, logo: &RgbImage, scale: f32) -> Rect {
    let size = Vec2::new(logo.width as f32, logo.height as f32) * scale;
    let at = rect.center() - size / 2.0;

    Rect::from_min_size(Pos2::new(at.x.round(), at.y.round()), size)
}

/// Fades in, holds, fades out. Clamped either side, so a clock that jumps cannot ask
/// for a colour that is more or less than a colour.
fn alpha(elapsed: f64) -> f64 {
    let fade_out = DURATION - FADE;

    if elapsed < FADE {
        (elapsed / FADE).clamp(0.0, 1.0)
    } else if elapsed < fade_out {
        1.0
    } else {
        ((DURATION - elapsed) / FADE).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The brand is never abruptly there or gone: it comes up out of the surface and
    /// goes back into it.
    #[test]
    fn the_brand_fades_in_holds_and_fades_out() {
        assert_eq!(alpha(0.0), 0.0);
        assert_eq!(alpha(FADE), 1.0);
        assert_eq!(alpha(DURATION / 2.0), 1.0);
        assert_eq!(alpha(DURATION), 0.0);
        assert!(nearly(alpha(FADE / 2.0), 0.5), "half way in");
        assert!(nearly(alpha(DURATION - FADE / 2.0), 0.5), "half way out");
    }

    /// A clock that jumped either way still leaves the brand a colour it can be drawn
    /// in, rather than one egui would have to interpret.
    #[test]
    fn a_jumped_clock_stays_within_a_colour() {
        assert_eq!(alpha(-1.0), 0.0);
        assert_eq!(alpha(DURATION * 2.0), 0.0);
    }

    /// Whole multiples keep the logo's pixels square, which is the whole look; only a
    /// window too narrow for the logo at 1:1 gets a fraction.
    #[test]
    fn the_logo_is_scaled_in_whole_pixels_where_it_can_be() {
        let logo = logo();
        let window = |width: f32| Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0));

        // Half of 2640 is four logos wide, but two is as far as the blocks may go.
        assert_eq!(scale_for(window(2640.0), &logo), MAX_SCALE);
        assert_eq!(scale_for(window(1980.0), &logo), 1.0, "1.5 rounds down");
        assert!(
            scale_for(window(660.0), &logo) < 1.0,
            "narrower than the logo"
        );
    }

    /// The wordmark is printed in the blocks it is drawn out of: part-way through, the
    /// edge of what has arrived lands between two of them and never inside one.
    #[test]
    fn the_wordmark_prints_in_whole_blocks() {
        let logo = logo();
        let blocks = logo.width as f32 / BLOCK;

        for step in 0..=20 {
            let printed = printed(PRINT * step as f64 / 20.0, &logo) * blocks;

            // Within the float error of dividing by the block count and multiplying it
            // back out, which is what the share is worked out through.
            assert!((printed - printed.round()).abs() < 1e-4, "{printed} blocks");
        }
    }

    /// Nothing is left half printed: the wordmark is whole by the end of the print, and
    /// stays whole for the rest of the splash.
    #[test]
    fn the_wordmark_is_whole_once_it_is_printed() {
        let logo = logo();

        assert_eq!(printed(0.0, &logo), 0.0);
        assert_eq!(printed(PRINT, &logo), 1.0);
        assert_eq!(printed(DURATION, &logo), 1.0);
    }

    /// The name lands first: the line under it waits out the print, and is up in full
    /// while the brand still holds.
    #[test]
    fn the_line_follows_the_name() {
        assert_eq!(arrived(0.0), 0.0);
        assert_eq!(arrived(PRINT), 0.0, "not while the wordmark is printing");
        assert_eq!(arrived(PRINT + FADE), 1.0);
        assert_eq!(alpha(PRINT + FADE), 1.0, "up before the brand leaves");
    }

    /// The line is set clear of the plate and across the window, not across the plate:
    /// a line longer than the badge is still a line and not an ellipsis.
    #[test]
    fn the_line_is_set_under_the_plate_and_as_wide_as_the_window() {
        let window = Rect::from_min_size(Pos2::ZERO, Vec2::new(1600.0, 900.0));
        let plate = Rect::from_center_size(window.center(), Vec2::new(700.0, 260.0));
        let band = band_under(window, plate);

        assert!(band.top() >= plate.bottom(), "the line runs into the plate");
        assert_eq!(band.width(), window.width());
    }

    /// The credit sits at the foot, clear of the line under the brand.
    #[test]
    fn the_byline_is_signed_in_the_margin() {
        let window = Rect::from_min_size(Pos2::new(7.0, 13.0), Vec2::new(1600.0, 900.0));
        let plate = Rect::from_center_size(window.center(), Vec2::new(700.0, 260.0));
        let band = foot(window);

        assert_eq!(band.bottom(), window.bottom() - theme::ROW_GAP);
        assert_eq!(band.width(), window.width());
        assert!(
            band.top() >= band_under(window, plate).bottom(),
            "one block"
        );
    }

    /// A blown-up block is only square if the whole wordmark starts on a whole point.
    #[test]
    fn the_logo_starts_on_a_whole_point() {
        let logo = logo();
        let window = Rect::from_min_size(Pos2::new(7.0, 13.0), Vec2::new(1337.0, 911.0));
        let at = logo_rect(window, &logo, 2.0);

        assert_eq!(at.min.x.fract(), 0.0, "left edge off a point");
        assert_eq!(at.min.y.fract(), 0.0, "top edge off a point");
    }

    /// The asset's shape, which is all these tests read of it.
    fn logo() -> RgbImage {
        RgbImage {
            rgb: Vec::new(),
            width: 660,
            height: 220,
        }
    }

    /// The curve's ends are exact; a point inside a fade lands within the float error
    /// of the durations it is worked out from.
    fn nearly(alpha: f64, expected: f64) -> bool {
        (alpha - expected).abs() < 1e-9
    }
}
