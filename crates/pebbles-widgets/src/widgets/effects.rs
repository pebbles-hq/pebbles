//! Compositing widgets: [`Opacity`] and [`ClipRRect`].

use pebbles_render::{BorderRadius, RenderClipRRect, RenderObject, RenderOpacity};

use pebbles_core::widget::{AnyWidget, RenderWidget};

/// Fades its child by a uniform alpha in `0.0..=1.0`.
#[derive(Clone)]
pub struct Opacity {
    pub opacity: f32,
    child: Option<AnyWidget>,
}

impl Opacity {
    pub fn new(opacity: f32, child: impl pebbles_core::IntoWidget) -> Self {
        Opacity { opacity: opacity.clamp(0.0, 1.0), child: Some(child.into_widget()) }
    }
}

pebbles_core::render_widget!(Opacity);

impl RenderWidget for Opacity {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderOpacity::new(self.opacity))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderOpacity>() {
            r.opacity = self.opacity;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

/// Clips its child to a rounded rectangle.
#[derive(Clone)]
pub struct ClipRRect {
    pub radius: BorderRadius,
    child: Option<AnyWidget>,
}

impl ClipRRect {
    pub fn new(radius: BorderRadius, child: impl pebbles_core::IntoWidget) -> Self {
        ClipRRect { radius, child: Some(child.into_widget()) }
    }
}

pebbles_core::render_widget!(ClipRRect);

impl RenderWidget for ClipRRect {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderClipRRect::new(self.radius))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderClipRRect>() {
            r.radius = self.radius;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

/// A repaint boundary: the child subtree encodes into a retained scene fragment,
/// re-encoded only when something inside it changes — clean frames re-APPEND the
/// fragment at the current offset instead of re-encoding glyphs and paths.
/// Virtualized list items get one automatically; wrap expensive static subtrees
/// manually when profiling shows re-encode cost.
#[derive(Clone)]
pub struct RepaintBoundary {
    child: Option<AnyWidget>,
}

/// Wrap `child` in a [`RepaintBoundary`].
pub fn repaint_boundary(child: impl pebbles_core::IntoWidget) -> RepaintBoundary {
    RepaintBoundary { child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(RepaintBoundary);

impl RenderWidget for RepaintBoundary {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(pebbles_render::RenderBoundary::new())
    }
    fn update_render_object(&self, _object: &mut dyn RenderObject) {}
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
