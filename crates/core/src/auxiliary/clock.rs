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

    #[inline(always)]
    pub fn tick_m_cycles(&mut self, m_cycles: usize) {
        for _ in 0..m_cycles {
            self.m_cycles = self.m_cycles.wrapping_add(1);
            OamDma::tick(&mut self.bus);

            for _ in 0..T_CYCLES_PER_M_CYCLE {
                self.bus.io.timer.tick(&mut self.bus.io.interrupts);
                // The serial edge detector only matters mid-transfer; its
                // idle state is re-seeded on the SC write that starts one.
                if self.bus.io.serial.is_active() {
                    let sclk = self
                        .bus
                        .io
                        .timer
                        .serial_clock_bit(self.bus.io.serial.is_fast_clock());
                    self.bus.io.serial.tick(sclk, &mut self.bus.io.interrupts);
                }

                // PPU/APU/VRAM-DMA run on the fixed 4 MHz clock: every other
                // CPU T-cycle in double speed, phase-continuous, so a 1
                // M-cycle shift moves their observable phase by half a period.
                if self.bus.io.cgb_speed.double_speed {
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
                let div_apu_bit = self
                    .bus
                    .io
                    .timer
                    .div_apu_bit(self.bus.io.cgb_speed.double_speed);
                self.bus.io.apu.tick(div_apu_bit);
            }
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
