//! The long-tail Flutter layout widgets:
//! [`indexed_stack`], [`offstage`], [`visibility`], [`baseline`], [`rotated_box`],
//! [`unconstrained_box`], [`sized_overflow_box`], [`fractional_translation`], and
//! [`layout_builder`].
//!
//! The render-backed ones (`Offstage`/`Baseline`/`RotatedBox`/`SizedOverflowBox`/
//! `FractionalTranslation`) wrap a single child. The composites (`indexed_stack`,
//! `visibility`, `unconstrained_box`) are built from existing primitives, and take
//! care to keep a **stable element structure** across toggles so a hidden child keeps
//! its state. `layout_builder` reacts to its own laid-out size (one frame behind).

use std::rc::Rc;

use pebbles_foundation::{Alignment, Offset, Size};
use pebbles_render::{
    Affine, BorderRadius, BoxConstraints, RenderBaseline, RenderCustomMultiChild, RenderCustomSingleChild,
    RenderFlow, RenderFractionalTranslation, RenderObject, RenderOffstage, RenderRotatedBox,
    RenderSizedOverflowBox, RenderTable, SizeFn, TableColumnWidth,
};

use crate::widgets::{Opacity, SizedBox, clip_rrect, ignore_pointer, overflow_box, stack};
use pebbles_core::widget::{AnyWidget, IntoWidget, RenderWidget};
use pebbles_core::{Element, component_props, use_bounds};

// ===========================================================================
// Offstage — in the tree, but zero-size + not painted/hit
// ===========================================================================

/// Hides `child` from layout, paint, and hit-testing when `offstage` is true, while
/// keeping it mounted (state preserved). Flutter's `Offstage`.
#[derive(Clone)]
pub struct Offstage {
    offstage: bool,
    child: Option<AnyWidget>,
}

/// See [`Offstage`]. `offstage(true, child)` collapses the child to nothing.
pub fn offstage(offstage: bool, child: impl IntoWidget) -> Offstage {
    Offstage { offstage, child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(Offstage);

impl RenderWidget for Offstage {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderOffstage::new(self.offstage))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(o) = object.downcast_mut::<RenderOffstage>() {
            o.offstage = self.offstage;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// Baseline — position the child by its text baseline
// ===========================================================================

/// Positions `child` so its first-text baseline sits `baseline` px below the top.
/// Flutter's `Baseline`.
#[derive(Clone)]
pub struct Baseline {
    baseline: f64,
    child: Option<AnyWidget>,
}

/// See [`Baseline`].
pub fn baseline(baseline: f64, child: impl IntoWidget) -> Baseline {
    Baseline { baseline, child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(Baseline);

impl RenderWidget for Baseline {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderBaseline::new(self.baseline))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(b) = object.downcast_mut::<RenderBaseline>() {
            b.baseline = self.baseline;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// RotatedBox — quarter-turn rotation that also rotates the layout box
// ===========================================================================

/// Rotates `child` by `quarter_turns` × 90°, swapping width/height in layout for odd
/// turns (unlike paint-only `Transform`). Flutter's `RotatedBox`.
#[derive(Clone)]
pub struct RotatedBox {
    quarter_turns: i32,
    child: Option<AnyWidget>,
}

/// See [`RotatedBox`].
pub fn rotated_box(quarter_turns: i32, child: impl IntoWidget) -> RotatedBox {
    RotatedBox { quarter_turns, child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(RotatedBox);

impl RenderWidget for RotatedBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderRotatedBox::new(self.quarter_turns))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderRotatedBox>() {
            r.quarter_turns = self.quarter_turns;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// SizedOverflowBox — fixed reported size, child may overflow
// ===========================================================================

/// Reports a fixed size but lays `child` out loosely, so it may be larger and
/// overflow — aligned within the box. Flutter's `SizedOverflowBox`.
#[derive(Clone)]
pub struct SizedOverflowBox {
    size: Size,
    alignment: Alignment,
    child: Option<AnyWidget>,
}

/// See [`SizedOverflowBox`].
pub fn sized_overflow_box(width: f64, height: f64, child: impl IntoWidget) -> SizedOverflowBox {
    SizedOverflowBox {
        size: Size::new(width, height),
        alignment: Alignment::CENTER,
        child: Some(child.into_widget()),
    }
}

impl SizedOverflowBox {
    /// How the (possibly larger) child is aligned within the reported size.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

pebbles_core::render_widget!(SizedOverflowBox);

impl RenderWidget for SizedOverflowBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderSizedOverflowBox::new(self.size, self.alignment))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(b) = object.downcast_mut::<RenderSizedOverflowBox>() {
            b.size = self.size;
            b.alignment = self.alignment;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// FractionalTranslation — offset by a fraction of the child's own size
// ===========================================================================

/// Translates `child` by (`dx`, `dy`) as a fraction of its own size (paint/hit only;
/// layout unaffected). Flutter's `FractionalTranslation`.
#[derive(Clone)]
pub struct FractionalTranslation {
    dx: f64,
    dy: f64,
    child: Option<AnyWidget>,
}

/// See [`FractionalTranslation`]. `dx`/`dy` of `1.0` shifts by the full width/height.
pub fn fractional_translation(dx: f64, dy: f64, child: impl IntoWidget) -> FractionalTranslation {
    FractionalTranslation { dx, dy, child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(FractionalTranslation);

impl RenderWidget for FractionalTranslation {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderFractionalTranslation::new(self.dx, self.dy))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(t) = object.downcast_mut::<RenderFractionalTranslation>() {
            t.dx = self.dx;
            t.dy = self.dy;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// IndexedStack — show one child, keep all children's state
// ===========================================================================

/// A stack that shows only child `index` but lays out and keeps ALL children mounted
/// (so each keeps its state). Flutter's `IndexedStack`.
///
/// Every child is wrapped identically (opacity + pointer barrier, toggled by
/// selection) so the element structure is stable across index changes — switching the
/// index never remounts a child, preserving its state.
pub fn indexed_stack(index: usize, children: Vec<AnyWidget>) -> impl IntoWidget {
    let kids: Vec<AnyWidget> = children
        .into_iter()
        .enumerate()
        .map(|(i, child)| {
            let selected = i == index;
            ignore_pointer(Opacity::new(if selected { 1.0 } else { 0.0 }, child))
                .enabled(!selected)
                .into_widget()
        })
        .collect();
    stack(kids)
}

// ===========================================================================
// Visibility — show/hide with optional space + state retention
// ===========================================================================

/// Show or hide `child` with control over whether it keeps its space and state.
/// Flutter's `Visibility`. Built by [`visibility`].
#[derive(Clone)]
pub struct Visibility {
    visible: bool,
    child: AnyWidget,
    replacement: Option<AnyWidget>,
    maintain_size: bool,
    maintain_state: bool,
}

/// See [`Visibility`]. By default an invisible child takes no space and is dropped;
/// use [`Visibility::maintain_size`] / [`Visibility::maintain_state`] to change that.
pub fn visibility(visible: bool, child: impl IntoWidget) -> Visibility {
    Visibility {
        visible,
        child: child.into_widget(),
        replacement: None,
        maintain_size: false,
        maintain_state: false,
    }
}

impl Visibility {
    /// What to show in place of a hidden child (default: nothing). Ignored when
    /// [`maintain_size`](Visibility::maintain_size) is set.
    pub fn replacement(mut self, replacement: impl IntoWidget) -> Self {
        self.replacement = Some(replacement.into_widget());
        self
    }
    /// Keep the child's space (and paint it invisibly) while hidden.
    pub fn maintain_size(mut self, maintain: bool) -> Self {
        self.maintain_size = maintain;
        self
    }
    /// Keep the child mounted (state preserved) while hidden.
    pub fn maintain_state(mut self, maintain: bool) -> Self {
        self.maintain_state = maintain;
        self
    }
}

impl IntoWidget for Visibility {
    fn into_widget(self) -> AnyWidget {
        let replacement = self.replacement.unwrap_or_else(|| SizedBox::new(None, None, None).into_widget());
        if self.maintain_size {
            // Reserve space + paint invisibly + swallow no hits; child stays mounted.
            ignore_pointer(Opacity::new(if self.visible { 1.0 } else { 0.0 }, self.child))
                .enabled(!self.visible)
                .into_widget()
        } else if self.maintain_state {
            // Stable structure: child is always present (offstage when hidden), and the
            // replacement offstage when visible — so the child never remounts.
            stack(vec![
                offstage(!self.visible, self.child).into_widget(),
                offstage(self.visible, replacement).into_widget(),
            ])
            .into_widget()
        } else if self.visible {
            self.child
        } else {
            replacement
        }
    }
}

// ===========================================================================
// Table — a column-negotiating grid layout (not the data table)
// ===========================================================================

/// A grid whose columns size by [`TableColumnWidth`] and whose rows take their tallest
/// cell's height. Flutter's `Table` (the layout widget). Built by [`layout_table`].
#[derive(Clone)]
pub struct LayoutTable {
    cells: Vec<AnyWidget>,
    column_count: usize,
    columns: Vec<TableColumnWidth>,
}

/// A [`LayoutTable`] from `rows` of cells. Short rows are padded with empty cells so the
/// grid stays rectangular. Columns default to equal [`TableColumnWidth::Flex`]; set
/// them with [`Table::column_widths`].
pub fn layout_table(rows: Vec<Vec<AnyWidget>>) -> LayoutTable {
    let column_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut cells = Vec::with_capacity(rows.len() * column_count);
    for mut row in rows {
        while row.len() < column_count {
            row.push(SizedBox::new(None, None, None).into_widget());
        }
        cells.extend(row);
    }
    LayoutTable { cells, column_count, columns: vec![TableColumnWidth::Flex(1.0); column_count] }
}

impl LayoutTable {
    /// Per-column width specs (length should match the column count; missing columns
    /// default to `Flex(1)`).
    pub fn column_widths(mut self, columns: Vec<TableColumnWidth>) -> Self {
        self.columns = columns;
        self
    }
}

pebbles_core::render_widget!(LayoutTable);

impl RenderWidget for LayoutTable {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderTable::new(self.columns.clone(), self.column_count))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(t) = object.downcast_mut::<RenderTable>() {
            t.columns = self.columns.clone();
            t.column_count = self.column_count;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        std::mem::take(&mut self.cells)
    }
}

// ===========================================================================
// UnconstrainedBox — let the child size itself, then shrink to it (clipped)
// ===========================================================================

/// Lets `child` size itself with no constraints, sizes to it (clamped to the incoming
/// constraints, overflow clipped). Flutter's `UnconstrainedBox`.
pub fn unconstrained_box(child: impl IntoWidget) -> impl IntoWidget {
    clip_rrect(BorderRadius::ZERO, overflow_box(child))
}

// ===========================================================================
// CustomSingleChildLayout / CustomMultiChildLayout — offset-based delegates
// ===========================================================================

/// Lay out and position a single child with your own functions. Flutter's
/// `CustomSingleChildLayout`. Positions are offsets, so hit-testing stays exact.
#[derive(Clone)]
pub struct CustomSingleChildLayout {
    child: Option<AnyWidget>,
    size_fn: SizeFn,
    child_constraints_fn: Rc<dyn Fn(BoxConstraints) -> BoxConstraints>,
    position_fn: Rc<dyn Fn(Size, Size) -> Offset>,
}

/// See [`CustomSingleChildLayout`]. Defaults: box fills the constraints, the child
/// gets them unchanged, positioned at the top-left — override with the builders.
pub fn custom_single_child_layout(child: impl IntoWidget) -> CustomSingleChildLayout {
    CustomSingleChildLayout {
        child: Some(child.into_widget()),
        size_fn: Rc::new(|c: BoxConstraints| c.biggest()),
        child_constraints_fn: Rc::new(|c| c),
        position_fn: Rc::new(|_, _| Offset::ZERO),
    }
}

impl CustomSingleChildLayout {
    /// This box's own size, from the incoming constraints.
    pub fn size(mut self, f: impl Fn(BoxConstraints) -> Size + 'static) -> Self {
        self.size_fn = Rc::new(f);
        self
    }
    /// The constraints handed to the child.
    pub fn child_constraints(mut self, f: impl Fn(BoxConstraints) -> BoxConstraints + 'static) -> Self {
        self.child_constraints_fn = Rc::new(f);
        self
    }
    /// The child's offset, given `(this_size, child_size)`.
    pub fn position(mut self, f: impl Fn(Size, Size) -> Offset + 'static) -> Self {
        self.position_fn = Rc::new(f);
        self
    }
}

pebbles_core::render_widget!(CustomSingleChildLayout);

impl RenderWidget for CustomSingleChildLayout {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderCustomSingleChild {
            size_fn: self.size_fn.clone(),
            child_constraints_fn: self.child_constraints_fn.clone(),
            position_fn: self.position_fn.clone(),
        })
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(o) = object.downcast_mut::<RenderCustomSingleChild>() {
            o.size_fn = self.size_fn.clone();
            o.child_constraints_fn = self.child_constraints_fn.clone();
            o.position_fn = self.position_fn.clone();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

/// Lay out and position many children by index with your own functions. Flutter's
/// `CustomMultiChildLayout`. Positions are offsets, so hit-testing stays exact.
#[derive(Clone)]
pub struct CustomMultiChildLayout {
    children: Vec<AnyWidget>,
    size_fn: SizeFn,
    child_constraints_fn: Rc<dyn Fn(usize, BoxConstraints) -> BoxConstraints>,
    position_fn: Rc<dyn Fn(usize, Size, Size) -> Offset>,
}

/// See [`CustomMultiChildLayout`]. Defaults: box fills the constraints, each child gets
/// loosened constraints, positioned at the top-left — override with the builders.
pub fn custom_multi_child_layout(children: Vec<AnyWidget>) -> CustomMultiChildLayout {
    CustomMultiChildLayout {
        children,
        size_fn: Rc::new(|c: BoxConstraints| c.biggest()),
        child_constraints_fn: Rc::new(|_, c: BoxConstraints| c.loosen()),
        position_fn: Rc::new(|_, _, _| Offset::ZERO),
    }
}

impl CustomMultiChildLayout {
    /// This box's own size, from the incoming constraints.
    pub fn size(mut self, f: impl Fn(BoxConstraints) -> Size + 'static) -> Self {
        self.size_fn = Rc::new(f);
        self
    }
    /// The constraints handed to child `index`.
    pub fn child_constraints(
        mut self,
        f: impl Fn(usize, BoxConstraints) -> BoxConstraints + 'static,
    ) -> Self {
        self.child_constraints_fn = Rc::new(f);
        self
    }
    /// Child `index`'s offset, given `(index, this_size, child_size)`.
    pub fn position(mut self, f: impl Fn(usize, Size, Size) -> Offset + 'static) -> Self {
        self.position_fn = Rc::new(f);
        self
    }
}

pebbles_core::render_widget!(CustomMultiChildLayout);

impl RenderWidget for CustomMultiChildLayout {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderCustomMultiChild {
            size_fn: self.size_fn.clone(),
            child_constraints_fn: self.child_constraints_fn.clone(),
            position_fn: self.position_fn.clone(),
        })
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(o) = object.downcast_mut::<RenderCustomMultiChild>() {
            o.size_fn = self.size_fn.clone();
            o.child_constraints_fn = self.child_constraints_fn.clone();
            o.position_fn = self.position_fn.clone();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        std::mem::take(&mut self.children)
    }
}

// ===========================================================================
// Flow — per-child affine transforms from a delegate
// ===========================================================================

/// Positions each child by an arbitrary affine **transform** from a delegate.
/// Flutter's `Flow`. Because the transform is applied on the child's node (not just an
/// offset), rotation/scale flows both paint AND hit-test correctly.
#[derive(Clone)]
pub struct Flow {
    children: Vec<AnyWidget>,
    size_fn: SizeFn,
    child_constraints_fn: Rc<dyn Fn(usize, BoxConstraints) -> BoxConstraints>,
    transform_fn: Rc<dyn Fn(usize, Size, Size) -> Affine>,
}

/// See [`Flow`]. Defaults: box fills the constraints, each child gets loosened
/// constraints and the identity transform — set [`Flow::transform`] to place them.
pub fn flow(children: Vec<AnyWidget>) -> Flow {
    Flow {
        children,
        size_fn: Rc::new(|c: BoxConstraints| c.biggest()),
        child_constraints_fn: Rc::new(|_, c: BoxConstraints| c.loosen()),
        transform_fn: Rc::new(|_, _, _| Affine::IDENTITY),
    }
}

impl Flow {
    /// This box's own size, from the incoming constraints.
    pub fn size(mut self, f: impl Fn(BoxConstraints) -> Size + 'static) -> Self {
        self.size_fn = Rc::new(f);
        self
    }
    /// The constraints handed to child `index`.
    pub fn child_constraints(
        mut self,
        f: impl Fn(usize, BoxConstraints) -> BoxConstraints + 'static,
    ) -> Self {
        self.child_constraints_fn = Rc::new(f);
        self
    }
    /// The affine transform for child `index`, given `(index, this_size, child_size)`.
    pub fn transform(mut self, f: impl Fn(usize, Size, Size) -> Affine + 'static) -> Self {
        self.transform_fn = Rc::new(f);
        self
    }
}

pebbles_core::render_widget!(Flow);

impl RenderWidget for Flow {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderFlow {
            size_fn: self.size_fn.clone(),
            child_constraints_fn: self.child_constraints_fn.clone(),
            transform_fn: self.transform_fn.clone(),
        })
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(o) = object.downcast_mut::<RenderFlow>() {
            o.size_fn = self.size_fn.clone();
            o.child_constraints_fn = self.child_constraints_fn.clone();
            o.transform_fn = self.transform_fn.clone();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        std::mem::take(&mut self.children)
    }
}

// ===========================================================================
// LayoutBuilder — build against the available size (one frame behind)
// ===========================================================================

/// Builds against this widget's laid-out size, rebuilding when it changes. Flutter's
/// `LayoutBuilder`.
///
/// NOTE: Pebbles builds the widget tree before layout, so — unlike Flutter — this
/// reads the size from the *previous* frame ([`use_bounds`]) and is therefore one
/// frame behind on the first paint and on a resize. It reports the size this widget
/// was actually given, so let it stretch (e.g. inside an `Expanded`/stretch column)
/// for a responsive width.
#[derive(Clone)]
pub struct LayoutBuilder {
    builder: Rc<dyn Fn(Size) -> AnyWidget>,
}

/// See [`LayoutBuilder`]. `builder(size)` receives the available size.
pub fn layout_builder<W: IntoWidget>(builder: impl Fn(Size) -> W + 'static) -> LayoutBuilder {
    LayoutBuilder { builder: Rc::new(move |s| builder(s).into_widget()) }
}

impl IntoWidget for LayoutBuilder {
    fn into_widget(self) -> AnyWidget {
        component_props(render_layout_builder, self).into_widget()
    }
}

fn render_layout_builder(b: &LayoutBuilder) -> Element {
    let bounds = use_bounds(); // window-space rect, one frame behind
    (b.builder)(Size::new(bounds.width(), bounds.height()))
}
