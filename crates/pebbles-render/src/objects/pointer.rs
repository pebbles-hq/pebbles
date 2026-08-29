//! [`RenderPointerListener`] — a transparent single-child box that records pointer
//! callbacks (tap, double-tap, secondary/right-click, and hover enter/exit). It
//! performs no dispatch itself: the shell hit-tests the tree and the widget layer
//! interprets the (type-erased) callbacks. Keeping callbacks erased lets the render
//! crate stay below the widget crate in the dependency graph.

use std::any::Any;

use pebbles_foundation::{Offset, Size};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// Which pointer button triggered an interaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
}

/// Details delivered to a pointer event handler (Flutter's `TapDownDetails` etc.).
#[derive(Clone, Copy, Debug)]
pub struct PointerEvent {
    /// Position within the receiving widget's local coordinate space.
    pub position: Offset,
    /// Position in window (global) coordinates.
    pub global: Offset,
    /// Which button was involved.
    pub button: PointerButton,
}

/// The mouse cursor a widget requests while hovered. The shell maps these to the
/// platform cursor (kept here so the render crate has no windowing dependency).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Default,
    Pointer,
    Text,
    Grab,
    Grabbing,
    ColResize,
    RowResize,
    NotAllowed,
}

/// A callback, type-erased so this crate need not know the widget layer's
/// `Callback` type. The widget layer downcasts it back on dispatch.
pub type TapCallback = Box<dyn Any>;

/// Records pointer callbacks; sizes to its single child. Each event may carry
/// **several** callbacks (so a widget can attach its own internal handler *and* the
/// developer's handler to the same event); dispatch fires all of them.
#[derive(Default)]
pub struct RenderPointerListener {
    /// Primary-button tap (press + release inside).
    pub on_tap: Vec<TapCallback>,
    /// Second primary tap within the double-tap interval.
    pub on_double_tap: Vec<TapCallback>,
    /// Primary press began but ended without a tap (released off / dragged away).
    pub on_tap_cancel: Vec<TapCallback>,
    /// Secondary-button (right-click) tap.
    pub on_secondary_tap: Vec<TapCallback>,
    /// Secondary button pressed down.
    pub on_secondary_tap_down: Vec<TapCallback>,
    /// Secondary button released.
    pub on_secondary_tap_up: Vec<TapCallback>,
    /// Secondary press cancelled.
    pub on_secondary_tap_cancel: Vec<TapCallback>,
    /// Primary button pressed down over this box.
    pub on_pointer_down: Vec<TapCallback>,
    /// Primary button released over this box.
    pub on_pointer_up: Vec<TapCallback>,
    /// Primary button held down beyond the long-press interval.
    pub on_long_press: Vec<TapCallback>,
    /// Primary pressed down — a long press may begin.
    pub on_long_press_down: Vec<TapCallback>,
    /// Long press recognized (with details).
    pub on_long_press_start: Vec<TapCallback>,
    /// Pointer moved during the long press.
    pub on_long_press_move: Vec<TapCallback>,
    /// Pointer released after the long press.
    pub on_long_press_up: Vec<TapCallback>,
    /// Long press ended (with details).
    pub on_long_press_end: Vec<TapCallback>,
    /// A pending long press was cancelled before it began.
    pub on_long_press_cancel: Vec<TapCallback>,
    /// Tertiary (middle) button pressed down.
    pub on_tertiary_tap_down: Vec<TapCallback>,
    /// Tertiary (middle) button released.
    pub on_tertiary_tap_up: Vec<TapCallback>,
    /// Tertiary press cancelled.
    pub on_tertiary_tap_cancel: Vec<TapCallback>,
    /// The pointer entered this box's bounds.
    pub on_enter: Vec<TapCallback>,
    /// The pointer left this box's bounds.
    pub on_exit: Vec<TapCallback>,
    /// Primary press began a drag over this box (fires with the down position).
    pub on_pan_start: Vec<TapCallback>,
    /// Pointer moved while the drag it started is active (fires with the position).
    pub on_pan_update: Vec<TapCallback>,
    /// The drag ended (primary released).
    pub on_pan_end: Vec<TapCallback>,
    /// The cursor to show while hovered.
    pub cursor: Option<Cursor>,
}

impl RenderPointerListener {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether this listener participates in hover tracking.
    pub fn wants_hover(&self) -> bool {
        !self.on_enter.is_empty() || !self.on_exit.is_empty()
    }

    /// Whether this listener participates in drag/pan tracking.
    pub fn wants_pan(&self) -> bool {
        !self.on_pan_start.is_empty()
            || !self.on_pan_update.is_empty()
            || !self.on_pan_end.is_empty()
    }
}

impl RenderObject for RenderPointerListener {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        match cx.children().first().copied() {
            Some(child) => {
                let size = cx.layout_child(child, constraints);
                cx.set_child_offset(child, Offset::ZERO);
                size
            }
            None => constraints.constrain(Size::ZERO),
        }
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        if let Some(child) = cx.children().first().copied() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderPointerListener"
    }
}
