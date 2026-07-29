/// Deterministic PRNG (LCG, Knuth's MMIX constants) — reproducible seeds
/// without a `rand` dependency; high bits only, low LCG bits are weak.
pub struct Lcg(u64);

impl Lcg {
    /// Seed from a small counter. The multiplier is `floor(2^64 / phi)` (the
    /// golden ratio, as in splitmix64): it scatters consecutive seeds to
    /// far-apart start states so `0, 1, 2, ...` yield uncorrelated runs. `+1`
    /// keeps `seed = 0` off the zero state.
    pub fn seeded(seed: u64) -> Self {
        Lcg(seed.wrapping_mul(0x9E3779B97F4A7C15) + 1)
    }

    /// Advance state and return the top 31 bits (the low LCG bits are weak).
    pub fn next(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }

    /// Uniform-ish value in `[0, n)`; slight modulo bias, fine for tests.
    pub fn below(&mut self, n: u32) -> u32 {
        self.next() % n
    }
}
