//! The constraint/fit widgets: [`FittedBox`], [`FractionallySizedBox`],
//! [`IntrinsicWidth`] / [`IntrinsicHeight`], [`LimitedBox`] and [`OverflowBox`] —
//! the Flutter sizing vocabulary that sits between plain boxes and full layouts.
//! Each wraps exactly one render object.

use pebbles_foundation::{Alignment, BoxFit};
use pebbles_render::{
    RenderFittedBox, RenderFractionallySizedBox, RenderIntrinsicHeight, RenderIntrinsicWidth,
    RenderLimitedBox, RenderObject, RenderOverflowBox,
};

use pebbles_core::widget::{AnyWidget, RenderWidget};

// ---------------------------------------------------------------------------
// FittedBox
// ---------------------------------------------------------------------------

/// Scales its child (laid out at its natural size) to fit the box it is given,
/// per a [`BoxFit`]. Flutter's `FittedBox`: the classic way to make content
/// responsive to its parent — scale a wide banner down to a narrow column.
#[derive(Clone)]
pub struct FittedBox {
    pub fit: BoxFit,
    pub alignment: Alignment,
    child: Option<AnyWidget>,
}

/// Fit `child` into its parent per `fit` (default [`BoxFit::Contain`]).
pub fn fitted_box(child: impl pebbles_core::IntoWidget) -> FittedBox {
    FittedBox { fit: BoxFit::Contain, alignment: Alignment::CENTER, child: Some(child.into_widget()) }
}

impl FittedBox {
    /// How the child is scaled into the box (Contain/Cover/Fill/None/FitWidth/
    /// FitHeight/ScaleDown).
    pub fn fit(mut self, fit: BoxFit) -> Self {
        self.fit = fit;
        self
    }
    /// Where the scaled child sits within the leftover space (default: center).
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

pebbles_core::render_widget!(FittedBox);

impl RenderWidget for FittedBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderFittedBox::new(self.fit, self.alignment))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderFittedBox>() {
            r.fit = self.fit;
            r.alignment = self.alignment;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// FractionallySizedBox
// ---------------------------------------------------------------------------

/// Sizes its child to a fraction of the incoming constraints
/// (Flutter's `FractionallySizedBox`): `.width_factor(0.5)` = half the available
/// width; unset axes pass through.
#[derive(Clone)]
pub struct FractionallySizedBox {
    pub width_factor: Option<f64>,
    pub height_factor: Option<f64>,
    pub alignment: Alignment,
    child: Option<AnyWidget>,
}

/// Wrap `child` in a box whose size is driven by fractions of the parent's
/// constraints.
pub fn fractionally_sized_box(child: impl pebbles_core::IntoWidget) -> FractionallySizedBox {
    FractionallySizedBox {
        width_factor: None,
        height_factor: None,
        alignment: Alignment::CENTER,
        child: Some(child.into_widget()),
    }
}

impl FractionallySizedBox {
    /// Child width as a fraction of the incoming max width (`0..=1` typically).
    pub fn width_factor(mut self, factor: f64) -> Self {
        self.width_factor = Some(factor);
        self
    }
    /// Child height as a fraction of the incoming max height.
    pub fn height_factor(mut self, factor: f64) -> Self {
        self.height_factor = Some(factor);
        self
    }
    /// Where the child sits within the (full-size) box.
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

pebbles_core::render_widget!(FractionallySizedBox);

impl RenderWidget for FractionallySizedBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderFractionallySizedBox::new(
            self.width_factor,
            self.height_factor,
            self.alignment,
        ))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderFractionallySizedBox>() {
            r.width_factor = self.width_factor;
            r.height_factor = self.height_factor;
            r.alignment = self.alignment;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// IntrinsicWidth / IntrinsicHeight
// ---------------------------------------------------------------------------

/// Sizes its child's **width** to the child's intrinsic width — "how wide would
/// you be if nobody constrained you?" (Flutter's `IntrinsicWidth`). The basis for
/// shrink-wrap layouts: a column as wide as its widest child.
#[derive(Clone)]
pub struct IntrinsicWidth {
    child: Option<AnyWidget>,
}

/// Shrink-wrap `child`'s width to its intrinsic extent.
pub fn intrinsic_width(child: impl pebbles_core::IntoWidget) -> IntrinsicWidth {
    IntrinsicWidth { child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(IntrinsicWidth);

impl RenderWidget for IntrinsicWidth {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderIntrinsicWidth::new())
    }
    fn update_render_object(&self, _object: &mut dyn RenderObject) {}
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

/// Sizes its child's **height** to the child's intrinsic height — "how tall would
/// you be if nobody constrained you?" (Flutter's `IntrinsicHeight`). Lets siblings
/// stretch to match a tall child.
#[derive(Clone)]
pub struct IntrinsicHeight {
    child: Option<AnyWidget>,
}

/// Shrink-wrap `child`'s height to its intrinsic extent.
pub fn intrinsic_height(child: impl pebbles_core::IntoWidget) -> IntrinsicHeight {
    IntrinsicHeight { child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(IntrinsicHeight);

impl RenderWidget for IntrinsicHeight {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderIntrinsicHeight::new())
    }
    fn update_render_object(&self, _object: &mut dyn RenderObject) {}
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// LimitedBox
// ---------------------------------------------------------------------------

/// Caps its child's size, but only on axes where the incoming constraints are
/// unbounded (Flutter's `LimitedBox`: a bounded axis passes straight through).
/// Use it inside unbounded containers (lists, scroll views) to give a child a
/// sane maximum.
#[derive(Clone)]
pub struct LimitedBox {
    pub max_width: f64,
    pub max_height: f64,
    child: Option<AnyWidget>,
}

/// Limit `child` to `max_width` × `max_height` on unbounded axes only.
pub fn limited_box(child: impl pebbles_core::IntoWidget) -> LimitedBox {
    LimitedBox { max_width: f64::INFINITY, max_height: f64::INFINITY, child: Some(child.into_widget()) }
}

impl LimitedBox {
    /// The maximum width applied when the incoming width is unbounded.
    pub fn max_width(mut self, width: f64) -> Self {
        self.max_width = width;
        self
    }
    /// The maximum height applied when the incoming height is unbounded.
    pub fn max_height(mut self, height: f64) -> Self {
        self.max_height = height;
        self
    }
}

pebbles_core::render_widget!(LimitedBox);

impl RenderWidget for LimitedBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderLimitedBox::new(self.max_width, self.max_height))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderLimitedBox>() {
            r.max_width = self.max_width;
            r.max_height = self.max_height;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// OverflowBox
// ---------------------------------------------------------------------------

/// Lets its child size itself naturally and **overflow** the box's own bounds
/// (paint is not clipped) — Flutter's `OverflowBox`. The classic way to pop a
/// badge out of an avatar's corner or overhang a banner from a row.
#[derive(Clone)]
pub struct OverflowBox {
    pub alignment: Alignment,
    child: Option<AnyWidget>,
}

/// Let `child` overflow the box it is given, positioned per `alignment`
/// (default: center).
pub fn overflow_box(child: impl pebbles_core::IntoWidget) -> OverflowBox {
    OverflowBox { alignment: Alignment::CENTER, child: Some(child.into_widget()) }
}

impl OverflowBox {
    /// Where the child sits when it is smaller than the box (and which direction
    /// it overflows toward).
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

pebbles_core::render_widget!(OverflowBox);

impl RenderWidget for OverflowBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderOverflowBox::new(self.alignment))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderOverflowBox>() {
            r.alignment = self.alignment;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
