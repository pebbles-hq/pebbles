#![no_main]
use libfuzzer_sys::fuzz_target;

const SAMPLES: [&str; 7] = [
    "Hello, world", "مرحبا بالعالم", "שלום עולם", "你好，世界",
    "👩‍👩‍👧‍👦 🇺🇳 é", "a\u{301}\u{302}\u{303}", "ffi fl Ŧ ǅ",
];

// Pebbles contains panics from malformed fonts at its own boundaries (GlyphRaster::rasterize,
// shape_line_contained) — that containment is part of what is under test. libfuzzer-sys's hook
// would abort on those contained panics before they unwind, so drop it; a panic that ESCAPES
// still reaches libfuzzer-sys's catch_unwind around the target, which aborts and records it.
fuzz_target!(init: { let _ = std::panic::take_hook(); }, |data: &[u8]| {
    if data.len() < 4 { return; }
    let px = 4.0 + f32::from(data[0]);
    let text = SAMPLES[usize::from(data[1]) % SAMPLES.len()];
    let font = &data[2..];
    let _ = pebbles_render::__fuzz::shape_font_bytes(font, text, px);
    let glyphs: Vec<u16> = (0u16..64).collect();
    let _ = pebbles_gpu::__fuzz::raster_font_bytes(font, 0, &glyphs, px);
});
