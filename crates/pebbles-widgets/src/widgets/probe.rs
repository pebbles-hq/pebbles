//! [`ExtentProbe`] — measures its child's laid-out extent along one axis and
//! reports changes to a callback. The building block for widgets that must size
//! themselves from a live measurement (a carousel page = the measured viewport
//! width). Reports only when the extent changed by more than 0.5px, so a settled
//! layout never re-renders.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_foundation::Axis;
use pebbles_render::{RenderMeasureProbe, RenderObject};

use pebbles_core::widget::{AnyWidget, RenderWidget};

/// A layout pass-through that reports its child's extent along `axis`.
#[derive(Clone)]
pub struct ExtentProbe {
    axis: Axis,
    on_change: Rc<dyn Fn(f64)>,
    child: Option<AnyWidget>,
}

/// Wrap `child` so its laid-out extent along `axis` is reported to `on_change`
/// (fired on every change of more than 0.5px — not on re-layouts of an
/// unchanged extent).
pub fn extent_probe(
    axis: Axis,
    on_change: impl Fn(f64) + 'static,
    child: impl pebbles_core::IntoWidget,
) -> ExtentProbe {
    ExtentProbe { axis, on_change: Rc::new(on_change), child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(ExtentProbe);

impl RenderWidget for ExtentProbe {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderMeasureProbe::new(self.axis, Some(self.report())))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(p) = object.downcast_mut::<RenderMeasureProbe>() {
            p.axis = self.axis;
            p.report = Some(self.report());
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

impl ExtentProbe {
    fn report(&self) -> Rc<dyn Fn(f64)> {
        let cb = self.on_change.clone();
        let last = Rc::new(Cell::new(f64::NAN));
        Rc::new(move |extent: f64| {
            let prev = last.get();
            if prev.is_nan() || (extent - prev).abs() > 0.5 {
                last.set(extent);
                cb(extent);
            }
        })
    }
}
