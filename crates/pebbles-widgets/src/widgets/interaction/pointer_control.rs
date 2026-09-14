//! [`IgnorePointer`] / [`AbsorbPointer`] — control whether a subtree takes part in
//! hit testing (Flutter's widgets of the same names).
//!
//! * [`ignore_pointer`]`(child)` — the child (and its subtree) become transparent to
//!   the pointer: taps, drags and hovers fall straight through to whatever is behind.
//!   Use it to disable an interactive subtree without changing how it looks.
//! * [`absorb_pointer`]`(child)` — the child's subtree is also unhittable, but the
//!   barrier *absorbs* the event so nothing painted behind it (e.g. a lower `Stack`
//!   layer) receives it either. Use it as a lightweight modal barrier.
//!
//! Both are transparent to layout and paint — only hit testing changes. An `enabled`
//! toggle lets you switch the behavior on and off reactively.

use std::any::Any;

use pebbles_render::{HitBehavior, RenderObject, RenderPointerBarrier};

use pebbles_core::widget::{AnyWidget, RenderWidget, Widget};

/// A transparent wrapper that removes its child's subtree from hit testing (see the
/// module docs). Built by [`ignore_pointer`] / [`absorb_pointer`].
#[derive(Clone)]
pub struct PointerControl {
    behavior: HitBehavior,
    enabled: bool,
    child: Option<AnyWidget>,
}

/// Make `child` transparent to the pointer — events fall through to what's behind.
pub fn ignore_pointer(child: impl pebbles_core::IntoWidget) -> PointerControl {
    PointerControl { behavior: HitBehavior::Ignore, enabled: true, child: Some(child.into_widget()) }
}

/// Make `child` unhittable AND absorb the event so nothing behind it receives it.
pub fn absorb_pointer(child: impl pebbles_core::IntoWidget) -> PointerControl {
    PointerControl { behavior: HitBehavior::Absorb, enabled: true, child: Some(child.into_widget()) }
}

impl PointerControl {
    /// When `false`, the barrier is inert and the subtree hit-tests normally
    /// (default `true`). Lets you toggle ignore/absorb reactively.
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    fn effective(&self) -> HitBehavior {
        if self.enabled { self.behavior } else { HitBehavior::Normal }
    }
}

impl Widget for PointerControl {
    fn debug_name(&self) -> &'static str {
        "PointerControl"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> AnyWidget {
        Box::new(self.clone())
    }
    fn as_render(&self) -> Option<&dyn RenderWidget> {
        Some(self)
    }
    fn as_render_mut(&mut self) -> Option<&mut dyn RenderWidget> {
        Some(self)
    }
}

impl RenderWidget for PointerControl {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderPointerBarrier::new(self.effective()))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(b) = object.downcast_mut::<RenderPointerBarrier>() {
            b.behavior = self.effective();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
