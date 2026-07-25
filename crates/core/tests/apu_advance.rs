//! `Apu::advance` must be bit-identical to the per-tick `tick()` chain.
//! Randomized register writes (triggers, sweeps, NR52 power cycling, wave
//! RAM) between random-size windows, plus occasional DIV-counter jumps.
//! Compared after every window: PCM12/34, every readable register, and the
//! emitted sample buffer bit-for-bit.

use core::apu::Apu;

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}

#[test]
fn test_apu_advance_equals_ticks_randomized() {
    for seed in 0..8u64 {
        let mut rng = Lcg(seed.wrapping_mul(0x9E3779B97F4A7C15) + 1);
        let mut a = Apu::default(); // reference: per-tick
        let mut b = Apu::default(); // candidate: advance
        let mut v: u16 = 0xABCD; // stand-in DIV counter

        for round in 0..30_000usize {
            if rng.below(4) == 0 {
                for _ in 0..=rng.below(3) {
                    // NR10..NR52 plus wave RAM; the FF27-FF2F hole is not
                    // routed to the APU by the bus
                    let r = rng.below(0x17 + 0x10);
                    let addr = if r < 0x17 { 0xFF10 + r } else { 0xFF30 + (r - 0x17) } as u16;
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
            for addr in 0xFF10..=0xFF26u16 {
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
