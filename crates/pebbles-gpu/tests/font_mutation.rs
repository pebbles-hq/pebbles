//! Seeded font-mutation loop (11 S33): malformed font bytes must never panic the shaper or the
//! glyph rasterizer. Deterministic (XorShift64*, fixed seeds), no dependencies, every CI run;
//! crashes found by the weekly fuzzer (S32/S34) are added below as fixed regression cases.

const FONTS: [&[u8]; 6] = [
    include_bytes!("../../pebbles-render/assets/fonts/InterVariable.ttf"),
    include_bytes!("../../pebbles-render/assets/fonts/InterVariable-Italic.ttf"),
    include_bytes!("../../pebbles-render/assets/fonts/JetBrainsMono[wght].ttf"),
    include_bytes!("../../pebbles-render/assets/fonts/JetBrainsMono-Italic[wght].ttf"),
    include_bytes!("../../pebbles-render/assets/fonts/SpaceGrotesk[wght].ttf"),
    include_bytes!("../../pebbles-render/assets/fonts/Lora[wght].ttf"),
];

/// XorShift64* (Vigna).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// 1..=16 edits; half the offsets land in the first 512 bytes (the table directory).
fn mutate(rng: &mut Rng, font: &[u8]) -> Vec<u8> {
    let mut m = font.to_vec();
    for _ in 0..1 + rng.below(16) {
        if m.is_empty() {
            break;
        }
        let at = if rng.next() & 1 == 0 { rng.below(m.len().min(512)) } else { rng.below(m.len()) };
        match rng.below(5) {
            0 => m[at] ^= 1 << rng.below(8),
            1 => m[at] = rng.next() as u8,
            2 => m.insert(at, rng.next() as u8),
            3 => {
                m.remove(at);
            }
            _ => m.truncate(at),
        }
    }
    m
}

fn run(n: usize) {
    let glyphs: Vec<u16> = (0..32).collect();
    for (i, font) in FONTS.iter().enumerate() {
        let mut rng = Rng(0xF0E7_0000_0000_0001 ^ i as u64);
        for _ in 0..n {
            let m = mutate(&mut rng, font);
            let _ = pebbles_render::__fuzz::shape_font_bytes(&m, "Hello مرحبا 你好", 16.0);
            let _ = pebbles_gpu::__fuzz::raster_font_bytes(&m, 0, &glyphs, 16.0);
        }
    }
}

#[test]
fn bundled_font_mutations_never_panic() {
    run(32);
}

#[test]
#[ignore = "long fuzz loop"]
fn bundled_font_mutations_never_panic_long() {
    run(2_000);
}
