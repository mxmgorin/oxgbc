//! Two ways to advance one clock window. `advance_window` is the
//! event-scheduler path: batch the window, each device skipping to its next
//! event (the skip-ahead lives in the devices' own `advance`); this module
//! only picks what runs per window and in what order. `advance_per_tick` is
//! the pre-migration per-tick chain, behind the `per-tick-clock` feature for
//! A/B benches and whole-chain equivalence.

use crate::apu::DIV_APU_BIT;
use crate::auxiliary::clock::{Clock, T_CYCLES_PER_M_CYCLE};
use crate::auxiliary::dma::{OamDma, VramDma};
#[cfg(not(feature = "per-tick-clock"))]
use crate::cpu::interrupts::InterruptType;

/// Upper bound on one HALT jump, in T-cycles: keeps the CPU stepping at a sane
/// cadence when no IF-capable event is on the horizon at all (LCD off + timer
/// off), so external inputs and frame pacing still get their step boundaries.
#[cfg(not(feature = "per-tick-clock"))]
const HALT_HORIZON_CAP: usize = 256 * T_CYCLES_PER_M_CYCLE;

/// One batched window: devices skip edge-free spans and replay `tick()` only
/// at their edges. Legal because nothing inside a window observes them — the
/// CPU reads state only between windows, DIV via the closed-form `div0 + i`,
/// IF bits OR-accumulate.
#[inline(always)]
pub fn advance_window(clock: &mut Clock, m_cycles: usize) {
    let ticks = m_cycles * T_CYCLES_PER_M_CYCLE;
    let div0 = clock.bus.io.timer.raw_div();

    clock
        .bus
        .io
        .timer
        .advance(ticks, &mut clock.bus.io.interrupts);

    // The serial edge detector only matters mid-transfer; its idle state
    // is re-seeded on the SC write that starts one.
    if clock.bus.io.serial.is_active() {
        clock
            .bus
            .io
            .serial
            .advance(div0, ticks, &mut clock.bus.io.interrupts);
    }

    clock.m_cycles = clock.m_cycles.wrapping_add(m_cycles);

    // APU: its DIV-APU bit is a pure function of window-start DIV, it raises
    // no IF, and NR52/PCM read between windows — nothing inside observes it.
    let (dev_ticks, v_first, step, shift) = if clock.bus.io.cgb_speed.double_speed {
        // device ticks land on every other master tick; which ones is
        // decided by the double_speed_phase parity at window entry
        let offset: u16 = if clock.double_speed_phase { 1 } else { 2 };
        (ticks / 2, div0.wrapping_add(offset), 2, DIV_APU_BIT + 1)
    } else {
        (ticks, div0.wrapping_add(1), 1, DIV_APU_BIT)
    };
    clock.bus.io.apu.advance(dev_ticks, v_first, step, shift);

    // Both DMAs only leave idle via IO write, between windows: both idle here
    // means DMA-free throughout, so the PPU batch-advances too. Otherwise
    // replay dot-by-dot — OAM/VRAM-DMA accesses interleave with the PPU's.
    if clock.bus.oam_dma.is_active || !clock.bus.vram_dma.is_idle() {
        run_devices(clock, ticks);
    } else {
        // Phases stay untouched: windows are multiples of 4 T-cycles, so both
        // parities are invariant across a whole window (dev_ticks is even).
        clock
            .bus
            .io
            .ppu
            .advance(dev_ticks, &mut clock.bus.io.interrupts);
    }
}

/// A DMA-active window: OAM-DMA, VRAM-DMA and PPU replay per-tick so their
/// memory accesses interleave exactly as on hardware.
#[inline(always)]
fn run_devices(clock: &mut Clock, ticks: usize) {
    let double_speed = clock.bus.io.cgb_speed.double_speed;

    for i in 0..ticks {
        if i % T_CYCLES_PER_M_CYCLE == 0 {
            OamDma::tick(&mut clock.bus);
        }

        // PPU/VRAM-DMA run on the fixed 4 MHz clock — every other CPU T-cycle
        // in double speed, phase-continuous.
        if double_speed {
            clock.double_speed_phase = !clock.double_speed_phase;

            if clock.double_speed_phase {
                continue;
            }
        }

        clock.device_phase = !clock.device_phase;

        if clock.device_phase && !clock.cpu_halted {
            VramDma::tick(&mut clock.bus);
        }

        clock.bus.io.ppu.tick(&mut clock.bus.io.interrupts);
    }
}

/// M-cycles a halted CPU can jump without missing a wake-up: the minimum over
/// the IF-capable device horizons, filtered by IE — a disabled interrupt's
/// event still happens inside the window (the devices replay it), it just
/// can't wake the CPU, so the jump need not stop there. IE is stable while
/// halted (only the CPU writes it). Rounding up to the wake event's M-cycle
/// reproduces the per-M-cycle wait bit-for-bit. Joypad is injected between
/// steps and needs no horizon; the APU raises no IF.
#[cfg(not(feature = "per-tick-clock"))]
pub fn halt_horizon(clock: &Clock) -> usize {
    // DMA-active windows replay per-dot; keep them at today's cadence
    // (GDMA also parks the CPU through `is_cpu_halted`).
    if clock.bus.oam_dma.is_active || !clock.bus.vram_dma.is_idle() {
        return 1;
    }

    let io = &clock.bus.io;
    let ie = io.interrupts.ie;
    let mut t = HALT_HORIZON_CAP;

    if ie & InterruptType::Timer as u8 != 0 {
        t = t.min(io.timer.if_horizon());
    }

    if ie & InterruptType::Serial as u8 != 0 {
        t = t.min(io.serial.if_horizon(io.timer.raw_div()));
    }

    if ie & (InterruptType::VBlank as u8 | InterruptType::LCDStat as u8) != 0 {
        // dots are 4 MHz device ticks: 1 T-cycle at normal speed, 2 in
        // double speed. ×2 may overshoot the event by one T-cycle when
        // the dot grid sits between M-cycles, but never past the event's
        // M-cycle boundary (an odd true offset is never a multiple of 4).
        let dots = io.ppu.dots_to_next_event();
        let factor = 1 + io.cgb_speed.double_speed as usize;
        t = t.min(dots.saturating_mul(factor));
    }

    // stop exactly on the wake event's M-cycle
    t.div_ceil(T_CYCLES_PER_M_CYCLE).max(1)
}

/// The reference chain keeps the per-M-cycle halt wait.
#[cfg(feature = "per-tick-clock")]
pub fn halt_horizon(_clock: &Clock) -> usize {
    1
}

/// The original per-T-cycle chain (order: OamDma, Timer, Serial, VramDma,
/// Ppu, Apu), kept verbatim as the `per-tick-clock` reference build.
#[cfg(feature = "per-tick-clock")]
#[inline(always)]
pub fn advance_per_tick(clock: &mut Clock, m_cycles: usize) {
    for _ in 0..m_cycles {
        clock.m_cycles = clock.m_cycles.wrapping_add(1);
        OamDma::tick(&mut clock.bus);

        for _ in 0..T_CYCLES_PER_M_CYCLE {
            clock.bus.io.timer.tick(&mut clock.bus.io.interrupts);
            // The serial edge detector only matters mid-transfer; its
            // idle state is re-seeded on the SC write that starts one.
            if clock.bus.io.serial.is_active() {
                let sclk = clock
                    .bus
                    .io
                    .timer
                    .serial_clock_bit(clock.bus.io.serial.is_fast_clock());
                clock.bus.io.serial.tick(sclk, &mut clock.bus.io.interrupts);
            }

            // PPU/APU/VRAM-DMA run on the fixed 4 MHz clock — every other CPU
            // T-cycle in double speed, phase-continuous.
            if clock.bus.io.cgb_speed.double_speed {
                clock.double_speed_phase = !clock.double_speed_phase;

                if clock.double_speed_phase {
                    continue;
                }
            }

            clock.device_phase = !clock.device_phase;

            if clock.device_phase && !clock.cpu_halted {
                VramDma::tick(&mut clock.bus);
            }

            clock.bus.io.ppu.tick(&mut clock.bus.io.interrupts);
            let div_apu_bit = clock
                .bus
                .io
                .timer
                .div_apu_bit(clock.bus.io.cgb_speed.double_speed);
            clock.bus.io.apu.tick(div_apu_bit);
        }
    }
}
