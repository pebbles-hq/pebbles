//! [`ExtentProbe`] — measures its child's laid-out extent along one axis and
//! reports changes to a callback. The building block for widgets that must size
//! themselves from a live measurement (a carousel page = the measured viewport
//! width). Reports only when the extent changed by more than 0.5px, so a settled
//! layout never re-renders.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_foundation::{Axis, Offset, Size};
use pebbles_render::{RenderMeasureProbe, RenderObject, RenderPositionProbe};

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

/// A layout pass-through that reports its child's **window-space rect** (top-left +
/// size) after each paint. Used to open a menu/popover anchored to a trigger via
/// the keyboard, where there is no pointer event to derive the rect from. The
/// callback should stash the rect somewhere non-reactive (an `Rc<Cell>`), NOT set a
/// signal — it fires during paint, so a reactive write would dirty the frame.
#[derive(Clone)]
pub struct PositionProbe {
    on_change: Rc<dyn Fn(Offset, Size)>,
    child: Option<AnyWidget>,
}

/// Wrap `child` so its window-space rect (top-left + size) is reported to
/// `on_change` after each paint (deduped: only when it moves/resizes by >0.5px).
pub fn position_probe(
    on_change: impl Fn(Offset, Size) + 'static,
    child: impl pebbles_core::IntoWidget,
) -> PositionProbe {
    PositionProbe { on_change: Rc::new(on_change), child: Some(child.into_widget()) }
}

pebbles_core::render_widget!(PositionProbe);

impl RenderWidget for PositionProbe {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderPositionProbe::new(Some(self.report())))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(p) = object.downcast_mut::<RenderPositionProbe>() {
            p.report = Some(self.report());
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

impl PositionProbe {
    fn report(&self) -> Rc<dyn Fn(Offset, Size)> {
        let cb = self.on_change.clone();
        let last: Rc<Cell<(f64, f64, f64, f64)>> = Rc::new(Cell::new((f64::NAN, 0.0, 0.0, 0.0)));
        Rc::new(move |off: Offset, size: Size| {
            let (px, py, pw, ph) = last.get();
            let changed = px.is_nan()
                || (off.x - px).abs() > 0.5
                || (off.y - py).abs() > 0.5
                || (size.width - pw).abs() > 0.5
                || (size.height - ph).abs() > 0.5;
            if changed {
                last.set((off.x, off.y, size.width, size.height));
                cb(off, size);
            }
        })
    }
}
