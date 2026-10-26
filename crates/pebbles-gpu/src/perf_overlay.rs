//! The on-screen performance overlay (11 S57; Flutter's `PerformanceOverlay`, DR-24).
//!
//! Enabled per frame by [`FrameParams::perf_overlay`](crate::FrameParams) (the shell sets it from
//! `App::perf_overlay(true)` or `PEBBLES_PERF_OVERLAY=1`). The overlay is ordinary display-list
//! ops in its own [`Scene`], drawn by the renderer after the frame's content into a 240×96
//! logical-px panel at the top-right: the last 120 frame times as bars (renderer CPU time, and
//! GPU time when timestamp queries are on, M22), a 16.7 ms line, and one line of counters.
//!
//! It lives in the window's [`FrameTargets`](crate::FrameTargets), so it is per window and never
//! global (DR-12). The offscreen API never sets the flag, so captures and goldens never contain
//! it; damage tracking (M30) sees its ops change every frame and redraws its rect. When the flag
//! is off nothing here runs: no allocation, no ops.

use std::collections::VecDeque;

use kurbo::{Affine, Rect};
use pebbles_render::paint::{Glyph, Painter, Scene};
use peniko::{Brush, Color, Fill, FontData};
use skrifa::MetadataProvider;

use crate::FrameStats;

/// Panel size, logical px.
pub(crate) const WIDTH: f64 = 240.0;
/// Panel height, logical px.
pub(crate) const HEIGHT: f64 = 96.0;
/// Frames kept (one bar each).
pub(crate) const HISTORY: usize = 120;
/// Gap to the window's top and right edges, logical px.
const MARGIN: f64 = 8.0;
/// The bar area's full height in ms (bars clamp here); the 16.7 ms line sits at 2/3.
const SCALE_MS: f64 = 25.0;
/// The 60 Hz frame budget.
const BUDGET_MS: f64 = 1000.0 / 60.0;
/// Counter text size, logical px.
const TEXT_PX: f32 = 10.0;
/// Bar area: from the panel top to above the counter line.
const BARS_TOP: f64 = 6.0;
const BARS_BOTTOM: f64 = HEIGHT - 22.0;

/// One frame's sample.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    cpu_ms: f64,
    gpu_ms: Option<f64>,
}

/// The bundled monospace face the counter line is set in (no shaping: ASCII on a grid).
struct Mono {
    bytes: &'static [u8],
    font: FontData,
}

impl Mono {
    fn load() -> Option<Self> {
        let (_, bytes) =
            pebbles_render::fonts::builtin_fonts().iter().find(|(name, _)| *name == "JetBrains Mono")?;
        Some(Mono { bytes, font: FontData::new(peniko::Blob::from(bytes.to_vec()), 0) })
    }
}

/// A window's overlay state: the frame history, the overlay scene and the content + overlay
/// scene the renderer draws.
#[derive(Default)]
pub(crate) struct PerfOverlay {
    samples: VecDeque<Sample>,
    last: Option<FrameStats>,
    /// Pixels in the last frame's target (for the damage percentage).
    last_px: u64,
    mono: Option<Mono>,
    overlay: Scene,
    combined: Scene,
}

impl std::fmt::Debug for PerfOverlay {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PerfOverlay").field("samples", &self.samples.len()).finish_non_exhaustive()
    }
}

impl PerfOverlay {
    /// Record a rendered frame's stats (`px` = target width × height).
    pub(crate) fn record(&mut self, stats: &FrameStats, px: u64) {
        let us = |v: u32| f64::from(v) / 1000.0;
        let cpu_ms = us(stats.prepare_us) + us(stats.upload_us) + us(stats.encode_us) + us(stats.submit_us);
        if self.samples.len() == HISTORY {
            self.samples.pop_front();
        }
        self.samples.push_back(Sample { cpu_ms, gpu_ms: stats.gpu_us.map(us) });
        self.last = Some(*stats);
        self.last_px = px;
    }

    /// The counter line for the last recorded frame.
    pub(crate) fn counters(&self) -> String {
        let Some(s) = self.last.as_ref() else { return "no frame yet".into() };
        let damage = if self.last_px == 0 || s.full_redraws > 0 || s.damage_px == 0 {
            100.0
        } else {
            100.0 * s.damage_px as f64 / self.last_px as f64
        };
        let retained = s.ops.saturating_sub(s.ops_prepared);
        format!(
            "dc {} mk {} abc {}/{}/{} dmg {:.0}% ret {}",
            s.draw_calls,
            s.masks_rasterized,
            s.tier_a_draws,
            s.tier_b_draws,
            s.tier_c_draws,
            damage,
            retained
        )
    }

    /// Rebuild the overlay scene for a target `logical_width` px wide (logical coordinates).
    pub(crate) fn build(&mut self, logical_width: f64) -> &Scene {
        self.overlay.reset();
        let x0 = (logical_width - WIDTH - MARGIN).max(0.0);
        let y0 = MARGIN;
        let counters = self.counters();
        if self.mono.is_none() {
            self.mono = Mono::load();
        }
        let mut p = Painter::new(&mut self.overlay);
        let panel = Rect::new(x0, y0, x0 + WIDTH, y0 + HEIGHT);
        p.fill(Fill::NonZero, Affine::IDENTITY, &Brush::Solid(Color::from_rgba8(0, 0, 0, 200)), None, &panel);
        let bar_w = WIDTH / HISTORY as f64;
        let area = BARS_BOTTOM - BARS_TOP;
        let height = |ms: f64| (ms.clamp(0.0, SCALE_MS) / SCALE_MS) * area;
        let cpu = Brush::Solid(Color::from_rgba8(56, 189, 248, 255));
        let gpu = Brush::Solid(Color::from_rgba8(250, 204, 21, 220));
        let over = Brush::Solid(Color::from_rgba8(248, 113, 113, 255));
        // Newest frame at the right edge.
        let first = HISTORY - self.samples.len();
        for (i, s) in self.samples.iter().enumerate() {
            let x = x0 + (first + i) as f64 * bar_w;
            let bottom = y0 + BARS_BOTTOM;
            let h = height(s.cpu_ms);
            let brush = if s.cpu_ms > BUDGET_MS { &over } else { &cpu };
            p.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                brush,
                None,
                &Rect::new(x, bottom - h, x + bar_w * 0.5, bottom),
            );
            if let Some(g) = s.gpu_ms {
                let h = height(g);
                p.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    &gpu,
                    None,
                    &Rect::new(x + bar_w * 0.5, bottom - h, x + bar_w, bottom),
                );
            }
        }
        // The 16.7 ms line: a 1 px rect on the pixel grid (a stroke's AA would spill past the panel).
        let budget_y = (y0 + BARS_BOTTOM - height(BUDGET_MS)).round();
        p.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(Color::from_rgba8(255, 255, 255, 160)),
            None,
            &Rect::new(x0, budget_y, x0 + WIDTH, budget_y + 1.0),
        );
        if let Some(mono) = self.mono.as_ref()
            && let Ok(face) = skrifa::FontRef::new(mono.bytes)
        {
            let charmap = face.charmap();
            let metrics = face.glyph_metrics(
                skrifa::instance::Size::new(TEXT_PX),
                skrifa::instance::LocationRef::default(),
            );
            let mut x = 0.0f32;
            let mut glyphs = Vec::with_capacity(counters.len());
            for c in counters.chars() {
                let Some(id) = charmap.map(c) else { continue };
                glyphs.push(Glyph { id: id.to_u32(), x, y: 0.0 });
                x += metrics.advance_width(id).unwrap_or(TEXT_PX * 0.6);
            }
            p.draw_glyphs(
                &mono.font,
                TEXT_PX,
                &[],
                &Brush::Solid(Color::from_rgba8(240, 240, 240, 255)),
                Affine::translate((x0 + 6.0, y0 + HEIGHT - 7.0)),
                None,
                Fill::NonZero,
                glyphs.into_iter(),
            );
        }
        &self.overlay
    }

    /// `content` with the overlay drawn on top, for a `size` physical-px target at `scale`
    /// (the overlay is built in logical px and scaled like the content's root transform).
    pub(crate) fn compose(&mut self, content: &Scene, size: (u32, u32), scale: f64) -> &Scene {
        let scale = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        // Drop last frame's references first, so the overlay rebuild reuses its storage.
        self.combined.reset();
        self.build(f64::from(size.0) / scale);
        self.combined.append(content, None);
        self.combined.append(&self.overlay, Some(Affine::scale(scale)));
        &self.combined
    }

    /// Bars in the overlay scene (tests).
    #[cfg(test)]
    pub(crate) fn sample_count(&self) -> usize {
        self.samples.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(prepare_us: u32, gpu_us: Option<u32>) -> FrameStats {
        FrameStats {
            prepare_us,
            gpu_us,
            draw_calls: 7,
            tier_a_draws: 5,
            ops: 10,
            ops_prepared: 4,
            ..Default::default()
        }
    }

    #[test]
    fn overlay_off_records_nothing() {
        // Off is the default: a fresh overlay has no history, no font and no ops.
        let o = PerfOverlay::default();
        assert_eq!(o.sample_count(), 0);
        assert!(o.overlay.is_empty() && o.combined.is_empty());
        assert!(o.mono.is_none(), "nothing loaded until a frame asks for the overlay");
    }

    #[test]
    fn overlay_scene_has_expected_bars() {
        let mut o = PerfOverlay::default();
        for i in 0..150u32 {
            // Every third frame also reports GPU time.
            o.record(&stats(1000 + i, (i % 3 == 0).then_some(500)), 100 * 100);
        }
        assert_eq!(o.sample_count(), HISTORY, "history is capped at 120 frames");
        let gpu_bars = o.samples.iter().filter(|s| s.gpu_ms.is_some()).count();
        let len = o.build(800.0).len();
        // Panel + one CPU bar per frame + the GPU bars + the 16.7 ms line + the counter run.
        let text = usize::from(o.mono.is_some());
        assert_eq!(len, 1 + HISTORY + gpu_bars + 1 + text);
        assert_eq!(o.counters(), "dc 7 mk 0 abc 5/0/0 dmg 100% ret 6");
    }

    #[test]
    fn compose_puts_the_overlay_after_the_content() {
        let mut content = Scene::new();
        Painter::new(&mut content).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            &Brush::Solid(Color::from_rgba8(1, 2, 3, 255)),
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let mut o = PerfOverlay::default();
        o.record(&stats(2000, None), 0);
        let overlay_len = o.build(400.0).len();
        assert!(overlay_len > 0);
        // The content, then the overlay as one scaled fragment on top.
        let combined = o.compose(&content, (800, 600), 2.0);
        assert_eq!(combined.len(), 2);
        assert!(matches!(
            combined.ops()[1],
            pebbles_render::paint::display_list::Op::Fragment { transform: Some(t), .. } if t == Affine::scale(2.0)
        ));
    }
}
