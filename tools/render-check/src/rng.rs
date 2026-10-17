//! Seeded XorShift64 for corpus geometry — the same generator as pebbles-gpu's
//! `test_support` (V05, 06 §10.4), so a seed means the same sequence everywhere.

/// Marsaglia xorshift (13, 7, 17). Seed 0 is mapped to 1 (0 is a fixed point).
#[derive(Clone, Debug)]
pub struct XorShift64(u64);

impl XorShift64 {
    /// A generator seeded with `seed` (0 → 1).
    pub fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    /// The next raw value.
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in `[lo, hi)` (modulo bias is irrelevant for corpus geometry).
    pub fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next_u64() % u64::from(hi - lo)) as u32
    }

    /// Uniform in `[lo, hi)`.
    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xorshift_sequence_is_stable() {
        let mut rng = XorShift64::new(1);
        assert_eq!(rng.next_u64(), 1082269761);
        assert_eq!(rng.next_u64(), 1152992998833853505);
        assert_eq!(rng.next_u64(), 11177516664432764457);
        assert_eq!(rng.next_u64(), 17678023832001937445);
    }

    #[test]
    fn ranges_stay_in_bounds() {
        let mut rng = XorShift64::new(7);
        for _ in 0..10_000 {
            assert!((10..20).contains(&rng.range_u32(10, 20)));
            let f = rng.range_f64(-1.0, 1.0);
            assert!((-1.0..1.0).contains(&f));
        }
    }
}
