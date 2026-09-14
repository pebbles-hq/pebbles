//! More painting & effects widgets: [`clip_rect`], [`clip_oval`], [`clip_path`],
//! [`color_filtered`], and [`shader_mask`] — siblings of the existing `clip_rrect` /
//! `opacity`. Each wraps a single child and pushes a vello layer around (or over) it.

use std::rc::Rc;

use pebbles_foundation::{Color, Size};
use pebbles_render::{
    BezPath, BlendMode, BorderRadius, Gradient, RenderClipOval, RenderClipPath, RenderColorFilter,
    RenderObject, RenderShaderMask,
};

use crate::widgets::{ClipRRect, clip_rrect};
use pebbles_core::widget::{AnyWidget, IntoWidget, RenderWidget};

/// Clip `child` to a plain rectangle (its bounds). A thin convenience over
/// [`clip_rrect`] with a zero radius — Flutter's `ClipRect`.
pub fn clip_rect(child: impl IntoWidget) -> ClipRRect {
    clip_rrect(BorderRadius::ZERO, child)
}

// ===========================================================================
// ClipOval
// ===========================================================================

/// Clips `child` to the ellipse (a circle for a square box) inscribed in its bounds.
/// Flutter's `ClipOval`.
#[derive(Clone)]
pub struct ClipOval {
    child: Option<AnyWidget>,
}

/// See [`ClipOval`].
pub fn clip_oval(child: impl IntoWidget) -> ClipOval {
    ClipOval { child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(ClipOval);

impl RenderWidget for ClipOval {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderClipOval)
    }
    fn update_render_object(&self, _object: &mut dyn RenderObject) {}
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// ClipPath
// ===========================================================================

/// Clips `child` to a path your delegate builds from the box size (Flutter's
/// `ClipPath` + `CustomClipper<Path>`). Built by [`clip_path`].
#[derive(Clone)]
pub struct ClipPath {
    path_fn: Rc<dyn Fn(Size) -> BezPath>,
    child: Option<AnyWidget>,
}

/// See [`ClipPath`]. `clipper(size)` returns the clip path in the box's local space.
pub fn clip_path(clipper: impl Fn(Size) -> BezPath + 'static, child: impl IntoWidget) -> ClipPath {
    ClipPath { path_fn: Rc::new(clipper), child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(ClipPath);

impl RenderWidget for ClipPath {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderClipPath { path_fn: self.path_fn.clone() })
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(o) = object.downcast_mut::<RenderClipPath>() {
            o.path_fn = self.path_fn.clone();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// ColorFiltered
// ===========================================================================

/// Blends a `color` over `child` with a blend mode (Flutter's `ColorFiltered` with a
/// `ColorFilter.mode`). Built by [`color_filtered`].
#[derive(Clone)]
pub struct ColorFiltered {
    color: Color,
    blend: BlendMode,
    child: Option<AnyWidget>,
}

/// See [`ColorFiltered`]. Default blend is `Multiply` (Flutter's modulate); change it
/// with [`ColorFiltered::blend`].
pub fn color_filtered(color: Color, child: impl IntoWidget) -> ColorFiltered {
    ColorFiltered { color, blend: BlendMode::Multiply, child: Some(child.into_widget()) }
}

impl ColorFiltered {
    /// The blend mode used to composite the color over the child.
    pub fn blend(mut self, blend: BlendMode) -> Self {
        self.blend = blend;
        self
    }
}

pebbles_core::render_widget!(ColorFiltered);

impl RenderWidget for ColorFiltered {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderColorFilter { color: self.color, blend: self.blend })
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(o) = object.downcast_mut::<RenderColorFilter>() {
            o.color = self.color;
            o.blend = self.blend;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ===========================================================================
// ShaderMask
// ===========================================================================

/// Masks `child` by a gradient's luminance — bright areas keep the child, dark areas
/// hide it (Flutter's `ShaderMask`, the common fade/vignette). Built by [`shader_mask`].
#[derive(Clone)]
pub struct ShaderMask {
    gradient: Gradient,
    child: Option<AnyWidget>,
}

/// See [`ShaderMask`].
pub fn shader_mask(gradient: Gradient, child: impl IntoWidget) -> ShaderMask {
    ShaderMask { gradient, child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(ShaderMask);

impl RenderWidget for ShaderMask {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderShaderMask { gradient: self.gradient.clone() })
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(o) = object.downcast_mut::<RenderShaderMask>() {
            o.gradient = self.gradient.clone();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
