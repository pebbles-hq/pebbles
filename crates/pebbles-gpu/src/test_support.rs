//! Test-only helpers: a seeded RNG, a property-loop runner, and a bundled-font source
//! (DR-15 — no `proptest`).

/// The bundled Inter variable font as a `FontData` (for glyph-run CPU tests).
pub(crate) fn bundled_font() -> peniko::FontData {
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0)
}

/// xorshift64 (13, 7, 17). Deterministic, dependency-free.
pub(crate) struct XorShift64(u64);

impl XorShift64 {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform in `[lo, hi)`.
    #[allow(dead_code)] // first caller: C07's property loop
    pub(crate) fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64)
    }

    /// Uniform in `lo..hi` (hi exclusive, hi > lo).
    #[allow(clippy::cast_possible_truncation)] // remainder < hi - lo <= u32::MAX
    pub(crate) fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next_u64() % u64::from(hi - lo)) as u32
    }
}

/// Run `body` for seeds `1..=n`; a panic message is prefixed with the failing seed.
pub(crate) fn for_seeds(n: u64, mut body: impl FnMut(u64, &mut XorShift64)) {
    for seed in 1..=n {
        let mut rng = XorShift64::new(seed);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| body(seed, &mut rng)));
        if let Err(e) = r {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            panic!("property failed for seed {seed}: {msg}");
        }
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
    fn range_u32_stays_in_bounds() {
        let mut rng = XorShift64::new(1);
        for _ in 0..10_000 {
            let v = rng.range_u32(10, 20);
            assert!((10..20).contains(&v), "out of bounds: {v}");
        }
    }

    #[test]
    #[should_panic(expected = "seed 3")]
    fn for_seeds_reports_the_seed() {
        for_seeds(5, |seed, _| {
            if seed == 3 {
                panic!("boom");
            }
        });
    }
}
