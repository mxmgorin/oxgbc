use crate::auxiliary::scheduler;
use crate::bus::Bus;
use crate::cpu::CPU_CLOCK_SPEED;
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
    pub m_cycles: usize,
    /// Toggles every CPU T-cycle in double speed: PPU/APU/VRAM-DMA sit on the
    /// fixed 4 MHz clock, so they tick on every other CPU T-cycle,
    /// phase-continuous across M-cycles (not in per-M-cycle bursts).
    #[serde(default)]
    pub double_speed_phase: bool,
    /// Parity of device (4 MHz) ticks, drives the 2 MHz VRAM-DMA cadence.
    #[serde(default)]
    pub device_phase: bool,
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
            double_speed_phase: false,
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

    /// Advance the whole system by `m_cycles`. The window strategy lives in
    /// `scheduler`: batched skip-ahead by default, the per-tick reference chain
    /// behind the `per-tick-clock` feature (A/B benches + whole-chain equivalence).
    #[inline(always)]
    pub fn tick_m_cycles(&mut self, m_cycles: usize) {
        #[cfg(not(feature = "per-tick-clock"))]
        scheduler::advance_window(self, m_cycles);

        #[cfg(feature = "per-tick-clock")]
        scheduler::advance_per_tick(self, m_cycles);
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
