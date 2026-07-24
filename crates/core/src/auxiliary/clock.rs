use crate::auxiliary::dma::VramDma;
use crate::bus::Bus;
use crate::{auxiliary::dma::OamDma, cpu::CPU_CLOCK_SPEED};
use serde::{Deserialize, Serialize};
use web_time::Instant;

pub const T_CYCLES_PER_M_CYCLE: usize = 4;
const NANOS_PER_SECOND: u32 = 1_000_000_000;
const T_CYCLE_DURATION_NANOS: f64 = NANOS_PER_SECOND as f64 / CPU_CLOCK_SPEED as f64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clock {
    #[serde(with = "crate::instant_serde")]
    pub time: Instant,
    pub bus: Bus,
    pub cpu_halted: bool,
    m_cycles: usize,
    /// Toggles every CPU T-cycle in double speed: PPU/APU/VRAM-DMA sit on the
    /// fixed 4 MHz clock, so they tick on every other CPU T-cycle,
    /// phase-continuous across M-cycles (not in per-M-cycle bursts).
    #[serde(default)]
    ds_phase: bool,
    /// Parity of device (4 MHz) ticks, drives the 2 MHz VRAM-DMA cadence.
    #[serde(default)]
    device_phase: bool,
}

impl Default for Clock {
    fn default() -> Self {
        Self::new(Default::default())
    }
}

impl Clock {
    pub fn new(bus: Bus) -> Self {
        Self {
            time: Instant::now(),
            bus,
            cpu_halted: false,
            m_cycles: 0,
            ds_phase: false,
            device_phase: false,
        }
    }

    #[inline(always)]
    pub fn is_cpu_halted(&self) -> bool {
        self.cpu_halted || self.bus.vram_dma.is_transferring()
    }

    #[inline(always)]
    pub fn calc_elapsed_nanos(&self) -> f64 {
        self.get_t_cycles() as f64 * self.get_t_cycle_duration_nanos()
    }

    #[inline(always)]
    pub fn reset(&mut self) {
        self.m_cycles = 0;
        self.time = Instant::now();
    }

    /// Advance the whole system by `m_cycles`. Timer and serial batch-advance
    /// over the window (skip edge-free spans, replay `tick()` at edges): legal
    /// because nothing inside observes them — the CPU reads only between
    /// windows, others see DIV via the closed-form `div0 + i`, IF OR-accumulates.
    /// PPU/APU/DMA still tick per device (event-ized in later stages).
    #[inline(always)]
    pub fn tick_m_cycles(&mut self, m_cycles: usize) {
        let ticks = m_cycles * T_CYCLES_PER_M_CYCLE;
        let div0 = self.bus.io.timer.raw_div();

        self.bus.io.timer.advance(ticks, &mut self.bus.io.interrupts);

        // Serial edges only matter mid-transfer; idle is re-seeded on SC write.
        if self.bus.io.serial.is_active() {
            self.bus
                .io
                .serial
                .advance(div0, ticks, &mut self.bus.io.interrupts);
        }

        let double_speed = self.bus.io.cgb_speed.double_speed;
        // DIV-APU bit: 12 at normal speed, 13 in double. The timer already
        // advanced, so reconstruct the per-tick value from div0 (`+ i + 1`:
        // the timer used to increment before the APU ran).
        let div_apu_shift = if double_speed { 13 } else { 12 };

        for i in 0..ticks {
            if i % T_CYCLES_PER_M_CYCLE == 0 {
                self.m_cycles = self.m_cycles.wrapping_add(1);
                OamDma::tick(&mut self.bus);
            }

            // PPU/APU/VRAM-DMA run on the fixed 4 MHz clock — every other CPU
            // T-cycle in double speed, phase-continuous.
            if double_speed {
                self.ds_phase = !self.ds_phase;

                if self.ds_phase {
                    continue;
                }
            }

            self.device_phase = !self.device_phase;

            if self.device_phase && !self.cpu_halted {
                VramDma::tick(&mut self.bus);
            }

            self.bus.io.ppu.tick(&mut self.bus.io.interrupts);
            let div_apu_bit = div0.wrapping_add(i as u16 + 1) >> div_apu_shift & 1 != 0;
            self.bus.io.apu.tick(div_apu_bit);
        }
    }

    #[inline(always)]
    pub fn get_m_cycles(&self) -> usize {
        self.m_cycles
    }

    #[inline(always)]
    pub fn get_t_cycles(&self) -> usize {
        self.m_cycles * T_CYCLES_PER_M_CYCLE
    }


    fn get_t_cycle_duration_nanos(&self) -> f64 {
        if self.bus.io.cgb_speed.double_speed {
            return T_CYCLE_DURATION_NANOS / 2.0;
        }

        T_CYCLE_DURATION_NANOS
    }
}
