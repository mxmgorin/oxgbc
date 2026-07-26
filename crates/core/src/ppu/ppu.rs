use crate::cpu::interrupts::{InterruptType, Interrupts};
use crate::ppu::fetcher::PixelFetcher;
use crate::ppu::lcd::{Lcd, LcdStatSrc, PpuMode};
use crate::ppu::oam::OamRam;
use crate::ppu::vram::VideoRam;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use web_time::Instant;

pub const OAM_DOTS: usize = 80;
pub const TRANSFER_DOTS: usize = 172;
pub const LINES_PER_FRAME: usize = 154;
pub const TICKS_PER_LINE: usize = 456;
pub const LCD_Y_RES: u8 = 144;
pub const LCD_X_RES: u8 = 160;
pub const TARGET_FPS_F: f64 = 59.7;
pub const TARGET_FRAME_TIME_MILLIS: u64 = FRAME_DURATION.as_millis() as u64;
pub const PPU_PIXELS_COUNT: usize = LCD_Y_RES as usize * LCD_X_RES as usize;
pub const PPU_BYTES_PER_PIXEL: usize = 2;
pub const PPU_BUFFER_LEN: usize = PPU_PIXELS_COUNT * PPU_BYTES_PER_PIXEL;
pub const PPU_PITCH: usize = PPU_BYTES_PER_PIXEL * LCD_X_RES as usize;

pub const FRAME_DURATION: Duration = Duration::from_nanos(16_743_000); // ~59.7 fps

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ppu {
    pub video_ram: VideoRam,
    pub oam_ram: OamRam,
    pub lcd: Lcd,
    pub current_frame: usize,
    line_ticks: usize,
    fps_counter: Option<FpsCounter>,
    fetcher: PixelFetcher,
    /// Composite STAT interrupt line: OR of all enabled STAT sources. The
    /// LCDStat interrupt is requested only on a rising edge of this line.
    #[serde(default)]
    stat_line: bool,
    /// First scanline after the LCD was enabled runs a special short sequence
    /// (no mode 2, shifted timings).
    #[serde(default)]
    lcdon_line0: bool,
}

impl Ppu {
    pub fn new(lcd: Lcd) -> Self {
        Self {
            lcd,
            ..Default::default()
        }
    }

    pub fn toggle_fps(&mut self, enable: bool) {
        if enable {
            self.fps_counter = Some(FpsCounter::default());
        } else {
            self.fps_counter = None;
        }
    }

    #[inline(always)]
    pub fn get_fps(&self) -> Option<f32> {
        self.fps_counter.as_ref().map(|x| x.get())
    }

    #[inline(always)]
    pub fn tick(&mut self, interrupts: &mut Interrupts) {
        if !self.lcd.control.is_lcd_enabled() {
            // Frozen while the LCD is off; state was fixed up at disable time.
            return;
        }

        self.line_ticks += 1;

        // LY becomes visible 4 dots before the line's mode events (mooneye
        // hblank_ly_scx_timing). While the new LY comparison is in flight the
        // LYC coincidence flag reads 0; it is recomputed at the line boundary.
        // OAM reads (but not writes) are already blocked in this window when
        // the next line is a visible one.
        if self.line_ticks == TICKS_PER_LINE - 4 {
            self.lcd.increment_ly();

            if self.lcd.ly as usize >= LINES_PER_FRAME {
                self.lcd.reset_ly();
            }

            self.lcd.status.set_lyc(false);

            if self.lcd.ly < LCD_Y_RES {
                self.lcd.oam_read_blocked = true;
            } else if self.lcd.ly == LCD_Y_RES
                && self.lcd.model == crate::emu::config::GbModel::Cgb
            {
                // On CGB the line-144 OAM STAT pulse fires one M-cycle before
                // the VBlank interrupt (mooneye vblank_stat_intr-C); on DMG
                // they are simultaneous (handled in set_mode_vblank).
                self.update_stat_line(true, interrupts);
            }
        } else if self.line_ticks == TICKS_PER_LINE {
            self.lcd.update_lyc_flag();
        }

        match self.lcd.status.get_ppu_mode() {
            PpuMode::HBlank => self.mode_hblank(interrupts),
            PpuMode::VBlank => self.mode_vblank(interrupts),
            PpuMode::Oam => self.mode_oam(),
            PpuMode::Transfer => return self.mode_transfer(interrupts),
        }
    }

    /// Advance the PPU by `dots` device ticks in one call. In blank/OAM modes
    /// the span to the next event (see `next_event_dot`) collapses to a
    /// `line_ticks` bump, running `tick()` only at the event; mode 3 stays
    /// per-dot (variable length). Caller must keep both DMAs idle (no
    /// interleaving OAM/VRAM writes) or the clock falls back to per-tick.
    pub fn advance(&mut self, dots: usize, interrupts: &mut Interrupts) {
        if !self.lcd.control.is_lcd_enabled() {
            // Frozen while the LCD is off, same early-out as tick().
            return;
        }

        let mut remaining = dots;

        while remaining > 0 {
            if self.lcd.status.get_ppu_mode() == PpuMode::Transfer {
                self.tick(interrupts);
                remaining -= 1;
                continue;
            }

            // `tick()` bumps line_ticks then acts, so the event fires on the
            // tick that brings line_ticks to next_event_dot(); the dots before
            // it just advance the counter.
            let event = self.next_event_dot();
            let to_event = event - self.line_ticks;

            if to_event > remaining {
                self.line_ticks += remaining;
                return;
            }

            self.line_ticks = event - 1; // sit one dot before the event
            remaining -= to_event;
            self.tick(interrupts); // the tick that fires it
        }
    }

    /// The next `line_ticks` value at which `tick` does anything in the
    /// current non-transfer mode. Static per mode — compares only, no
    /// divisions (the per-window cost must stay O(1)).
    #[inline(always)]
    fn next_event_dot(&self) -> usize {
        match self.lcd.status.get_ppu_mode() {
            // 76: OAM write-unblock + VRAM read prelock; 80: mode 3 entry.
            PpuMode::Oam => {
                if self.line_ticks < OAM_DOTS - 4 {
                    OAM_DOTS - 4
                } else {
                    OAM_DOTS
                }
            }
            // The LCD-enable line runs as mode 0 until its dot-80 mode 3
            // entry; 452/456 are unreachable while the flag is set.
            PpuMode::HBlank if self.lcdon_line0 => OAM_DOTS,
            // 452: early LY increment; 456: LYC recompute + line wrap.
            PpuMode::HBlank | PpuMode::VBlank => {
                if self.line_ticks < TICKS_PER_LINE - 4 {
                    TICKS_PER_LINE - 4
                } else {
                    TICKS_PER_LINE
                }
            }
            PpuMode::Transfer => unreachable!("advance() ticks mode 3 per-dot, not scheduled"),
        }
    }

    /// LCD register write with PPU side effects (LCDC enable/disable, STAT and
    /// LYC writes re-evaluate the composite STAT line).
    pub fn write_lcd(&mut self, address: u16, value: u8, interrupts: &mut Interrupts) {
        match address {
            crate::ppu::lcd::LCD_CONTROL_ADDRESS => {
                let was_enabled = self.lcd.control.is_lcd_enabled();
                self.lcd.control.byte = value;
                let now_enabled = self.lcd.control.is_lcd_enabled();

                if was_enabled && !now_enabled {
                    // Turning the LCD off resets LY and the mode bits, but the
                    // LYC coincidence flag is frozen (not recomputed).
                    self.lcd.ly = 0;
                    self.lcd.window.line_number = 0;
                    self.line_ticks = 0;
                    self.lcd.status.set_ppu_mode(PpuMode::HBlank);
                    self.lcd.oam_read_blocked = false;
                    self.lcd.oam_write_blocked = false;
                    self.lcd.vram_read_prelock = false;
                } else if !was_enabled && now_enabled {
                    // The first line after enabling has no mode 2 and starts
                    // in mode 0; the comparison clock restarts immediately.
                    self.line_ticks = 0;
                    self.lcdon_line0 = true;
                    self.lcd.status.set_ppu_mode(PpuMode::HBlank);
                    self.lcd.update_lyc_flag();
                    self.update_stat_line(false, interrupts);
                }
            }
            crate::ppu::lcd::LCD_STATUS_ADDRESS => {
                self.lcd.status.write(value);

                if self.lcd.control.is_lcd_enabled() {
                    self.update_stat_line(false, interrupts);
                }
            }
            crate::ppu::lcd::LCD_LY_COMPARE_ADDRESS => {
                self.lcd.ly_compare = value;

                // The comparison clock only runs while the LCD is on.
                if self.lcd.control.is_lcd_enabled() {
                    self.lcd.update_lyc_flag();
                    self.update_stat_line(false, interrupts);
                }
            }
            _ => self.lcd.write(address, value),
        }
    }

    /// Recompute the composite STAT line and request the LCDStat interrupt on
    /// a rising edge. `oam_pulse` models the OAM source firing at the start of
    /// line 144 together with VBlank (DMG behavior).
    #[inline(always)]
    fn update_stat_line(&mut self, oam_pulse: bool, interrupts: &mut Interrupts) {
        let status = &self.lcd.status;
        let mode = status.get_ppu_mode();

        let line = (status.is_stat_interrupt(LcdStatSrc::HBlank) && mode == PpuMode::HBlank)
            || (status.is_stat_interrupt(LcdStatSrc::VBlank) && mode == PpuMode::VBlank)
            || (status.is_stat_interrupt(LcdStatSrc::Oam) && (mode == PpuMode::Oam || oam_pulse))
            || (status.is_stat_interrupt(LcdStatSrc::Lyc) && status.get_lyc());

        if line && !self.stat_line {
            interrupts.request_interrupt(InterruptType::LCDStat);
        }

        self.stat_line = line;
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.line_ticks = 0;
        self.current_frame = 0;
        self.lcd.buffer.reset_x();
        self.fetcher.reset(self.lcd.scroll_x);
    }

    #[inline(always)]
    fn mode_oam(&mut self) {
        // For the last 4 dots of mode 2, OAM write access unblocks and VRAM
        // read access already blocks (mooneye lcdon_write_timing-GS /
        // lcdon_timing-GS); OAM reads stay blocked.
        if self.line_ticks == OAM_DOTS - 4 {
            self.lcd.oam_write_blocked = false;
            self.lcd.vram_read_prelock = true;
        }

        if self.line_ticks >= OAM_DOTS {
            self.fetcher
                .sprite_fetcher
                .scan_oam(&self.lcd, &self.oam_ram);
            self.set_mode_transfer();
        }
    }

    #[inline(always)]
    fn mode_transfer(&mut self, interrupts: &mut Interrupts) {
        // Check completion before ticking so mode 0 begins on the dot after
        // the 160th pixel was pushed (dot 252 + SCX%8 + sprite penalties).
        if self.lcd.buffer.count_x() >= LCD_X_RES as usize {
            self.set_mode_hblank(interrupts);
            return;
        }

        self.fetcher
            .tick(&mut self.lcd, &self.video_ram, self.line_ticks);
    }

    #[inline(always)]
    fn mode_vblank(&mut self, interrupts: &mut Interrupts) {
        if self.line_ticks >= TICKS_PER_LINE {
            if self.lcd.ly == 0 {
                // LY wrapped 4 dots ago: new frame begins.
                self.set_mode_oam(interrupts);
            } else {
                self.update_stat_line(false, interrupts);
            }

            self.line_ticks = 0;
        }
    }

    #[inline(always)]
    fn mode_hblank(&mut self, interrupts: &mut Interrupts) {
        if self.lcdon_line0 {
            // First line after LCD-enable: the mode bits read 0 instead of 2
            // and OAM stays accessible, then the line continues normally from
            // mode 3 (mooneye lcdon_timing-GS).
            if self.line_ticks >= OAM_DOTS {
                self.lcdon_line0 = false;
                self.fetcher
                    .sprite_fetcher
                    .scan_oam(&self.lcd, &self.oam_ram);
                self.set_mode_transfer();
            }

            return;
        }

        if self.line_ticks >= TICKS_PER_LINE {
            if self.lcd.ly >= LCD_Y_RES {
                self.set_mode_vblank(interrupts);
                self.current_frame += 1;

                if let Some(fps) = self.fps_counter.as_mut() {
                    fps.update()
                }
            } else {
                self.set_mode_oam(interrupts);
            }

            self.line_ticks = 0;
        }
    }

    #[inline(always)]
    fn set_mode_oam(&mut self, interrupts: &mut Interrupts) {
        self.lcd.status.set_ppu_mode(PpuMode::Oam);
        self.lcd.oam_read_blocked = true;
        self.lcd.oam_write_blocked = true;
        self.update_stat_line(false, interrupts);
    }

    #[inline(always)]
    const fn set_mode_transfer(&mut self) {
        self.lcd.buffer.reset_x();
        self.fetcher.reset(self.lcd.scroll_x);
        self.lcd.oam_read_blocked = true;
        self.lcd.oam_write_blocked = true;
        self.lcd.status.set_ppu_mode(PpuMode::Transfer);
    }

    #[inline(always)]
    fn set_mode_hblank(&mut self, interrupts: &mut Interrupts) {
        self.lcd.status.set_ppu_mode(PpuMode::HBlank);
        self.lcd.oam_read_blocked = false;
        self.lcd.oam_write_blocked = false;
        self.lcd.vram_read_prelock = false;
        self.update_stat_line(false, interrupts);
    }

    #[inline(always)]
    fn set_mode_vblank(&mut self, interrupts: &mut Interrupts) {
        self.lcd.status.set_ppu_mode(PpuMode::VBlank);
        self.lcd.oam_read_blocked = false;
        self.lcd.oam_write_blocked = false;
        self.lcd.vram_read_prelock = false;
        interrupts.request_interrupt(InterruptType::VBlank);

        // On DMG the OAM STAT source also pulses at the start of line 144,
        // simultaneously with VBlank (mooneye vblank_stat_intr-GS).
        let oam_pulse = self.lcd.model == crate::emu::config::GbModel::Dmg;
        self.update_stat_line(oam_pulse, interrupts);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FpsCounter {
    #[serde(with = "crate::instant_serde")]
    timer: Instant,
    prev_frame_time: Duration,
    last_fps_update: Duration,
    frame_accum: f32,
    frame_count: u32,
    fps: f32,
}

impl Default for FpsCounter {
    fn default() -> Self {
        Self {
            timer: Instant::now(),
            prev_frame_time: Duration::ZERO,
            last_fps_update: Duration::ZERO,
            frame_accum: 0.0,
            frame_count: 0,
            fps: 0.0,
        }
    }
}

impl FpsCounter {
    #[inline(always)]
    pub fn update(&mut self) {
        let now = self.timer.elapsed();
        let frame_time = (now - self.prev_frame_time).as_secs_f32();
        self.prev_frame_time = now;
        self.frame_accum += frame_time;
        self.frame_count += 1;

        if (now - self.last_fps_update).as_secs_f32() >= 1.0 {
            self.fps = if self.frame_accum > 0.0 {
                self.frame_count as f32 / self.frame_accum
            } else {
                0.0
            };

            self.last_fps_update = now;
            self.frame_count = 0;
            self.frame_accum = 0.0;
        }
    }

    #[inline(always)]
    pub fn get(&self) -> f32 {
        self.fps
    }
}
