//! Single-child layout widgets: [`ColoredBox`], [`Padding`], [`Align`]/[`center`],
//! [`SizedBox`] and [`ConstrainedBox`]. Each wraps one render object.

use pebbles_foundation::{Alignment, Color, EdgeInsets};
use pebbles_render::{
    BoxConstraints, RenderAlign, RenderColoredBox, RenderConstrainedBox, RenderObject,
    RenderPadding,
};

use pebbles_core::widget::{AnyWidget, RenderWidget};

// ---------------------------------------------------------------------------
// ColoredBox
// ---------------------------------------------------------------------------

/// Fills its child's box with a solid color.
#[derive(Clone)]
pub struct ColoredBox {
    pub color: Color,
    child: Option<AnyWidget>,
}

impl ColoredBox {
    pub fn new(color: Color, child: impl pebbles_core::IntoWidget) -> Self {
        ColoredBox { color, child: Some(child.into_widget()) }
    }
}

pebbles_core::render_widget!(ColoredBox);

impl RenderWidget for ColoredBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderColoredBox::new(self.color))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderColoredBox>() {
            r.color = self.color;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Padding
// ---------------------------------------------------------------------------

/// Insets its child by [`EdgeInsets`].
#[derive(Clone)]
pub struct Padding {
    pub insets: EdgeInsets,
    child: Option<AnyWidget>,
}

impl Padding {
    pub fn new(insets: EdgeInsets, child: impl pebbles_core::IntoWidget) -> Self {
        Padding { insets, child: Some(child.into_widget()) }
    }
    /// Equal padding on all four sides.
    pub fn all(value: f64, child: impl pebbles_core::IntoWidget) -> Self {
        Padding::new(EdgeInsets::all(value), child)
    }
}

pebbles_core::render_widget!(Padding);

impl RenderWidget for Padding {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderPadding::new(self.insets))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderPadding>() {
            r.insets = self.insets;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// Align / center
// ---------------------------------------------------------------------------

/// Positions its child within itself per an [`Alignment`].
#[derive(Clone)]
pub struct Align {
    pub alignment: Alignment,
    child: Option<AnyWidget>,
}

impl Align {
    pub fn new(alignment: Alignment, child: impl pebbles_core::IntoWidget) -> Self {
        Align { alignment, child: Some(child.into_widget()) }
    }
}

/// Center a child within the available space.
pub fn center(child: impl pebbles_core::IntoWidget) -> Align {
    Align::new(Alignment::CENTER, child)
}

pebbles_core::render_widget!(Align);

impl RenderWidget for Align {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderAlign::new(self.alignment))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderAlign>() {
            r.alignment = self.alignment;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// SizedBox
// ---------------------------------------------------------------------------

/// Forces a specific width and/or height on its child (unset axes pass through).
#[derive(Clone)]
pub struct SizedBox {
    pub width: Option<f64>,
    pub height: Option<f64>,
    child: Option<AnyWidget>,
}

impl SizedBox {
    pub fn new(width: Option<f64>, height: Option<f64>, child: Option<AnyWidget>) -> Self {
        SizedBox { width, height, child }
    }
    /// A fixed-size box wrapping a child.
    pub fn exact(width: f64, height: f64, child: impl pebbles_core::IntoWidget) -> Self {
        SizedBox::new(Some(width), Some(height), Some(child.into_widget()))
    }
    /// An empty box of a fixed size — handy as a spacer.
    pub fn spacer(width: f64, height: f64) -> Self {
        SizedBox::new(Some(width), Some(height), None)
    }
    /// A square box of side `dim` (Flutter's `SizedBox.square`).
    pub fn square(dim: f64, child: impl pebbles_core::IntoWidget) -> Self {
        SizedBox::new(Some(dim), Some(dim), Some(child.into_widget()))
    }
    /// Expands to fill the parent on both axes (Flutter's `SizedBox.expand`).
    pub fn expand(child: impl pebbles_core::IntoWidget) -> Self {
        SizedBox::new(Some(f64::INFINITY), Some(f64::INFINITY), Some(child.into_widget()))
    }
    /// A zero-size box (Flutter's `SizedBox.shrink`).
    pub fn shrink() -> Self {
        SizedBox::new(Some(0.0), Some(0.0), None)
    }
    /// Force a width on the child (unset height passes through).
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }
    /// Force a height on the child (unset width passes through).
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }

    fn constraints(&self) -> BoxConstraints {
        let (min_w, max_w) = match self.width {
            Some(w) => (w, w),
            None => (0.0, f64::INFINITY),
        };
        let (min_h, max_h) = match self.height {
            Some(h) => (h, h),
            None => (0.0, f64::INFINITY),
        };
        BoxConstraints { min_width: min_w, max_width: max_w, min_height: min_h, max_height: max_h }
    }
}

/// Wrap a child in a [`SizedBox`], then chain `.width()` / `.height()` to force a
/// size — Flutter's `SizedBox(width:, height:, child:)`. Unset axes pass through.
pub fn sized_box(child: impl pebbles_core::IntoWidget) -> SizedBox {
    SizedBox::new(None, None, Some(child.into_widget()))
}

pebbles_core::render_widget!(SizedBox);

impl RenderWidget for SizedBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderConstrainedBox::new(self.constraints()))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderConstrainedBox>() {
            r.additional = self.constraints();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// ConstrainedBox
// ---------------------------------------------------------------------------

/// Imposes explicit additional [`BoxConstraints`] on its child.
#[derive(Clone)]
pub struct ConstrainedBox {
    pub constraints: BoxConstraints,
    child: Option<AnyWidget>,
}

impl ConstrainedBox {
    pub fn new(constraints: BoxConstraints, child: impl pebbles_core::IntoWidget) -> Self {
        ConstrainedBox { constraints, child: Some(child.into_widget()) }
    }
}

pebbles_core::render_widget!(ConstrainedBox);

impl RenderWidget for ConstrainedBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderConstrainedBox::new(self.constraints))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderConstrainedBox>() {
            r.additional = self.constraints;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
