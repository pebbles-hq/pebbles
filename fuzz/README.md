# pebbles-fuzz

Coverage-guided fuzzing of the font pipeline (11 S32), in its own workspace so
`libfuzzer-sys` never enters the main `Cargo.lock` or the cargo-deny graph.

| Target | Input | Exercises |
|---|---|---|
| `font_bytes` | arbitrary bytes as a font | registration + shaping (`pebbles_render::__fuzz::shape_font_bytes`) and the glyph atlas rasterizer (`pebbles_gpu::__fuzz::raster_font_bytes`) |
| `text_layout` | arbitrary text | line breaking and layout with the bundled fonts (`pebbles_render::__fuzz::layout_bundled`) |

## Run

```bash
cargo install cargo-fuzz
mkdir -p fuzz/corpus/font_bytes && cp crates/pebbles-render/assets/fonts/*.ttf fuzz/corpus/font_bytes/
cargo +nightly fuzz run font_bytes -- -max_total_time=600 -timeout=10
cargo +nightly fuzz run text_layout -- -max_total_time=600 -timeout=10
```

(from the `pebbles/` root). CI runs both weekly (`security.yml`, job `fuzz-fonts`).

## A crash

1. Minimize it: `cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/<crash-file>`.
2. Fix the bug (or contain an upstream panic at our boundary, as `GlyphRaster::rasterize` and
   `shape_line_contained` do).
3. Add the minimized bytes as a fixed regression case in
   `crates/pebbles-gpu/tests/font_mutation.rs` (the S33 loop), so stable CI keeps it fixed.

## Contained upstream panics

Malformed fonts can panic inside the pinned font stack. Pebbles contains these at its own
boundaries, so a bad font costs its glyphs or text and logs one warning, never the app. The
targets drop libfuzzer-sys's aborting panic hook so contained panics do not count as crashes;
anything that escapes still aborts. Known cases (both found by the S33 loop, 2026-10):

| Where | Upstream | Contained in |
|---|---|---|
| `read-fonts` 0.41 `glyf.rs:244`: indexes an empty flag array for a glyph claiming points | read-fonts (via skrifa 0.40 / swash) | `pebbles_gpu` `GlyphRaster::rasterize` (face skipped from then on) |
| `cosmic-text` 0.19 `shape.rs:234`: `expect` that a shaped glyph id fits `u16` | cosmic-text | `pebbles_render` `shape_line_contained` (paragraph shapes empty) |

Remove a row (and keep the containment) once the fixed upstream release reaches our lockfile.

Never commit `corpus/`, `artifacts/` or font binaries here.
