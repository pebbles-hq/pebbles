//! [`RenderTransform`] — applies an affine transform (rotate/scale/translate/skew)
//! to its child's painting and hit-testing, around a configurable origin. Layout is
//! a pass-through: the child keeps its size and the box occupies its untransformed
//! bounds for layout; only paint + hit-testing are transformed (Flutter's
//! `Transform` / `Container.transform`).

use kurbo::Affine;
use pebbles_foundation::{Alignment, Offset, Size};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Transforms its single child.
pub struct RenderTransform {
    /// The raw transform (e.g. `Affine::rotate` / `Affine::scale`).
    pub matrix: Affine,
    /// Where the transform's origin sits within the box (`-1..1` per axis).
    pub alignment: Alignment,
    /// A perspective transform (Flutter `Transform(transform: Matrix4)`, 09 §10.7); replaces
    /// `matrix` when set. An affine `Matrix4` keeps the affine path.
    pub projective: Option<crate::paint::Projective>,
}

impl RenderTransform {
    pub fn new(matrix: Affine, alignment: Alignment) -> Self {
        RenderTransform { matrix, alignment, projective: None }
    }

    /// From a Flutter-style column-major `Matrix4` (an affine one uses the affine path).
    pub fn matrix4(m: [f64; 16], alignment: Alignment) -> Self {
        let h = crate::paint::Projective::from_matrix4(m);
        if h.is_affine() {
            Self::new(h.to_affine(), alignment)
        } else {
            RenderTransform { matrix: Affine::IDENTITY, alignment, projective: Some(h) }
        }
    }

    fn origin(&self, size: Size) -> (f64, f64) {
        ((self.alignment.x + 1.0) / 2.0 * size.width, (self.alignment.y + 1.0) / 2.0 * size.height)
    }
}

impl RenderObject for RenderTransform {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        match cx.children().first().copied() {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(Size::ZERO),
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        // The transform itself is applied by the parent's `paint_child` (which reads
        // `transform()`); here we just paint the child normally into that space.
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn transform(&self, size: Size) -> Option<Affine> {
        if self.projective.is_some() {
            return None;
        }
        // Resolve the alignment to a pixel origin, then transform around it.
        let (ox, oy) = self.origin(size);
        Some(Affine::translate((ox, oy)) * self.matrix * Affine::translate((-ox, -oy)))
    }

    fn projection(&self, size: Size) -> Option<crate::paint::Projective> {
        use crate::paint::Projective;
        let h = self.projective?;
        let (ox, oy) = self.origin(size);
        let to = Projective::from_affine(Affine::translate((-ox, -oy)));
        let back = Projective::from_affine(Affine::translate((ox, oy)));
        Some(to.then(&h).then(&back))
    }

    fn debug_name(&self) -> &'static str {
        "RenderTransform"
    }
}
