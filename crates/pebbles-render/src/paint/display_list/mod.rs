//! The shared, backend-neutral display list (DR-02): [`Scene`] (a retained `Vec<Op>`),
//! [`Painter`] (the recording API) and the [`op`] vocabulary. Every renderer backend
//! consumes the same list — the hybrid flush (until Phase 7), the wgpu backend (until C05
//! moves it to pebbles-gpu) and the render-check tool.

use std::sync::Arc;

use kurbo::Affine;

pub mod op;
pub mod painter;

pub use op::{Geometry, Glyph, NormalizedCoord, Op, PathHint, PixelSnap};
pub use painter::Painter;

/// Path-flattening tolerance for recorded non-primitive shapes (logical px).
pub(crate) const TOL: f64 = 0.1;

/// A retained, backend-neutral list of [`Op`]s. Cloning and appending are O(1): the op
/// vector is shared copy-on-write. `Send + Sync`: a finished `Scene` travels to the render
/// thread inside a `FramePacket` (DR-26, 08 §17). Every payload type reachable from `Op`
/// must therefore be `Send + Sync` (`Arc`, never `Rc` or `RefCell`).
#[derive(Clone, Debug, Default)]
pub struct Scene {
    ops: Arc<Vec<Op>>,
}

// Compile-time guarantee (C03): a Scene can cross to the render thread.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Scene>();
};

impl Scene {
    /// Create an empty [`Scene`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear for re-recording WITHOUT cloning a list another scene still shares.
    pub fn reset(&mut self) {
        match Arc::get_mut(&mut self.ops) {
            Some(v) => v.clear(),
            None => self.ops = Arc::new(Vec::new()),
        }
    }

    /// Number of recorded ops (a fragment's re-append cost is one op).
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// Whether anything has been recorded.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Read-only op access for consumers (renderer contract).
    pub fn ops(&self) -> &[Op] {
        &self.ops
    }

    /// Append `fragment` as ONE `Op::Fragment` sharing its storage (`Arc::clone`) — never a
    /// deep copy.
    pub fn append(&mut self, fragment: &Scene, transform: Option<Affine>) {
        self.push(Op::Fragment { transform, ops: Arc::clone(&fragment.ops) });
    }

    /// Record one op: `Arc::make_mut(&mut self.ops).push(op)`.
    pub(crate) fn push(&mut self, op: Op) {
        Arc::make_mut(&mut self.ops).push(op);
    }

    /// Test-only accessor for `Arc::strong_count` / `Arc::as_ptr` assertions.
    #[cfg(test)]
    pub(crate) fn ops_arc(&self) -> &Arc<Vec<Op>> {
        &self.ops
    }
}

/// Create an empty [`Scene`].
pub fn scene() -> Scene {
    Scene::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{BezPath, Circle, Rect, RoundedRect, Shape};
    use peniko::{Color, Fill};

    #[test]
    fn appended_fragment_remains_a_single_retained_operation() {
        let mut fragment = Scene::new();
        Painter::new(&mut fragment).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 0, 255),
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );

        let mut scene = Scene::new();
        scene.append(&fragment, Some(Affine::translate((4.0, 8.0))));

        assert_eq!(fragment.len(), 1);
        assert_eq!(scene.len(), 1);
    }

    #[test]
    fn append_is_o1_and_shares_storage() {
        let mut fragment = Scene::new();
        {
            let mut p = Painter::new(&mut fragment);
            for i in 0..10_000 {
                p.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    Color::from_rgba8(0, 0, 0, 255),
                    None,
                    &Rect::new(i as f64, 0.0, i as f64 + 1.0, 1.0),
                );
            }
        }
        assert_eq!(fragment.len(), 10_000);

        let before = Arc::strong_count(fragment.ops_arc());
        let mut scene = Scene::new();
        for _ in 0..1_000 {
            scene.append(&fragment, None);
        }
        // Each append is a single `Arc::clone` — no deep copy of the 10 000 ops.
        assert_eq!(Arc::strong_count(fragment.ops_arc()), before + 1_000);
        assert_eq!(fragment.len(), 10_000);
        assert_eq!(scene.len(), 1_000);
    }

    #[test]
    fn reset_does_not_clone_shared_list() {
        let mut fragment = Scene::new();
        Painter::new(&mut fragment).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::from_rgba8(0, 0, 0, 255),
            None,
            &Rect::new(0.0, 0.0, 10.0, 10.0),
        );
        let ptr_before = Arc::as_ptr(fragment.ops_arc());

        let mut scene = Scene::new();
        scene.append(&fragment, None);
        scene.reset();

        // Resetting a scene that shares its list must NOT clear the fragment's storage.
        assert_eq!(fragment.len(), 1);
        assert_eq!(scene.len(), 0);
        assert_eq!(Arc::as_ptr(fragment.ops_arc()), ptr_before);
    }

    #[test]
    fn geometry_from_shape_preserves_primitives() {
        assert!(matches!(Geometry::from_shape(&Rect::new(0.0, 0.0, 1.0, 1.0)), Geometry::Rect(_)));
        assert!(matches!(
            Geometry::from_shape(&RoundedRect::from_rect(Rect::new(0.0, 0.0, 1.0, 1.0), 0.2)),
            Geometry::RoundedRect(_)
        ));
        assert!(matches!(Geometry::from_shape(&Circle::new((1.0, 1.0), 2.0)), Geometry::Circle(_)));

        let mut tri = BezPath::new();
        tri.move_to((0.0, 0.0));
        tri.line_to((10.0, 0.0));
        tri.line_to((5.0, 10.0));
        tri.close_path();
        assert!(matches!(Geometry::from_shape(&tri), Geometry::Path(_)));
    }

    #[test]
    fn geometry_to_path_matches_record_time_path() {
        let rect = Rect::new(1.0, 2.0, 5.0, 7.0);
        let rounded = RoundedRect::from_rect(Rect::new(1.0, 2.0, 5.0, 7.0), 0.5);
        let circle = Circle::new((3.0, 4.0), 2.5);
        let mut path = BezPath::new();
        path.move_to((0.0, 0.0));
        path.line_to((10.0, 0.0));
        path.line_to((5.0, 10.0));
        path.close_path();

        fn elements_of(g: &Geometry) -> Vec<kurbo::PathEl> {
            g.to_path(TOL).elements().to_vec()
        }

        assert_eq!(elements_of(&Geometry::from_shape(&rect)), rect.to_path(TOL).elements().to_vec());
        assert_eq!(elements_of(&Geometry::from_shape(&rounded)), rounded.to_path(TOL).elements().to_vec());
        assert_eq!(elements_of(&Geometry::from_shape(&circle)), circle.to_path(TOL).elements().to_vec());
        assert_eq!(elements_of(&Geometry::from_shape(&path)), path.to_path(TOL).elements().to_vec());
    }

    /// C04: `Op::Glyphs` carries no weight; variable weight travels only in normalized coords.
    #[test]
    fn glyph_op_has_no_weight_field() {
        let font = peniko::FontData::new(peniko::Blob::from(vec![0u8; 4]), 0);
        let mut scene = Scene::new();
        Painter::new(&mut scene).draw_glyphs(
            &font,
            16.0,
            &[1 << 13],
            &peniko::Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            std::iter::once(Glyph { id: 1, x: 0.0, y: 0.0 }),
        );
        let Some(Op::Glyphs { font_size, normalized_coords, .. }) = scene.ops().first() else {
            panic!("expected a glyph op");
        };
        assert_eq!((*font_size, normalized_coords.as_slice()), (16.0, [1 << 13].as_slice()));
    }
}
