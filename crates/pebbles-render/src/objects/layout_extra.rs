//! Extra layout render objects backing the long-tail Flutter layout widgets:
//! [`RenderOffstage`], [`RenderBaseline`], [`RenderRotatedBox`],
//! [`RenderSizedOverflowBox`], and [`RenderFractionalTranslation`].
//!
//! Each takes a single child (the first entry in its child list) and pass-through
//! paints it; they differ only in how they size, position, or transform it.

use std::f64::consts::FRAC_PI_2;

use pebbles_foundation::{Alignment, Offset, Size};
use vello::kurbo::Affine;

use crate::RenderId;
use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

fn first_child(cx: &LayoutCx<'_>) -> Option<RenderId> {
    cx.children().first().copied()
}
fn first_child_paint(cx: &PaintCx<'_>) -> Option<RenderId> {
    cx.children().first().copied()
}

// ===========================================================================
// Offstage — lay nothing out, take zero space, don't paint or hit (state kept)
// ===========================================================================

/// When `offstage`, the object reports zero size and skips painting/hit-testing its
/// child entirely — but the child element stays mounted, so its state persists.
pub struct RenderOffstage {
    pub offstage: bool,
}

impl RenderOffstage {
    pub fn new(offstage: bool) -> Self {
        RenderOffstage { offstage }
    }
}

impl RenderObject for RenderOffstage {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        if self.offstage {
            // Don't lay the child out; a zero-size node is neither painted nor hit
            // (the hit-test rect is empty), and the element stays mounted.
            return constraints.constrain(Size::ZERO);
        }
        match first_child(cx) {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(Size::ZERO),
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if self.offstage {
            return;
        }
        if let Some(child) = first_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderOffstage"
    }
}

// ===========================================================================
// Baseline — position the child so its text baseline sits at a fixed distance
// ===========================================================================

/// Positions the child so its first-text baseline sits `baseline` logical pixels
/// below the top of this box.
pub struct RenderBaseline {
    pub baseline: f64,
}

impl RenderBaseline {
    pub fn new(baseline: f64) -> Self {
        RenderBaseline { baseline }
    }
}

impl RenderObject for RenderBaseline {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let Some(child) = first_child(cx) else {
            return constraints.constrain(Size::ZERO);
        };
        let child_size = cx.layout_child(child, constraints.loosen());
        let child_baseline = cx.child_baseline(child).unwrap_or(child_size.height);
        let top = self.baseline - child_baseline;
        cx.set_child_offset(child, Offset::new(0.0, top));
        constraints.constrain(Size::new(child_size.width, top + child_size.height))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = first_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderBaseline"
    }
}

// ===========================================================================
// RotatedBox — rotate by quarter turns AND swap the layout box for odd turns
// ===========================================================================

/// Rotates the child by `quarter_turns` × 90° and, unlike a paint-only `Transform`,
/// swaps width and height in layout for odd turns so siblings see the rotated extent.
pub struct RenderRotatedBox {
    pub quarter_turns: i32,
    child_size: Size,
}

impl RenderRotatedBox {
    pub fn new(quarter_turns: i32) -> Self {
        RenderRotatedBox { quarter_turns, child_size: Size::ZERO }
    }
    fn is_odd(&self) -> bool {
        self.quarter_turns.rem_euclid(2) == 1
    }
}

impl RenderObject for RenderRotatedBox {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let Some(child) = first_child(cx) else {
            return constraints.constrain(Size::ZERO);
        };
        // Odd turns swap the axes the child is measured against.
        let child_constraints = if self.is_odd() {
            BoxConstraints {
                min_width: constraints.min_height,
                max_width: constraints.max_height,
                min_height: constraints.min_width,
                max_height: constraints.max_width,
            }
        } else {
            constraints
        };
        let child_size = cx.layout_child(child, child_constraints);
        self.child_size = child_size;
        cx.set_child_offset(child, Offset::ZERO);
        if self.is_odd() { Size::new(child_size.height, child_size.width) } else { child_size }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = first_child_paint(cx) {
            cx.paint_child(child, offset);
        }
    }

    fn transform(&self, size: Size) -> Option<Affine> {
        let turns = self.quarter_turns.rem_euclid(4);
        if turns == 0 {
            return None;
        }
        // Rotate about the box center, then step back by the child's own center so the
        // (unrotated) child paints centered within the rotated box.
        Some(
            Affine::translate((size.width / 2.0, size.height / 2.0))
                * Affine::rotate(turns as f64 * FRAC_PI_2)
                * Affine::translate((-self.child_size.width / 2.0, -self.child_size.height / 2.0)),
        )
    }

    fn debug_name(&self) -> &'static str {
        "RenderRotatedBox"
    }
}

// ===========================================================================
// SizedOverflowBox — report a fixed size; let the child overflow it
// ===========================================================================

/// Reports a fixed size (clamped to the incoming constraints) but lays the child out
/// loosely, so the child may be larger and overflow — aligned within the box.
pub struct RenderSizedOverflowBox {
    pub size: Size,
    pub alignment: Alignment,
    position: Offset,
}

impl RenderSizedOverflowBox {
    pub fn new(size: Size, alignment: Alignment) -> Self {
        RenderSizedOverflowBox { size, alignment, position: Offset::ZERO }
    }
}

impl RenderObject for RenderSizedOverflowBox {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let size = constraints.constrain(self.size);
        if let Some(child) = first_child(cx) {
            let child_size = cx.layout_child(child, constraints.loosen());
            let dw = size.width - child_size.width;
            let dh = size.height - child_size.height;
            self.position =
                Offset::new(dw * (self.alignment.x + 1.0) / 2.0, dh * (self.alignment.y + 1.0) / 2.0);
            cx.set_child_offset(child, Offset::ZERO);
        }
        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = first_child_paint(cx) {
            cx.paint_child(child, offset);
        }
    }

    fn transform(&self, _size: Size) -> Option<Affine> {
        if self.position == Offset::ZERO {
            None
        } else {
            Some(Affine::translate((self.position.x, self.position.y)))
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderSizedOverflowBox"
    }
}

// ===========================================================================
// FractionalTranslation — offset the child by a fraction of its own size
// ===========================================================================

/// Translates the child by (`dx`, `dy`) expressed as a fraction of the child's own
/// size — paint/hit only; layout is unaffected (the box takes the child's size).
pub struct RenderFractionalTranslation {
    pub dx: f64,
    pub dy: f64,
}

impl RenderFractionalTranslation {
    pub fn new(dx: f64, dy: f64) -> Self {
        RenderFractionalTranslation { dx, dy }
    }
}

impl RenderObject for RenderFractionalTranslation {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        match first_child(cx) {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(Size::ZERO),
        }
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = first_child_paint(cx) {
            cx.paint_child(child, offset);
        }
    }

    fn transform(&self, size: Size) -> Option<Affine> {
        if self.dx == 0.0 && self.dy == 0.0 {
            None
        } else {
            Some(Affine::translate((self.dx * size.width, self.dy * size.height)))
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderFractionalTranslation"
    }
}

// ===========================================================================
// Table — a column-negotiating grid (the LAYOUT table, not the data table)
// ===========================================================================

/// How a table column's width is decided. Resolved left-to-right against the table's
/// available width; `Flex` columns share whatever is left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TableColumnWidth {
    /// Exactly `px` logical pixels.
    Fixed(f64),
    /// A fraction (`0..1`) of the table's available width.
    Fraction(f64),
    /// The widest natural (unconstrained) content in the column.
    Intrinsic,
    /// A weighted share of the width left after fixed/fraction/intrinsic columns.
    Flex(f64),
}

/// A grid whose columns size by [`TableColumnWidth`] and whose rows take the height of
/// their tallest cell. Cells are the children, in row-major order.
pub struct RenderTable {
    pub columns: Vec<TableColumnWidth>,
    pub column_count: usize,
}

impl RenderTable {
    pub fn new(columns: Vec<TableColumnWidth>, column_count: usize) -> Self {
        RenderTable { columns, column_count }
    }

    fn column_spec(&self, col: usize) -> TableColumnWidth {
        self.columns.get(col).copied().unwrap_or(TableColumnWidth::Flex(1.0))
    }
}

impl RenderObject for RenderTable {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let cols = self.column_count.max(1);
        let children: Vec<RenderId> = cx.children().to_vec();
        if children.is_empty() {
            return constraints.constrain(Size::ZERO);
        }
        let rows = children.len().div_ceil(cols);
        let available =
            if constraints.has_bounded_width() { constraints.max_width } else { constraints.min_width };

        // --- Resolve column widths -----------------------------------------
        let mut widths = vec![0.0_f64; cols];
        let mut flex_total = 0.0_f64;
        let mut used = 0.0_f64;
        for (c, w) in widths.iter_mut().enumerate() {
            match self.column_spec(c) {
                TableColumnWidth::Fixed(px) => {
                    *w = px.max(0.0);
                    used += *w;
                }
                TableColumnWidth::Fraction(f) => {
                    *w = (available * f).max(0.0);
                    used += *w;
                }
                TableColumnWidth::Intrinsic => {
                    // Widest natural cell in this column (measured unconstrained).
                    let mut max_w = 0.0_f64;
                    for r in 0..rows {
                        if let Some(&cell) = children.get(r * cols + c) {
                            max_w = max_w.max(cx.layout_child(cell, BoxConstraints::UNBOUNDED).width);
                        }
                    }
                    *w = max_w;
                    used += *w;
                }
                TableColumnWidth::Flex(weight) => flex_total += weight.max(0.0),
            }
        }
        let leftover = (available - used).max(0.0);
        if flex_total > 0.0 {
            for (c, w) in widths.iter_mut().enumerate() {
                if let TableColumnWidth::Flex(weight) = self.column_spec(c) {
                    *w = leftover * (weight.max(0.0) / flex_total);
                }
            }
        }

        // --- Row heights = tallest cell, then place ------------------------
        let mut row_heights = vec![0.0_f64; rows];
        for (i, &cell) in children.iter().enumerate() {
            let (r, c) = (i / cols, i % cols);
            let size = cx.layout_child(cell, BoxConstraints::tight_for(widths[c], f64::INFINITY));
            row_heights[r] = row_heights[r].max(size.height);
        }
        let col_x: Vec<f64> = (0..cols)
            .scan(0.0, |x, c| {
                let cur = *x;
                *x += widths[c];
                Some(cur)
            })
            .collect();
        let row_y: Vec<f64> = (0..rows)
            .scan(0.0, |y, r| {
                let cur = *y;
                *y += row_heights[r];
                Some(cur)
            })
            .collect();
        for (i, &cell) in children.iter().enumerate() {
            let (r, c) = (i / cols, i % cols);
            cx.set_child_offset(cell, Offset::new(col_x[c], row_y[r]));
        }

        let total_w: f64 = widths.iter().sum();
        let total_h: f64 = row_heights.iter().sum();
        constraints.constrain(Size::new(total_w, total_h))
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderTable"
    }
}

// ===========================================================================
// Custom layout delegates — offset-based (compatible with hit-testing)
// ===========================================================================

/// Picks this box's own size from the incoming constraints.
pub type SizeFn = std::rc::Rc<dyn Fn(BoxConstraints) -> Size>;

/// A delegate that lays out and positions a SINGLE child (Flutter's
/// `CustomSingleChildLayout`). All positions are offsets, so hit-testing is exact.
pub struct RenderCustomSingleChild {
    pub size_fn: SizeFn,
    pub child_constraints_fn: std::rc::Rc<dyn Fn(BoxConstraints) -> BoxConstraints>,
    pub position_fn: std::rc::Rc<dyn Fn(Size, Size) -> Offset>,
}

impl RenderObject for RenderCustomSingleChild {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let size = constraints.constrain((self.size_fn)(constraints));
        if let Some(child) = first_child(cx) {
            let cc = (self.child_constraints_fn)(constraints);
            let child_size = cx.layout_child(child, cc);
            cx.set_child_offset(child, (self.position_fn)(size, child_size));
        }
        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        if let Some(child) = first_child_paint(cx) {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderCustomSingleChild"
    }
}

/// A delegate that lays out and positions MANY children by index (Flutter's
/// `CustomMultiChildLayout`). Positions are offsets, so hit-testing is exact.
pub struct RenderCustomMultiChild {
    pub size_fn: SizeFn,
    pub child_constraints_fn: std::rc::Rc<dyn Fn(usize, BoxConstraints) -> BoxConstraints>,
    pub position_fn: std::rc::Rc<dyn Fn(usize, Size, Size) -> Offset>,
}

impl RenderObject for RenderCustomMultiChild {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let size = constraints.constrain((self.size_fn)(constraints));
        let children: Vec<RenderId> = cx.children().to_vec();
        for (i, &child) in children.iter().enumerate() {
            let cc = (self.child_constraints_fn)(i, constraints);
            let child_size = cx.layout_child(child, cc);
            cx.set_child_offset(child, (self.position_fn)(i, size, child_size));
        }
        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderCustomMultiChild"
    }
}
