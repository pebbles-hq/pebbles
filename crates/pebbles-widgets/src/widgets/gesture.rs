//! [`GestureDetector`] — makes its child interactive with the full pointer event
//! set: tap, double-tap, secondary (right) click, long-press, pointer down/up, and
//! hover enter/exit. Backs [`pebbles_render::RenderPointerListener`].
//!
//! Each `on_*` builder can be called more than once; all handlers fire. This lets a
//! higher-level widget attach its own internal handler *and* the developer's handler
//! to the same event.

use pebbles_core::IntoCallback;
use pebbles_render::{Cursor, RenderObject, RenderPointerListener, TapCallback};

use pebbles_core::context::Callback;
use pebbles_core::widget::{AnyWidget, RenderWidget};

fn erase(cbs: &[Callback]) -> Vec<TapCallback> {
    cbs.iter().cloned().map(|c| Box::new(c) as TapCallback).collect()
}

/// Wraps a child and reports pointer interactions. Configure fluently:
///
/// ```ignore
/// let count = create_signal(0);
/// let hovered = create_signal(false);
/// GestureDetector::new(my_child)
///     .on_tap(move || count.update(|c| *c += 1))
///     .on_hover_enter(move || hovered.set(true))
///     .on_hover_exit(move || hovered.set(false))
/// ```
#[derive(Clone, Default)]
pub struct GestureDetector {
    on_tap: Vec<Callback>,
    on_tap_cancel: Vec<Callback>,
    on_double_tap: Vec<Callback>,
    on_secondary_tap: Vec<Callback>,
    on_secondary_tap_down: Vec<Callback>,
    on_secondary_tap_up: Vec<Callback>,
    on_secondary_tap_cancel: Vec<Callback>,
    on_long_press: Vec<Callback>,
    on_long_press_down: Vec<Callback>,
    on_long_press_start: Vec<Callback>,
    on_long_press_move: Vec<Callback>,
    on_long_press_up: Vec<Callback>,
    on_long_press_end: Vec<Callback>,
    on_long_press_cancel: Vec<Callback>,
    on_tertiary_tap_down: Vec<Callback>,
    on_tertiary_tap_up: Vec<Callback>,
    on_tertiary_tap_cancel: Vec<Callback>,
    on_pointer_down: Vec<Callback>,
    on_pointer_up: Vec<Callback>,
    on_enter: Vec<Callback>,
    on_exit: Vec<Callback>,
    on_pan_start: Vec<Callback>,
    on_pan_update: Vec<Callback>,
    on_pan_end: Vec<Callback>,
    cursor: Option<Cursor>,
    child: Option<AnyWidget>,
}

impl GestureDetector {
    /// Wrap `child` with no handlers yet — chain the `on_*` builders.
    pub fn new(child: impl pebbles_core::IntoWidget) -> Self {
        GestureDetector { child: Some(child.into_widget()), ..Default::default() }
    }

    pub fn on_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_tap.push(cb.into_callback());
        self
    }
    pub fn on_double_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_double_tap.push(cb.into_callback());
        self
    }
    pub fn on_tap_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_tap_cancel.push(cb.into_callback());
        self
    }
    pub fn on_secondary_tap(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap.push(cb.into_callback());
        self
    }
    pub fn on_secondary_tap_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap_down.push(cb.into_callback());
        self
    }
    pub fn on_secondary_tap_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap_up.push(cb.into_callback());
        self
    }
    pub fn on_secondary_tap_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_secondary_tap_cancel.push(cb.into_callback());
        self
    }
    pub fn on_long_press(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press.push(cb.into_callback());
        self
    }
    pub fn on_long_press_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_down.push(cb.into_callback());
        self
    }
    pub fn on_long_press_start(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_start.push(cb.into_callback());
        self
    }
    pub fn on_long_press_move(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_move.push(cb.into_callback());
        self
    }
    pub fn on_long_press_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_up.push(cb.into_callback());
        self
    }
    pub fn on_long_press_end(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_end.push(cb.into_callback());
        self
    }
    pub fn on_long_press_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_long_press_cancel.push(cb.into_callback());
        self
    }
    pub fn on_tertiary_tap_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_tertiary_tap_down.push(cb.into_callback());
        self
    }
    pub fn on_tertiary_tap_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_tertiary_tap_up.push(cb.into_callback());
        self
    }
    pub fn on_tertiary_tap_cancel(mut self, cb: impl IntoCallback) -> Self {
        self.on_tertiary_tap_cancel.push(cb.into_callback());
        self
    }
    pub fn on_pointer_down(mut self, cb: impl IntoCallback) -> Self {
        self.on_pointer_down.push(cb.into_callback());
        self
    }
    pub fn on_pointer_up(mut self, cb: impl IntoCallback) -> Self {
        self.on_pointer_up.push(cb.into_callback());
        self
    }
    pub fn on_hover_enter(mut self, cb: impl IntoCallback) -> Self {
        self.on_enter.push(cb.into_callback());
        self
    }
    pub fn on_hover_exit(mut self, cb: impl IntoCallback) -> Self {
        self.on_exit.push(cb.into_callback());
        self
    }
    /// A drag began on this widget (primary press). Use `action_event` to read the
    /// start position.
    pub fn on_pan_start(mut self, cb: impl IntoCallback) -> Self {
        self.on_pan_start.push(cb.into_callback());
        self
    }
    /// The pointer moved during an active drag. `action_event`'s `position` is in
    /// this widget's local space — the basis for sliders, resizers and scrollbars.
    pub fn on_pan_update(mut self, cb: impl IntoCallback) -> Self {
        self.on_pan_update.push(cb.into_callback());
        self
    }
    /// The drag ended (primary released).
    pub fn on_pan_end(mut self, cb: impl IntoCallback) -> Self {
        self.on_pan_end.push(cb.into_callback());
        self
    }
    /// The cursor to show while hovering this widget.
    pub fn cursor(mut self, cursor: Cursor) -> Self {
        self.cursor = Some(cursor);
        self
    }

    fn make_listener(&self) -> RenderPointerListener {
        RenderPointerListener {
            on_tap: erase(&self.on_tap),
            on_tap_cancel: erase(&self.on_tap_cancel),
            on_double_tap: erase(&self.on_double_tap),
            on_secondary_tap: erase(&self.on_secondary_tap),
            on_secondary_tap_down: erase(&self.on_secondary_tap_down),
            on_secondary_tap_up: erase(&self.on_secondary_tap_up),
            on_secondary_tap_cancel: erase(&self.on_secondary_tap_cancel),
            on_long_press: erase(&self.on_long_press),
            on_long_press_down: erase(&self.on_long_press_down),
            on_long_press_start: erase(&self.on_long_press_start),
            on_long_press_move: erase(&self.on_long_press_move),
            on_long_press_up: erase(&self.on_long_press_up),
            on_long_press_end: erase(&self.on_long_press_end),
            on_long_press_cancel: erase(&self.on_long_press_cancel),
            on_tertiary_tap_down: erase(&self.on_tertiary_tap_down),
            on_tertiary_tap_up: erase(&self.on_tertiary_tap_up),
            on_tertiary_tap_cancel: erase(&self.on_tertiary_tap_cancel),
            on_pointer_down: erase(&self.on_pointer_down),
            on_pointer_up: erase(&self.on_pointer_up),
            on_enter: erase(&self.on_enter),
            on_exit: erase(&self.on_exit),
            on_pan_start: erase(&self.on_pan_start),
            on_pan_update: erase(&self.on_pan_update),
            on_pan_end: erase(&self.on_pan_end),
            // Default to a pointer cursor when this detector is clickable.
            cursor: self.cursor.or_else(|| (!self.on_tap.is_empty()).then_some(Cursor::Pointer)),
        }
    }
}

pebbles_core::render_widget!(GestureDetector);

impl RenderWidget for GestureDetector {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(self.make_listener())
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(listener) = object.downcast_mut::<RenderPointerListener>() {
            *listener = self.make_listener();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
