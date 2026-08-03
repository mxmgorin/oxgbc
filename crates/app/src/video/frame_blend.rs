use crate::config::{RenderConfig, VideoConfig};
use core::ppu::PPU_BUFFER_LEN;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum FrameBlendMode {
    None,
    Linear(LinearFrameBlend),
    Additive(AdditiveFrameBlend),
    Exp(ExponentialFrameBlend),
    Gamma(GammaCorrectedFrameBlend),
    Accurate(BlendProfile),
}

#[derive(Debug)]
pub struct FrameBlend {
    prev_framebuffer: Box<[u8]>,
}

impl FrameBlend {
    pub fn new(mode: &FrameBlendMode) -> Option<Self> {
        if let FrameBlendMode::None = mode {
            None
        } else {
            Some(Self {
                prev_framebuffer: vec![0; PPU_BUFFER_LEN].into_boxed_slice(),
            })
        }
    }

    pub fn process_buffer(&mut self, pixel_buffer: &[u8], config: &VideoConfig) -> &[u8] {
        debug_assert_eq!(self.prev_framebuffer.len(), pixel_buffer.len());
        process_buffer_rgb565(&mut self.prev_framebuffer, pixel_buffer, config);

        &self.prev_framebuffer
    }
}

pub fn process_buffer(prev_framebuffer: &mut [u8], pixel_buffer: &[u8], config: &VideoConfig) {
    let w = RenderConfig::WIDTH;
    let h = RenderConfig::HEIGHT;

    for i in 0..(w * h) {
        // each pixel offset in bytes
        let idx = i * 4;

        // current pixel RGBA components
        let cr = pixel_buffer[idx];
        let cg = pixel_buffer[idx + 1];
        let cb = pixel_buffer[idx + 2];

        // previous pixel RGBA components
        let pr = prev_framebuffer[idx];
        let pg = prev_framebuffer[idx + 1];
        let pb = prev_framebuffer[idx + 2];

        let (r, g, b) = compute_pixel(
            prev_framebuffer,
            config,
            i,
            w,
            h,
            (pr, pg, pb),
            (cr, cg, cb),
        );

        // write new pixel back, keep alpha = 255 (fully opaque)
        prev_framebuffer[idx] = r;
        prev_framebuffer[idx + 1] = g;
        prev_framebuffer[idx + 2] = b;
    }
}

pub fn process_buffer_rgb565(
    prev_framebuffer: &mut [u8],
    pixel_buffer: &[u8],
    config: &VideoConfig,
) {
    let w = RenderConfig::WIDTH;
    let h = RenderConfig::HEIGHT;

    for i in 0..(w * h) {
        // index in bytes for 16-bit pixel
        let idx = i * 2;

        // ---- current pixel (RGB565) ----
        let cur565 = u16::from_le_bytes([pixel_buffer[idx], pixel_buffer[idx + 1]]);
        let cr = ((cur565 >> 11) & 0x1F) << 3; // 5 bits red -> 8 bits
        let cg = ((cur565 >> 5) & 0x3F) << 2; // 6 bits green -> 8 bits
        let cb = (cur565 & 0x1F) << 3; // 5 bits blue -> 8 bits

        // ---- previous pixel (RGB565) ----
        let prev565 = u16::from_le_bytes([prev_framebuffer[idx], prev_framebuffer[idx + 1]]);
        let pr = ((prev565 >> 11) & 0x1F) << 3;
        let pg = ((prev565 >> 5) & 0x3F) << 2;
        let pb = (prev565 & 0x1F) << 3;

        let (r, g, b) = compute_pixel(
            prev_framebuffer,
            config,
            i,
            w,
            h,
            (pr as u8, pg as u8, pb as u8),
            (cr as u8, cg as u8, cb as u8),
        );

        // ---- re-pack to RGB565 ----
        let new565: u16 = ((r as u16 & 0xF8) << 8) | // keep top 5 bits of red
                ((g as u16 & 0xFC) << 3) | // keep top 6 bits of green
                (b as u16 >> 3); // keep top 5 bits of blue

        let bytes = new565.to_le_bytes();
        prev_framebuffer[idx] = bytes[0];
        prev_framebuffer[idx + 1] = bytes[1];
    }
}

pub fn compute_pixel(
    prev_framebuffer: &mut [u8],
    config: &VideoConfig,
    i: usize,
    w: usize,
    h: usize,
    prev: (u8, u8, u8),
    curr: (u8, u8, u8),
) -> (u8, u8, u8) {
    let (pr, pg, pb) = prev;
    let (cr, cg, cb) = curr;

    // Convert to linear [0..1]
    let pr_lin = pr as f32 / 255.0;
    let pg_lin = pg as f32 / 255.0;
    let pb_lin = pb as f32 / 255.0;

    let cr_lin = cr as f32 / 255.0;
    let cg_lin = cg as f32 / 255.0;
    let cb_lin = cb as f32 / 255.0;

    // Final RGB values
    let (mut lr, mut lg, mut lb) = match &config.render.frame_blend_mode {
        FrameBlendMode::None => (cr_lin, cg_lin, cb_lin),
        FrameBlendMode::Linear(x) => {
            let a = x.alpha;
            (
                pr_lin * (1.0 - a) + cr_lin * a,
                pg_lin * (1.0 - a) + cg_lin * a,
                pb_lin * (1.0 - a) + cb_lin * a,
            )
        }

        FrameBlendMode::Exp(x) => {
            let fade = x.fade;
            (
                pr_lin * fade + cr_lin * (1.0 - fade),
                pg_lin * fade + cg_lin * (1.0 - fade),
                pb_lin * fade + cb_lin * (1.0 - fade),
            )
        }
        FrameBlendMode::Additive(x) => {
            let mut fr = pr_lin * x.fade + cr_lin * x.alpha;
            let mut fg = pg_lin * x.fade + cg_lin * x.alpha;
            let mut fb = pb_lin * x.fade + cb_lin * x.alpha;

            // Clamp to prevent overexposure
            fr = fr.min(1.0);
            fg = fg.min(1.0);
            fb = fb.min(1.0);

            (fr, fg, fb)
        }

        FrameBlendMode::Gamma(x) => {
            let gamma = 2.2;
            let fade = x.fade;

            fn to_linear(v: f32, g: f32) -> f32 {
                v.powf(g)
            }
            fn to_srgb(v: f32, g: f32) -> f32 {
                v.powf(1.0 / g)
            }

            let pr_l = to_linear(pr_lin, gamma);
            let pg_l = to_linear(pg_lin, gamma);
            let pb_l = to_linear(pb_lin, gamma);

            let cr_l = to_linear(cr_lin, gamma);
            let cg_l = to_linear(cg_lin, gamma);
            let cb_l = to_linear(cb_lin, gamma);

            (
                to_srgb(pr_l * fade + cr_l * (1.0 - fade), gamma),
                to_srgb(pg_l * fade + cg_l * (1.0 - fade), gamma),
                to_srgb(pb_l * fade + cb_l * (1.0 - fade), gamma),
            )
        }

        FrameBlendMode::Accurate(x) => {
            // Scanline timing & jitter
            let y = i / w;
            let jitter = ((y as f32).sin() * 0.003) + 1.0;
            let scan_delay = (1.0 - (y as f32 / h as f32) * 0.2) * jitter;

            fn lcd_step(prev: f32, curr: f32, rise: f32, fall: f32, delay: f32) -> f32 {
                let rate = if curr > prev { rise } else { fall };
                prev + (curr - prev) * rate * delay
            }

            // LCD response curve
            let mut lr = lcd_step(pr_lin, cr_lin, x.rise, x.fall, scan_delay);
            let mut lg = lcd_step(pg_lin, cg_lin, x.rise, x.fall, scan_delay);
            let mut lb = lcd_step(pb_lin, cb_lin, x.rise, x.fall, scan_delay);

            // Pixel bleeding (left + top)
            let left_idx = if i % w == 0 { i } else { i - 1 };
            let top_idx = if y == 0 { i } else { i - w };

            // Calculate byte offsets
            let left_base = left_idx * 4;
            let top_base = top_idx * 4;

            let lpr = prev_framebuffer[left_base] as f32 / 255.0;
            let lpg = prev_framebuffer[left_base + 1] as f32 / 255.0;
            let lpb = prev_framebuffer[left_base + 2] as f32 / 255.0;

            let tpr = prev_framebuffer[top_base] as f32 / 255.0;
            let tpg = prev_framebuffer[top_base + 1] as f32 / 255.0;
            let tpb = prev_framebuffer[top_base + 2] as f32 / 255.0;

            lr = lr * (1.0 - x.bleed) + ((lpr + tpr) * 0.5) * x.bleed;
            lg = lg * (1.0 - x.bleed) + ((lpg + tpg) * 0.5) * x.bleed;
            lb = lb * (1.0 - x.bleed) + ((lpb + tpb) * 0.5) * x.bleed;

            // Tint
            lr *= x.tint.red;
            lg *= x.tint.green;
            lb *= x.tint.blue;

            (lr, lg, lb)
        }
    };

    let dim = config.render.blend_dim;
    lr *= dim;
    lg *= dim;
    lb *= dim;

    // Convert back to 0..255
    (
        (lr * 255.0).clamp(0.0, 255.0) as u8,
        (lg * 255.0).clamp(0.0, 255.0) as u8,
        (lb * 255.0).clamp(0.0, 255.0) as u8,
    )
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GammaCorrectedFrameBlend {
    pub alpha: f32,
    pub fade: f32,
}

impl Default for GammaCorrectedFrameBlend {
    fn default() -> Self {
        Self {
            alpha: 0.5,
            fade: 0.5,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ExponentialFrameBlend {
    pub fade: f32,
}

impl Default for ExponentialFrameBlend {
    fn default() -> Self {
        Self { fade: 0.5 }
    }
}

impl FrameBlendMode {
    pub fn name(&self) -> &str {
        match self {
            FrameBlendMode::Linear(_) => "Linear",
            FrameBlendMode::Additive(_) => "Additive",
            FrameBlendMode::None => "None",
            FrameBlendMode::Exp(_) => "Exp",
            FrameBlendMode::Gamma(_) => "Gamma",
            FrameBlendMode::Accurate(_) => "Accurate",
        }
    }

    pub fn change_alpha(&mut self, v: f32) {
        match self {
            FrameBlendMode::None => {}
            FrameBlendMode::Linear(x) => {
                x.alpha = core::change_f32_rounded(x.alpha, v).clamp(0.0, 1.0)
            }
            FrameBlendMode::Additive(x) => {
                x.alpha = core::change_f32_rounded(x.alpha, v).clamp(0.0, 1.0)
            }
            FrameBlendMode::Exp(_) => {}
            FrameBlendMode::Gamma(x) => {
                x.alpha = core::change_f32_rounded(x.alpha, v).clamp(0.0, 1.0)
            }
            FrameBlendMode::Accurate(_) => {}
        }
    }

    pub fn change_fade(&mut self, v: f32) {
        match self {
            FrameBlendMode::None => {}
            FrameBlendMode::Linear(_) => {}
            FrameBlendMode::Additive(x) => {
                x.fade = core::change_f32_rounded(x.fade, v).clamp(0.0, 1.0)
            }
            FrameBlendMode::Exp(x) => x.fade = core::change_f32_rounded(x.fade, v).clamp(0.0, 1.0),
            FrameBlendMode::Gamma(x) => {
                x.fade = core::change_f32_rounded(x.fade, v).clamp(0.0, 1.0)
            }
            FrameBlendMode::Accurate(_) => {}
        }
    }

    pub fn alpha(&self) -> f32 {
        match self {
            FrameBlendMode::None => 0.0,
            FrameBlendMode::Linear(x) => x.alpha,
            FrameBlendMode::Additive(x) => x.alpha,
            FrameBlendMode::Exp(_) => 0.0,
            FrameBlendMode::Gamma(x) => x.alpha,
            FrameBlendMode::Accurate(_) => 0.0,
        }
    }

    pub fn fade(&self) -> f32 {
        match self {
            FrameBlendMode::None => 0.0,
            FrameBlendMode::Linear(_) => 0.0,
            FrameBlendMode::Additive(x) => x.fade,
            FrameBlendMode::Exp(x) => x.fade,
            FrameBlendMode::Gamma(x) => x.fade,
            FrameBlendMode::Accurate(_) => 0.0,
        }
    }

    pub fn profile(&self) -> Option<&BlendProfile> {
        match self {
            FrameBlendMode::None
            | FrameBlendMode::Linear(_)
            | FrameBlendMode::Additive(_)
            | FrameBlendMode::Exp(_)
            | FrameBlendMode::Gamma(_) => None,
            FrameBlendMode::Accurate(x) => Some(x),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct LinearFrameBlend {
    /// (0.0..1.0), smaller = stronger ghosting
    pub alpha: f32,
}

impl Default for LinearFrameBlend {
    fn default() -> Self {
        Self { alpha: 0.45 }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy)]
pub struct AdditiveFrameBlend {
    /// controls trail persistence
    pub fade: f32,
    /// controls how strong new pixels add
    pub alpha: f32,
}

impl Default for AdditiveFrameBlend {
    fn default() -> Self {
        Self {
            fade: 0.65,
            alpha: 0.35,
        }
    }
}

pub const DMG_PROFILE: BlendProfile =
    BlendProfile::new(0.35, 0.08, 0.15, BlendProfileTint::new(0.78, 0.86, 0.71));
pub const POCKET_PROFILE: BlendProfile =
    BlendProfile::new(0.5, 0.15, 0.07, BlendProfileTint::new(1.0, 1.0, 1.0));

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BlendProfile {
    pub rise: f32,
    pub fall: f32,
    pub bleed: f32,
    pub tint: BlendProfileTint,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct BlendProfileTint {
    pub red: f32,
    pub green: f32,
    pub blue: f32,
}

impl BlendProfileTint {
    pub const fn new(red: f32, green: f32, blue: f32) -> Self {
        Self { red, green, blue }
    }

    pub fn reset(&mut self) {
        self.red = 1.0;
        self.green = 1.0;
        self.blue = 1.0;
    }
}

impl BlendProfile {
    pub const fn new(rise: f32, fall: f32, bleed: f32, tint: BlendProfileTint) -> Self {
        Self {
            rise,
            fall,
            bleed,
            tint,
        }
    }

    pub fn name(&self) -> &str {
        if self == &DMG_PROFILE {
            "DMG"
        } else if self == &POCKET_PROFILE {
            "POCKET"
        } else {
            "CUSTOM"
        }
    }
}
