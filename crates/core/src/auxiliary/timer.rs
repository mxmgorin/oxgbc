use crate::cpu::interrupts::{InterruptType, Interrupts};
use serde::{Deserialize, Serialize};

pub const TIMER_DIV_ADDRESS: u16 = 0xFF04;
pub const TIMER_TIMA_ADDRESS: u16 = 0xFF05;
pub const TIMER_TMA_ADDRESS: u16 = 0xFF06;
pub const TIMER_TAC_ADDRESS: u16 = 0xFF07;
pub const TIMER_TAC_M_CYCLES: [usize; 4] = [256, 4, 16, 64];
pub const TIMER_TAC_UNUSED_MASK: u8 = 0b1111_1000;

// with 3 passes
// test mooneye::test_timer_rapid_toggle
// test mooneye::test_timer_tima_reload
// but fails
// test mooneye::tma_write_reloading
// test mooneye::tma_write_reloading

// with 4 passes
// test mooneye::tma_write_reloading
// test mooneye::tma_write_reloading
// but fails
// test mooneye::test_timer_rapid_toggle
// test mooneye::test_timer_tima_reload
const TIMA_RELOAD_DELAY_TICKS: usize = 4; // seems like must be 4 (1 M-cycle delay)
const TAC_ENABLE_BIT: u8 = 2;

// #1 During the strange cycle [A] you can prevent the IF flag from being set and prevent the TIMA from
// being reloaded from TMA by writing a value to TIMA. That new value will be the one that stays in
// the TIMA register after the instruction. Writing to DIV, TAC or other registers won't prevent the
// IF flag from being set or TIMA from being reloaded.

// #2 If you write to TIMA during the cycle that TMA is being loaded to it [B], the write will be
// ignored and TMA value will be written to TIMA instead.

// #3 If register IF is written during [B], the written value will overwrite the automatic flag set
// to '1'. If a '0' is written during this cycle, the interrupt won't happen.

// #4 If TMA is written the same cycle it is loaded to TIMA [B], TIMA is also loaded with that value.

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FallingEdgeDetector {
    pub prev_result: bool,
}

/// DIV masks of the TAC clock bits, indexed by TAC bits 0-1:
/// 4096 Hz (bit 9), 262144 Hz (bit 3), 65536 Hz (bit 5), 16384 Hz (bit 7).
const TAC_CLOCK_MASKS: [u16; 4] = [1 << 9, 1 << 3, 1 << 5, 1 << 7];

/// The falling-edge detector's input: the selected DIV bit AND the TAC enable
/// bit. Shared by the per-tick detector and the batch `advance` skip logic.
#[inline(always)]
fn detector_input(div: u16, tac: u8) -> bool {
    // all-ones when the TAC enable bit is set, zero otherwise
    let enabled = 0u16.wrapping_sub(((tac >> TAC_ENABLE_BIT) & 1) as u16);
    div & TAC_CLOCK_MASKS[(tac & 0b11) as usize] & enabled != 0
}

impl FallingEdgeDetector {
    /// Branchless on purpose: this runs on every T-cycle.
    #[inline(always)]
    pub fn detect(&mut self, div: u16, tac: u8) -> bool {
        let and_result = detector_input(div, tac);

        let is_falling_edge = self.prev_result && !and_result;
        self.prev_result = and_result;

        is_falling_edge
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Timer {
    // registers
    div: u16,
    tima: u8,
    tma: u8,
    tac: u8,
    // additional info
    falling_edge_detector: FallingEdgeDetector,
    tima_overflow_tma_write: Option<u8>,
    tima_overflow_ticks: Option<usize>,
    disabling_glitch: bool,
    /// A DIV/TAC write leaves `prev_result` deliberately stale (the reset/
    /// disable glitch), so the next tick must run for real. Set by those
    /// writes, consumed by `advance`; always false between instructions, so
    /// it stays out of the savestate.
    #[serde(skip)]
    dirty: bool,
}

impl Default for Timer {
    fn default() -> Self {
        Self {
            // This value depends on the model. For the original Game Boy (DMG) it is 0xABCC.
            div: 0xABCC,
            tima: 0,
            tma: 0,
            tac: 0,
            tima_overflow_tma_write: None,
            falling_edge_detector: Default::default(),
            tima_overflow_ticks: None,
            disabling_glitch: false,
            dirty: false,
        }
    }
}

impl Timer {
    /// The serial clock is divided from this same counter: 8192 Hz (bit 8) or
    /// the CGB fast 262144 Hz clock (bit 3).
    #[inline(always)]
    pub fn serial_clock_bit(&self, fast: bool) -> bool {
        let mask = if fast { 1 << 3 } else { 1 << 8 };
        self.div & mask != 0
    }

    /// DIV-APU: the frame sequencer is clocked by the falling edge of this
    /// bit of the internal counter — bit 12 (DIV bit 4) at normal speed,
    /// bit 13 (DIV bit 5) in CGB double speed, keeping it at 512 Hz.
    #[inline(always)]
    pub fn div_apu_bit(&self, double_speed: bool) -> bool {
        let mask = if double_speed { 1 << 13 } else { 1 << 12 };
        self.div & mask != 0
    }

    /// Post-boot DIV phase differs per model (mooneye boot_div-dmgABCmgb /
    /// boot_div-cgbABCDE).
    pub fn set_boot_phase(&mut self, model: crate::emu::config::GbModel) {
        self.div = match model {
            crate::emu::config::GbModel::Dmg => 0xABCC,
            crate::emu::config::GbModel::Cgb => 0x267A,
        };
    }

    /// The internal 16-bit counter as-is (not the DIV view). Exact at
    /// instruction boundaries: `advance` always consumes the whole window.
    #[inline(always)]
    pub fn raw_div(&self) -> u16 {
        self.div
    }

    /// Ticks to the next falling edge of the selected TAC bit (no writes in
    /// between). Rise and fall are both `2 * mask` crossings; the fall is the
    /// one where the low bits wrap to zero.
    #[inline(always)]
    fn next_falling_edge_offset(&self) -> usize {
        let period = (TAC_CLOCK_MASKS[(self.tac & 0b11) as usize] as usize) << 1;

        period - (self.div as usize & (period - 1))
    }

    /// Batch equivalent of `ticks × tick()`: skips edge-free spans
    /// arithmetically, runs the real `tick()` only where something stateful can
    /// happen — a falling edge, the TIMA overflow pipeline, or the tick after a
    /// DIV/TAC write (`dirty`). Semantics stay in `tick()`; the only new logic
    /// is the edge-offset formula, which the exhaustive test sweeps.
    pub fn advance(&mut self, mut ticks: usize, interrupts: &mut Interrupts) {
        while ticks > 0 {
            // anything stateful in flight → execute ticks for real
            if self.dirty || self.disabling_glitch || self.tima_overflow_ticks.is_some() {
                self.dirty = false;
                self.tick(interrupts);
                ticks -= 1;
                continue;
            }

            // disabled: input stuck low, no edges — the rest is one counter add
            if !self.is_enabled() {
                self.div = self.div.wrapping_add(ticks as u16);
                return;
            }

            let offset = self.next_falling_edge_offset();
            if offset > ticks {
                // no edge in window: jump, keeping the latch consistent
                // (the bit may have risen inside the span)
                self.div = self.div.wrapping_add(ticks as u16);
                self.falling_edge_detector.prev_result = detector_input(self.div, self.tac);
                return;
            }

            // jump to the tick before the edge, then execute the edge exactly
            self.div = self.div.wrapping_add((offset - 1) as u16);
            self.falling_edge_detector.prev_result = detector_input(self.div, self.tac);
            self.tick(interrupts);
            ticks -= offset;
        }
    }

    /// T-cycles until the tick that raises the Timer IF bit, assuming no
    /// register writes land in between — the HALT fast-forward bound
    /// (writes can't happen: the CPU is halted). `usize::MAX` when no
    /// overflow is on the way. Compares and shifts only.
    pub fn if_horizon(&self) -> usize {
        // a pending write glitch resolves on the next real tick — don't
        // reason past it (never set at a halt boundary, but cheap to honor)
        if self.dirty || self.disabling_glitch {
            return 1;
        }

        // reload pipeline in flight: IF fires on the tick that sees the
        // counter at the delay value
        if let Some(k) = self.tima_overflow_ticks {
            return TIMA_RELOAD_DELAY_TICKS - k + 1;
        }

        if !self.is_enabled() {
            return usize::MAX;
        }

        // increments until TIMA wraps, then the reload delay
        let period = (TAC_CLOCK_MASKS[(self.tac & 0b11) as usize] as usize) << 1;
        let increments = 0x100 - self.tima as usize;
        self.next_falling_edge_offset() + (increments - 1) * period + TIMA_RELOAD_DELAY_TICKS + 1
    }

    #[inline]
    pub fn tick(&mut self, interrupts: &mut Interrupts) {
        // TIMA overflowed during the last cycle
        if let Some(tima_overflow_ticks) = self.tima_overflow_ticks.as_mut() {
            if *tima_overflow_ticks == TIMA_RELOAD_DELAY_TICKS || self.disabling_glitch {
                self.tima = self.tma;
                interrupts.request_interrupt(InterruptType::Timer);

                // reset after overflow fully handled
                self.tima_overflow_ticks = None;
                self.disabling_glitch = false;
            } else {
                *tima_overflow_ticks += 1;
            }
        }

        self.div = self.div.wrapping_add(1);

        if self.falling_edge_detector.detect(self.div, self.tac) {
            self.inc_tima();
        }
    }

    #[inline(always)]
    fn inc_tima(&mut self) {
        let (tima, tima_overflow) = self.tima.overflowing_add(1);
        self.write_tima(tima);

        if tima_overflow && self.tima_overflow_ticks.is_none() {
            // Timer interrupt is delayed 4 ticks from the TIMA overflow.
            // The TMA reload to TIMA is also delayed for 1 tick.
            // After overflowing TIMA, the value in TIMA is 00, not TMA.
            self.tima = 0x00;
            self.tima_overflow_ticks = Some(0);
        }
    }

    #[inline(always)]
    fn get_clock_bit_position(&self) -> u8 {
        match self.tac & 0b11 {
            // 0b00 (4096 Hz): div bit 9, increment every 256 M-cycles
            0b00 => 9,
            // 0b01 (262144 Hz): div bit 3, increment every 4 M-cycles
            0b01 => 3,
            // 0b10 (65536 Hz): div bit 5, increment every 16 M-cycles
            0b10 => 5,
            // 0b11 (16384 Hz): div bit 7, increment every 64 M-cycles
            0b11 => 7,
            _ => unreachable!(),
        }
    }

    #[inline(always)]
    fn is_enabled(&self) -> bool {
        // If bit 2 of TAC is set to 0 then the timer is disabled
        self.tac & (1 << TAC_ENABLE_BIT) != 0
    }

    #[inline(always)]
    pub fn write(&mut self, address: u16, value: u8) {
        match address {
            TIMER_DIV_ADDRESS => self.reset_div(),
            TIMER_TIMA_ADDRESS => self.write_tima(value),
            TIMER_TMA_ADDRESS => {
                if self.tima_overflow_ticks.is_some() {
                    self.tima_overflow_tma_write = Some(value);
                }

                self.tma = value;
            }
            TIMER_TAC_ADDRESS => self.write_tac(value),
            _ => panic!("Invalid Timer address: {address:02X}"),
        }
    }

    #[inline(always)]
    fn write_tima(&mut self, value: u8) {
        if let Some(overflow_ticks) = self.tima_overflow_ticks {
            if overflow_ticks == TIMA_RELOAD_DELAY_TICKS {
                // case #2: the same tick on which the reload occurs - ignore write
                self.tima = self.tma;
                return;
            } else {
                // case #1: write during 4-ticks delay - abort handling
                self.tima_overflow_ticks = None;
            }
        }

        self.tima = value;
    }

    #[inline(always)]
    pub fn reset_div(&mut self) {
        self.div = 0;
        // The stale latch IS the reset glitch (a high selected bit falls with
        // the counter); the next tick must run for real to fire it.
        self.dirty = true;

        // - When writing to DIV register the TIMA register can be increased if the counter has reached half
        // the clocks it needs to increase because the selected bit by the multiplexer will go from 1 to 0 (which
        // is a falling edge, that will be detected by the falling edge detector).
        //if self.is_enabled() && self.is_falling_edge(self.prev_div) {
        //    self.inc_tima();
        //}
    }

    #[inline(always)]
    pub fn write_tac(&mut self, value: u8) {
        let old_is_enabled = self.is_enabled();
        let old_clock_bit = self.get_clock_bit_position();

        self.tac = value;
        // the detector latch is stale against the new clock select / enable:
        // the next tick must run for real (see `advance`)
        self.dirty = true;

        let new_is_enabled = self.is_enabled();

        // - When disabling the timer, if the corresponding bit in the system counter is set to 1, the falling edge
        // detector will see a change from 1 to 0, so TIMA will increase. This means that whenever half the
        // clocks of the count are reached, TIMA will increase when disabling the timer.
        // Correctly emulated by detect_falling_edge

        // fix rapid_toggle
        self.disabling_glitch =
            (self.div & (1 << old_clock_bit)) != 0 && old_is_enabled && !new_is_enabled;

        if !self.disabling_glitch {
            // - When changing TAC register value, if the old selected bit by the multiplexer was 0, the new one is
            // 1, and the new enable bit of TAC is set to 1, it will increase TIMA.
            let enabling_glitch = (self.div & (1 << old_clock_bit)) == 0
                && (self.div & (1 << self.get_clock_bit_position())) != 0
                && new_is_enabled;

            if enabling_glitch {
                self.inc_tima();
            }
        }
    }

    #[inline(always)]
    pub fn read(&self, address: u16) -> u8 {
        match address {
            TIMER_DIV_ADDRESS => {
                #[cfg(debug_assertions)]
                if std::env::var_os("OXGBC_TRACE_DIV").is_some() {
                    eprintln!("RD DIV counter={:04X}", self.div);
                }

                (self.div >> 8) as u8 // most significant byte in a 16-bit long number
            }
            TIMER_TIMA_ADDRESS => {
                // fix for tima_reload
                if let Some(overflow_ticks) = self.tima_overflow_ticks {
                    if overflow_ticks == TIMA_RELOAD_DELAY_TICKS {
                        return self.tma;
                    }
                }

                self.tima
            }
            TIMER_TMA_ADDRESS => self.tma,
            TIMER_TAC_ADDRESS => self.tac | TIMER_TAC_UNUSED_MASK,
            _ => panic!("Invalid Timer address: {address:02X}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::auxiliary::timer::Timer;
    use crate::cpu::interrupts::Interrupts;

    /// `advance(n)` must be bit-identical to `n × tick()` — swept over the
    /// whole u16 counter domain, every TAC value, and a TIMA about to overflow
    /// (so the reload pipeline runs in the batch path too).
    #[test]
    fn test_advance_equals_ticks_exhaustive() {
        let mut interrupts_a = Interrupts::default();
        let mut interrupts_b = Interrupts::default();

        for tac in 0u8..=0b111 {
            for div in 0u16..=0xFFFF {
                let mut a = Timer {
                    div,
                    tac,
                    tima: 0xFE, // one edge from the overflow pipeline
                    tma: 0xAB,
                    ..Timer::default()
                };
                // seed the detector latch consistently, like a running system
                a.falling_edge_detector.prev_result = super::detector_input(div, tac);
                let mut b = a.clone();

                interrupts_a.int_flags = 0;
                interrupts_b.int_flags = 0;

                for _ in 0..40 {
                    a.tick(&mut interrupts_a);
                }
                b.advance(40, &mut interrupts_b);

                assert_eq!(a.div, b.div, "div: start div={div:#06x} tac={tac:#05b}");
                assert_eq!(a.tima, b.tima, "tima: start div={div:#06x} tac={tac:#05b}");
                assert_eq!(
                    a.falling_edge_detector.prev_result, b.falling_edge_detector.prev_result,
                    "latch: start div={div:#06x} tac={tac:#05b}"
                );
                assert_eq!(
                    a.tima_overflow_ticks, b.tima_overflow_ticks,
                    "pipeline: start div={div:#06x} tac={tac:#05b}"
                );
                assert_eq!(
                    interrupts_a.int_flags, interrupts_b.int_flags,
                    "IF: start div={div:#06x} tac={tac:#05b}"
                );
            }
        }
    }

    /// The DIV-write glitch must survive the batch path: a high selected bit
    /// falling with the counter reset increments TIMA on the next tick.
    #[test]
    fn test_advance_div_write_glitch() {
        for tac in [0b100u8, 0b101, 0b110, 0b111] {
            let mut a = Timer {
                div: 0xFFFF, // every selected bit high
                tac,
                tima: 5,
                ..Timer::default()
            };
            a.falling_edge_detector.prev_result = super::detector_input(a.div, tac);
            let mut b = a.clone();
            let mut ia = Interrupts::default();
            let mut ib = Interrupts::default();

            a.reset_div();
            b.reset_div();
            for _ in 0..8 {
                a.tick(&mut ia);
            }
            b.advance(8, &mut ib);

            assert_eq!(a.tima, 6, "glitch must increment tima (tac={tac:#05b})");
            assert_eq!(a.tima, b.tima, "tac={tac:#05b}");
            assert_eq!(a.div, b.div, "tac={tac:#05b}");
        }
    }

    /// `if_horizon` must point at the exact tick that raises the Timer IF:
    /// every enabled TAC over a counter sweep with TIMA near the overflow
    /// (bounded walk), the full TIMA range on the fastest clock, and a
    /// mid-pipeline state — all verified against the per-tick chain.
    #[test]
    fn test_if_horizon_exact() {
        let assert_fires_at = |mut t: Timer, ctx: &str| {
            let h = t.if_horizon();
            assert_ne!(h, usize::MAX, "enabled timer always overflows ({ctx})");
            let mut i = Interrupts::default();
            for tick in 1..=h {
                t.tick(&mut i);
                let fired = i.int_flags & 0b100 != 0;
                assert_eq!(fired, tick == h, "IF at tick {tick}, horizon {h} ({ctx})");
            }
        };

        // every clock select, counter phases across the slowest period
        for tac in [0b100u8, 0b101, 0b110, 0b111] {
            for div_step in 0..64u16 {
                let div = div_step.wrapping_mul(641); // spread over the u16 domain
                for tima in [0xFEu8, 0xFF] {
                    let mut t = Timer {
                        div,
                        tac,
                        tima,
                        tma: 0x42,
                        ..Timer::default()
                    };
                    t.falling_edge_detector.prev_result = super::detector_input(div, tac);
                    assert_fires_at(
                        t,
                        &format!("div={div:#06x} tac={tac:#05b} tima={tima:#04x}"),
                    );
                }
            }
        }

        // full TIMA range on the fastest clock (period 16)
        for tima in 0u8..=0xFF {
            let mut t = Timer {
                div: 0x1234,
                tac: 0b101,
                tima,
                tma: 0x42,
                ..Timer::default()
            };
            t.falling_edge_detector.prev_result = super::detector_input(t.div, t.tac);
            assert_fires_at(t, &format!("tima={tima:#04x}"));
        }

        // disabled timer without a pipeline never fires
        let t = Timer {
            tac: 0b010,
            ..Timer::default()
        };
        assert_eq!(t.if_horizon(), usize::MAX);

        // mid-pipeline: horizon counts down through the reload delay
        let mut t = Timer {
            tac: 0b101,
            tima: 0xFF,
            ..Timer::default()
        };
        t.falling_edge_detector.prev_result = super::detector_input(t.div, t.tac);
        let mut i = Interrupts::default();
        // walk to the overflow edge so the pipeline is live
        while t.tima_overflow_ticks.is_none() {
            t.tick(&mut i);
        }
        let h = t.if_horizon();
        for tick in 1..=h {
            t.tick(&mut i);
            assert_eq!(
                i.int_flags & 0b100 != 0,
                tick == h,
                "pipeline tick {tick} of {h}"
            );
        }
    }

    #[test]
    pub fn test_timer_tima_01() {
        let mut timer = Timer {
            tac: 0b101,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();
        let mut prev_tima = 0;
        let cycles = 16;

        for i in 1..=500 {
            timer.tick(&mut interrupts);

            if prev_tima != timer.tima {
                assert_eq!(i % cycles, 0);
            }

            if i == cycles {
                assert_eq!(timer.tima, (cycles / i) as u8);
            }

            prev_tima = timer.tima;
        }
    }

    #[test]
    pub fn test_timer_tima_10() {
        let mut timer = Timer {
            tac: 0b110,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();
        let mut prev_tima = 0;
        let cycles = 64;

        for i in 1..=1000_usize {
            timer.tick(&mut interrupts);

            if prev_tima != timer.tima {
                assert_eq!(i % cycles, 0);
            }

            if i == cycles {
                assert_eq!(timer.tima, (cycles / i) as u8);
            }

            prev_tima = timer.tima;
        }
    }

    #[test]
    pub fn test_timer_tima_11() {
        let mut timer = Timer {
            tac: 0b111,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();
        let mut prev_tima = 0;
        let cycles = 256;

        for i in 1..=10000_usize {
            timer.tick(&mut interrupts);

            if prev_tima != timer.tima {
                assert_eq!(i % cycles, 0);
            }

            if i == cycles {
                assert_eq!(timer.tima, (cycles / i) as u8);
            }

            prev_tima = timer.tima;
        }
    }

    #[test]
    pub fn test_timer_tima_00() {
        let mut timer = Timer {
            tac: 0b100,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();
        let mut prev_tima = 0;
        let cycles = 1024;

        for i in 1..=100000_usize {
            timer.tick(&mut interrupts);

            if prev_tima != timer.tima {
                assert_eq!(i % cycles, 0);
            }

            if i == cycles {
                assert_eq!(timer.tima, (cycles / i) as u8);
            }

            prev_tima = timer.tima;
        }
    }

    #[test]
    pub fn test_timer_tima_00_trigger() {
        let mut timer = Timer {
            tac: 0b100,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();

        for _ in 1..=512 {
            timer.tick(&mut interrupts);
        }

        timer.reset_div();
        timer.tick(&mut interrupts);

        assert_eq!(1, timer.tima);
    }

    #[test]
    pub fn test_timer_tima_01_trigger() {
        let mut timer = Timer {
            tac: 0b101,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();

        for _ in 1..=8 {
            timer.tick(&mut interrupts);
        }

        timer.reset_div();
        timer.tick(&mut interrupts);

        assert_eq!(1, timer.tima);
    }

    #[test]
    pub fn test_timer_tima_10_trigger() {
        let mut timer = Timer {
            tac: 0b110,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();

        for _ in 1..=32 {
            timer.tick(&mut interrupts);
        }

        timer.reset_div();
        timer.tick(&mut interrupts);

        assert_eq!(1, timer.tima);
    }

    #[test]
    pub fn test_timer_tima_11_trigger() {
        let mut timer = Timer {
            tac: 0b111,
            div: 0,

            ..Timer::default()
        };
        let mut interrupts = Interrupts::default();

        for _ in 1..=128 {
            timer.tick(&mut interrupts);
        }

        timer.reset_div();
        timer.tick(&mut interrupts);

        assert_eq!(1, timer.tima);
    }
}
