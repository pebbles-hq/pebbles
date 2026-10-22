#![no_main]
use libfuzzer_sys::fuzz_target;

// Pebbles contains panics from malformed fonts at its own boundaries (GlyphRaster::rasterize,
// shape_line_contained) — that containment is part of what is under test. libfuzzer-sys's hook
// would abort on those contained panics before they unwind, so drop it; a panic that ESCAPES
// still reaches libfuzzer-sys's catch_unwind around the target, which aborts and records it.
fuzz_target!(init: { let _ = std::panic::take_hook(); }, |data: &[u8]| {
    let w = 50.0 + f32::from(data.first().copied().unwrap_or(0)) * 4.0;
    let text = String::from_utf8_lossy(data);
    let _ = pebbles_render::__fuzz::layout_bundled(&text, w);
});
