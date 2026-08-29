//! Additional layout widgets beyond flex: [`Wrap`] (flow) and [`AspectRatio`].

use pebbles_render::{RenderAspectRatio, RenderObject, RenderWrap};

use pebbles_core::widget::{AnyWidget, RenderWidget};

/// A flow layout: children fill a run, then wrap to the next line.
#[derive(Clone)]
pub struct Wrap {
    children: Vec<AnyWidget>,
    spacing: f64,
    run_spacing: f64,
}

/// Create a [`Wrap`] flowing `children`. Accepts `children![…]` or any iterator.
pub fn wrap<I, W>(children: I) -> Wrap
where
    I: IntoIterator<Item = W>,
    W: pebbles_core::widget::IntoWidget,
{
    Wrap { children: pebbles_core::widget::collect_widgets(children), spacing: 8.0, run_spacing: 8.0 }
}

impl Wrap {
    /// Gap between items within a run.
    pub fn spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing;
        self
    }
    /// Gap between runs.
    pub fn run_spacing(mut self, run_spacing: f64) -> Self {
        self.run_spacing = run_spacing;
        self
    }
}

pebbles_core::render_widget!(Wrap);

impl RenderWidget for Wrap {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderWrap::new(self.spacing, self.run_spacing))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(w) = object.downcast_mut::<RenderWrap>() {
            w.spacing = self.spacing;
            w.run_spacing = self.run_spacing;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        std::mem::take(&mut self.children)
    }
}

/// Forces its child to a fixed width:height ratio.
#[derive(Clone)]
pub struct AspectRatio {
    ratio: f64,
    child: Option<AnyWidget>,
}

/// Create an [`AspectRatio`] (`ratio` = width / height).
pub fn aspect_ratio(ratio: f64, child: impl pebbles_core::IntoWidget) -> AspectRatio {
    AspectRatio { ratio, child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(AspectRatio);

impl RenderWidget for AspectRatio {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderAspectRatio::new(self.ratio))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(a) = object.downcast_mut::<RenderAspectRatio>() {
            a.ratio = self.ratio.max(0.0001);
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
