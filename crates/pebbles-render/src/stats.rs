//! Debug-only per-frame pipeline counters: how many render objects were laid out,
//! painted, or culled this frame, and how many glyph runs reached the scene.
//!
//! These are the measuring stick for the viewport-bounded rendering work (and the
//! `PEBBLES_FRAME_STATS=1` print): a frame's cost must track what is *visible*, not
//! the size of the document. All counters compile to no-ops in release builds —
//! same pattern as the E6c lifecycle census and the paragraph `shape_count`.
//!
//! Reset happens at the start of each frame (`reset_frame`), read at the end.

#[cfg(debug_assertions)]
thread_local! {
    static LAYOUT_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static LAYOUT_SKIPS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static PAINTED_NODES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static CULLED_NODES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    static GLYPH_RUNS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Zero all frame counters (call at frame start, before layout).
pub fn reset_frame() {
    #[cfg(debug_assertions)]
    {
        LAYOUT_CALLS.with(|c| c.set(0));
        LAYOUT_SKIPS.with(|c| c.set(0));
        PAINTED_NODES.with(|c| c.set(0));
        CULLED_NODES.with(|c| c.set(0));
        GLYPH_RUNS.with(|c| c.set(0));
    }
}

macro_rules! counter {
    ($name:ident, $bump:ident, $get:ident) => {
        #[inline]
        pub(crate) fn $bump() {
            #[cfg(debug_assertions)]
            $name.with(|c| c.set(c.get() + 1));
        }
        /// Frame counter (debug builds; always 0 in release).
        pub fn $get() -> u64 {
            #[cfg(debug_assertions)]
            {
                $name.with(std::cell::Cell::get)
            }
            #[cfg(not(debug_assertions))]
            {
                0
            }
        }
    };
}

counter!(LAYOUT_CALLS, bump_layout, layout_calls);
counter!(LAYOUT_SKIPS, bump_layout_skip, layout_skips);
counter!(PAINTED_NODES, bump_painted, painted_nodes);
counter!(CULLED_NODES, bump_culled, culled_nodes);
counter!(GLYPH_RUNS, bump_glyph_run, glyph_runs);
