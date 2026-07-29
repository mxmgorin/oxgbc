//! `Apu::advance` must be bit-identical to the per-tick `tick()` chain.
//! Randomized register writes (triggers, sweeps, NR52 power cycling, wave
//! RAM) between random-size windows, plus occasional DIV-counter jumps.
//! Compared after every window: PCM12/34, every readable register, and the
//! emitted sample buffer bit-for-bit.

use core::apu::channels::wave_channel::{CH3_WAVE_RAM_END, CH3_WAVE_RAM_START};
use core::apu::{Apu, AUDIO_END_ADDRESS, AUDIO_START_ADDRESS};

mod common;
use common::Lcg;

// The two live APU register blocks. The FF27-FF2F hole between them is not
// routed to the APU by the bus, so writes pick from these and skip the gap.
const N_CTRL_REGS: u32 = (AUDIO_END_ADDRESS - AUDIO_START_ADDRESS + 1) as u32;
const N_WAVE_RAM: u32 = (CH3_WAVE_RAM_END - CH3_WAVE_RAM_START + 1) as u32;

#[test]
fn test_apu_advance_equals_ticks_randomized() {
    for seed in 0..8u64 {
        let mut rng = Lcg::seeded(seed);
        let mut a = Apu::default(); // reference: per-tick
        let mut b = Apu::default(); // candidate: advance
        let mut v: u16 = 0xABCD; // stand-in DIV counter

        for round in 0..30_000usize {
            if rng.below(4) == 0 {
                for _ in 0..=rng.below(3) {
                    let r = rng.below(N_CTRL_REGS + N_WAVE_RAM);
                    let addr = if r < N_CTRL_REGS {
                        AUDIO_START_ADDRESS + r as u16
                    } else {
                        CH3_WAVE_RAM_START + (r - N_CTRL_REGS) as u16
                    };
                    let val = rng.below(256) as u8;
                    a.write(addr, val, false);
                    b.write(addr, val, false);
                }
            }
            if rng.below(64) == 0 {
                v = rng.next() as u16; // DIV jump: the bit moves under the latch
            }

            let w = (rng.below(24) + 1) as usize;
            for j in 0..w {
                let vj = v.wrapping_add(j as u16 + 1);
                a.tick(vj >> 12 & 1 != 0);
            }
            b.advance(w, v.wrapping_add(1), 1, 12);
            v = v.wrapping_add(w as u16);

            assert_eq!(a.read_pcm12(), b.read_pcm12(), "pcm12 seed={seed} round={round}");
            assert_eq!(a.read_pcm34(), b.read_pcm34(), "pcm34 seed={seed} round={round}");
            for addr in AUDIO_START_ADDRESS..=AUDIO_END_ADDRESS {
                assert_eq!(
                    a.read(addr),
                    b.read(addr),
                    "reg {addr:04x} seed={seed} round={round}"
                );
            }
            if a.buffer_ready() || b.buffer_ready() {
                assert_eq!(a.get_buffer(), b.get_buffer(), "buffer seed={seed} round={round}");
                a.clear_buffer();
                b.clear_buffer();
            }
        }
    }
}
