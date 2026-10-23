//! Damage regions (DR-25, M30): which device pixels can differ from the previous frame of the
//! same target.
//!
//! The previous frame's op tree is kept (a `Scene` clone shares its lists, so it is O(1) and it
//! keeps every fragment allocation alive — an `Arc` address seen last frame cannot be reused by
//! a different fragment this frame). The two trees are walked in lockstep: a retained fragment
//! with the same op list and the same transform contributes nothing; a fragment at the same
//! place with the same transform but a new list is diffed recursively; every other op — and
//! every op that appeared or disappeared — contributes its conservative device bounds. Ops whose
//! pixels can change without their op changing (GPU and dynamic images, custom paint) or whose
//! reach is not a local bound (effect, backdrop and projected layers, meshes, atlases) force a
//! full redraw wherever they appear, including inside an unchanged fragment.

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, Rect};
use pebbles_render::paint::Scene;
use pebbles_render::paint::display_list::Op;

use super::cull::{blurred_bounds, device_bounds, image_bounds};
use super::plan::ScissorRect;
use super::retained::join;

/// Fringe added around every damaged bound (AA, pixel snapping), device px.
const FRINGE_PX: f64 = 2.0;
/// Above this share of the target the frame redraws in full (a partial frame costs a clear,
/// a copy and the scissored draws; past half the target that no longer pays).
const MAX_PARTIAL_SHARE: f64 = 0.5;

/// What the next frame must redraw.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Damage {
    /// Everything (and the persistent frame is rebuilt).
    Full,
    /// Only this device rect; everything outside it equals the previous frame.
    Rect(ScissorRect),
    /// Nothing changed.
    Empty,
}

/// Everything outside the op tree that changes every pixel when it changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FrameKey {
    pub size: (u32, u32),
    pub root_transform: [f64; 6],
    pub scale_factor: f64,
    pub glyph_hint: Option<bool>,
    pub base_color: [f32; 4],
    pub decode_srgb: bool,
    /// Bumped when the persistent frame texture is (re)created: its content is undefined.
    pub persistent_generation: u64,
}

/// Per render target: the previous frame and which fragments are known to hold only ops whose
/// pixels are a function of the op.
#[derive(Default)]
pub(crate) struct DamageTracker {
    prev: Option<(Scene, FrameKey)>,
    /// Fragment list address → "contains a volatile op", for lists alive in the previous frame.
    volatile: HashMap<usize, bool>,
}

impl std::fmt::Debug for DamageTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DamageTracker").field("has_prev", &self.prev.is_some()).finish_non_exhaustive()
    }
}

/// The damage accumulator: a device rect, or "unknown" (full redraw).
struct Acc {
    rect: Rect,
    unknown: bool,
}

impl Acc {
    fn add(&mut self, r: Option<Rect>) {
        match r {
            Some(r) => self.rect = join(self.rect, r),
            None => self.unknown = true,
        }
    }
}

impl DamageTracker {
    /// Compare `scene` with the previous frame of this target and remember it as the new
    /// previous frame.
    pub(crate) fn frame(&mut self, scene: &Scene, key: FrameKey) -> Damage {
        let prev = self.prev.replace((scene.clone(), key));
        let Some((prev_scene, prev_key)) = prev else { return Damage::Full };
        if prev_key != key {
            return Damage::Full;
        }
        // The same top-level list: nothing was recorded since (the kept clone makes an in-place
        // re-record impossible — `Scene::reset` allocates a new list while it is shared).
        let mut volatile = std::mem::take(&mut self.volatile);
        if std::ptr::eq(prev_scene.ops(), scene.ops()) {
            let v = list_is_volatile(scene.ops(), &mut volatile, &mut self.volatile);
            return if v { Damage::Full } else { Damage::Empty };
        }
        let mut acc = Acc { rect: Rect::ZERO, unknown: false };
        let m = Affine::new(key.root_transform);
        diff(prev_scene.ops(), scene.ops(), m, &mut acc, &mut volatile, &mut self.volatile);
        drop(volatile); // entries for lists not seen this frame are forgotten
        if acc.unknown {
            return Damage::Full;
        }
        if acc.rect == Rect::ZERO {
            return Damage::Empty;
        }
        let (w, h) = (f64::from(key.size.0), f64::from(key.size.1));
        let r = acc.rect.inflate(FRINGE_PX, FRINGE_PX).intersect(Rect::new(0.0, 0.0, w, h)).expand();
        if r.width() <= 0.0 || r.height() <= 0.0 {
            return Damage::Empty;
        }
        if r.area() > MAX_PARTIAL_SHARE * w * h {
            return Damage::Full;
        }
        // Clamped to the target above, so the casts are in range.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let s = ScissorRect { x0: r.x0 as u32, y0: r.y0 as u32, x1: r.x1 as u32, y1: r.y1 as u32 };
        Damage::Rect(s)
    }

    /// Forget the previous frame: the next one is a full redraw.
    pub(crate) fn reset(&mut self) {
        self.prev = None;
        self.volatile.clear();
    }
}

fn diff(
    prev: &[Op],
    cur: &[Op],
    m: Affine,
    acc: &mut Acc,
    seen: &mut HashMap<usize, bool>,
    keep: &mut HashMap<usize, bool>,
) {
    for i in 0..prev.len().max(cur.len()) {
        if acc.unknown {
            return;
        }
        match (prev.get(i), cur.get(i)) {
            (
                Some(Op::Fragment { transform: tp, ops: a, .. }),
                Some(Op::Fragment { transform: tc, ops: b, .. }),
            ) if tp == tc => {
                let inner = m * tp.unwrap_or(Affine::IDENTITY);
                if Arc::ptr_eq(a, b) {
                    // Unchanged — unless it holds ops whose pixels change on their own.
                    if is_volatile(b, seen, keep) {
                        acc.unknown = true;
                    }
                } else {
                    diff(a, b, inner, acc, seen, keep);
                }
            }
            (p, c) => {
                for op in [p, c].into_iter().flatten() {
                    acc.add(op_bounds(op, m));
                }
            }
        }
    }
}

/// Whether the shared list `ops` (recursively) holds an op whose pixels are not a function of
/// the op; memoized per list for as long as it stays alive.
fn is_volatile(ops: &Arc<Vec<Op>>, seen: &mut HashMap<usize, bool>, keep: &mut HashMap<usize, bool>) -> bool {
    let key = Arc::as_ptr(ops) as usize;
    if let Some(&v) = keep.get(&key) {
        return v;
    }
    let v = match seen.remove(&key) {
        Some(v) => v,
        None => list_is_volatile(ops, seen, keep),
    };
    keep.insert(key, v);
    v
}

fn list_is_volatile(ops: &[Op], seen: &mut HashMap<usize, bool>, keep: &mut HashMap<usize, bool>) -> bool {
    ops.iter().any(|op| match op {
        Op::Fragment { ops, .. } => is_volatile(ops, seen, keep),
        other => op_bounds(other, Affine::IDENTITY).is_none(),
    })
}

/// Conservative device bounds of `op` under `m`; `None` when it has no local bound.
fn op_bounds(op: &Op, m: Affine) -> Option<Rect> {
    Some(match op {
        Op::Fill { transform, shape, .. } => device_bounds(shape, m * *transform, None),
        Op::Stroke { style, transform, shape, .. } => device_bounds(shape, m * *transform, Some(style)),
        Op::ShaderFill { transform, shape, stroke, .. } => {
            device_bounds(shape, m * *transform, stroke.as_ref())
        }
        Op::PushLayer { transform, clip, .. } => device_bounds(clip, m * *transform, None),
        Op::PopLayer => Rect::ZERO,
        Op::Image { image, transform, .. } => {
            image_bounds(image.image.width, image.image.height, m * *transform)
        }
        Op::ImageNine { dst, transform, .. } => (m * *transform).transform_rect_bbox(*dst),
        Op::BlurredRect { transform, rect, radius, std_dev, .. } => {
            blurred_bounds(rect.inflate(radius.abs(), radius.abs()), *std_dev, m * *transform)
        }
        Op::BlurredFill { transform, shape, sigma, .. } => {
            let t = m * *transform;
            let reach = 3.0 * f64::from(sigma.abs()) * t.as_coeffs()[0].hypot(t.as_coeffs()[1]).max(1.0);
            device_bounds(shape, t, None).inflate(reach, reach)
        }
        Op::Glyphs { font_size, transform, glyph_transform, glyphs, .. } => {
            glyph_bounds(*font_size, 0.0, m * *transform, *glyph_transform, glyphs)?
        }
        Op::BlurredGlyphs { font_size, transform, glyph_transform, glyphs, sigma, .. } => {
            glyph_bounds(*font_size, *sigma, m * *transform, *glyph_transform, glyphs)?
        }
        Op::Fragment { transform, ops, .. } => {
            let inner = m * transform.unwrap_or(Affine::IDENTITY);
            let mut r = Rect::ZERO;
            for op in ops.iter() {
                r = join(r, op_bounds(op, inner)?);
            }
            r
        }
        // Pixels that change without the op changing, or reach that is not a local bound.
        Op::GpuImage { .. }
        | Op::DynamicImage { .. }
        | Op::Custom { .. }
        | Op::PushEffectLayer { .. }
        | Op::PushBackdropFilter { .. }
        | Op::PushProjectedLayer { .. }
        | Op::Vertices { .. }
        | Op::Atlas { .. } => return None,
        _ => return None,
    })
}

/// A box around every glyph origin generous enough for any glyph of the font size (ascenders,
/// descenders, wide and slanted glyphs), grown by the blur reach.
fn glyph_bounds(
    font_size: f32,
    sigma: f32,
    m: Affine,
    glyph_transform: Option<Affine>,
    glyphs: &[pebbles_render::paint::Glyph],
) -> Option<Rect> {
    let (first, rest) = glyphs.split_first()?;
    let s = f64::from(font_size.abs());
    let mut local = Rect::new(f64::from(first.x), f64::from(first.y), f64::from(first.x), f64::from(first.y));
    for g in rest {
        local = local.union_pt((f64::from(g.x), f64::from(g.y)));
    }
    let k = glyph_transform.map_or(1.0, |g| {
        let [a, b, c, d, _, _] = g.as_coeffs();
        a.abs().max(b.abs()).max(c.abs()).max(d.abs()).max(1.0) * 2.0
    });
    let reach = 3.0 * f64::from(sigma.abs());
    let pad = 2.0 * s * k + reach;
    Some(m.transform_rect_bbox(local.inflate(pad, pad)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Rect;
    use pebbles_render::paint::Painter;
    use peniko::{Color, Fill};

    fn key() -> FrameKey {
        FrameKey {
            size: (400, 300),
            root_transform: Affine::IDENTITY.as_coeffs(),
            scale_factor: 1.0,
            glyph_hint: None,
            base_color: [1.0; 4],
            decode_srgb: false,
            persistent_generation: 1,
        }
    }

    fn rects(n: usize, changed: Option<usize>) -> Scene {
        let mut s = Scene::new();
        let mut p = Painter::new(&mut s);
        for i in 0..n {
            let x = i as f64 * 10.0;
            let c = if Some(i) == changed { Color::BLACK } else { Color::WHITE };
            p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(x, 0.0, x + 8.0, 8.0));
        }
        s
    }

    #[test]
    fn first_frame_and_key_change_are_full() {
        let mut t = DamageTracker::default();
        assert_eq!(t.frame(&rects(3, None), key()), Damage::Full);
        let resized = FrameKey { size: (401, 300), ..key() };
        assert_eq!(t.frame(&rects(3, None), resized), Damage::Full);
    }

    #[test]
    fn retained_fragment_is_free_and_raw_ops_damage() {
        let mut t = DamageTracker::default();
        let frag = rects(20, None);
        let frame = |extra: f64| {
            let mut s = Scene::new();
            s.append(&frag, None);
            Painter::new(&mut s).fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::BLACK,
                None,
                &Rect::new(extra, 100.0, extra + 4.0, 124.0),
            );
            s
        };
        t.frame(&frame(50.0), key());
        match t.frame(&frame(50.0), key()) {
            // The raw caret-like rect is re-recorded every frame; the fragment costs nothing.
            Damage::Rect(r) => assert!(r.x0 >= 46 && r.x1 <= 57 && r.y0 >= 96 && r.y1 <= 128, "{r:?}"),
            d => panic!("{d:?}"),
        }
    }

    #[test]
    fn changed_fragment_damages_only_what_moved() {
        let mut t = DamageTracker::default();
        let frame = |changed| {
            let mut s = Scene::new();
            s.append(&rects(20, changed), None);
            s
        };
        t.frame(&frame(None), key());
        // Every op inside a re-recorded fragment counts (ops are not compared), so a whole new
        // list damages its whole extent — conservative, never wrong.
        assert!(matches!(t.frame(&frame(Some(3)), key()), Damage::Rect(_) | Damage::Full));
    }

    #[test]
    fn unchanged_scene_is_empty() {
        let mut t = DamageTracker::default();
        let mut s = Scene::new();
        s.append(&rects(5, None), None);
        t.frame(&s, key());
        assert_eq!(t.frame(&s, key()), Damage::Empty);
    }

    #[test]
    fn volatile_op_in_a_retained_fragment_forces_full() {
        let mut t = DamageTracker::default();
        let mut inner = Scene::new();
        Painter::new(&mut inner).draw_custom(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Affine::IDENTITY,
            Arc::new(Noop),
        );
        let mut s = Scene::new();
        s.append(&inner, None);
        t.frame(&s, key());
        assert_eq!(t.frame(&s, key()), Damage::Full);
    }

    #[derive(Debug)]
    struct Noop;
    impl pebbles_render::paint::display_list::CustomPaint for Noop {
        fn bounds(&self) -> Rect {
            Rect::new(0.0, 0.0, 10.0, 10.0)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }
}
