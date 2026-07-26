use crate::apu::DIV_APU_BIT;
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
    double_speed_phase: bool,
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

    /// Advance the whole system by `m_cycles`. Timer and serial batch-advance
    /// over the window (skip edge-free spans, replay `tick()` at edges): legal
    /// because nothing inside observes them — the CPU reads only between
    /// windows, others see DIV via the closed-form `div0 + i`, IF OR-accumulates.
    /// APU and the PPU batch-advance the same way; a DMA-active window instead
    /// replays per device so memory accesses interleave (see `run_devices`).
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

        self.m_cycles = self.m_cycles.wrapping_add(m_cycles);

        // APU: batch-advance the window. DIV-APU bits are a pure function of
        // window-start DIV, it raises no IF, and NR52/PCM read between windows
        // — nothing inside observes it.
        // In double speed the device ticks every other master tick (double_speed_phase
        // parity picks which), so DIV — counted at the master rate — advances 2
        // per device tick and the DIV-APU bit shifts up one. Single speed: 1:1.
        let (dev_ticks, v_first, step, shift) = if self.bus.io.cgb_speed.double_speed {
            let offset: u16 = if self.double_speed_phase { 1 } else { 2 };
            (ticks / 2, div0.wrapping_add(offset), 2, DIV_APU_BIT + 1)
        } else {
            (ticks, div0.wrapping_add(1), 1, DIV_APU_BIT)
        };
        self.bus.io.apu.advance(dev_ticks, v_first, step, shift);

        // DMAs only leave idle via IO write, between windows: both idle at
        // window start means DMA-free throughout, so the PPU batch-advances
        // to its line events. DMA-active windows still replay dot-by-dot —
        // OAM-DMA and VRAM-DMA interleave with PPU accesses.
        if self.bus.oam_dma.is_active || !self.bus.vram_dma.is_idle() {
            self.run_devices(ticks);
        } else {
            // double_speed_phase/device_phase stay untouched: windows are multiples of
            // 4 T-cycles, so both parities are invariant across a whole
            // window on either path (dev_ticks is even).
            self.bus.io.ppu.advance(dev_ticks, &mut self.bus.io.interrupts);
        }
    }

    /// One DMA-active window: OAM-DMA, VRAM-DMA and PPU replay per-tick so
    /// their memory accesses interleave exactly as on hardware.
    #[inline(always)]
    fn run_devices(&mut self, ticks: usize) {
        let double_speed = self.bus.io.cgb_speed.double_speed;

        for i in 0..ticks {
            if i % T_CYCLES_PER_M_CYCLE == 0 {
                OamDma::tick(&mut self.bus);
            }

            // PPU/VRAM-DMA run on the fixed 4 MHz clock — every other CPU
            // T-cycle in double speed, phase-continuous.
            if double_speed {
                self.double_speed_phase = !self.double_speed_phase;

                if self.double_speed_phase {
                    continue;
                }
            }

            self.device_phase = !self.device_phase;

            if self.device_phase && !self.cpu_halted {
                VramDma::tick(&mut self.bus);
            }

            self.bus.io.ppu.tick(&mut self.bus.io.interrupts);
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
