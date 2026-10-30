//! Pan slop — the gesture-arena rule that a press is not yet a drag.
//!
//! Flutter's `PanGestureRecognizer` holds a pan as a *candidate* on pointer-down
//! and only accepts it once the pointer travels past `kPanSlop`; below that the
//! release resolves as a tap, past it the tap is rejected. [`PanGesture`] is that
//! rule, shared by the shell runner (every window), the input storm and the
//! `pebbles-testing` harness so all of them deliver the same event order:
//!
//! * press — remember the pan target and the press point (no `pan_start` yet);
//! * move past the slop — `pan_start` at the press point, then `pan_update` to
//!   the current point (the accumulated delta is not lost); the caller cancels
//!   the pending tap;
//! * later moves — `pan_update`;
//! * release — `pan_end` only if the pan started; otherwise the release is a tap.
//!
//! Widgets that need the press point itself (a caret, a slider's jump-to-press)
//! read it from `on_pointer_down`, which still fires on the press.

use pebbles_foundation::Offset;

use crate::Ui;

/// Movement (logical px) before a mouse press becomes a drag. Matches the
/// local guard `Draggable` used, and desktop toolkits' drag threshold.
pub const MOUSE_PAN_SLOP: f64 = 8.0;
/// Movement (logical px) before a touch press becomes a drag — Flutter `kPanSlop`.
pub const TOUCH_PAN_SLOP: f64 = 18.0;

/// What a pointer move did to the gesture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PanMove {
    /// The pointer just travelled past the slop: the press is now a drag, so the
    /// pending tap (and anything else that was waiting on "no movement") must be
    /// cancelled by the caller.
    pub crossed_slop: bool,
    /// A pan listener handled the move (`pan_start` / `pan_update`).
    pub handled: bool,
}

/// One primary-pointer press, from down to up (see the module docs).
#[derive(Clone, Copy, Debug, Default)]
pub struct PanGesture {
    /// The pan-hungry widget under the press, if any.
    target: Option<u64>,
    /// Where the press landed; `None` when no press is in flight.
    origin: Option<Offset>,
    slop: f64,
    /// Whether the pointer has travelled past the slop during this press.
    dragging: bool,
}

impl PanGesture {
    /// A pointer went down at `at`; `target` is the pan candidate (from
    /// [`Ui::pan_target_at`], or `None` when something else claimed the drag).
    pub fn press(&mut self, target: Option<u64>, at: Offset, slop: f64) {
        *self = Self { target, origin: Some(at), slop, dragging: false };
    }

    /// The pan target, but only once the pan has actually started.
    pub fn active_target(&self) -> Option<u64> {
        if self.dragging { self.target } else { None }
    }

    /// Whether the pointer has moved past the slop during the current press.
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Whether a press is in flight.
    pub fn is_pressed(&self) -> bool {
        self.origin.is_some()
    }

    /// The pointer moved to `to` while held.
    pub fn moved(&mut self, ui: &mut Ui, to: Offset) -> PanMove {
        let Some(origin) = self.origin else { return PanMove::default() };
        if self.dragging {
            let handled = self.target.is_some_and(|t| ui.dispatch_pan_update(t, to));
            return PanMove { crossed_slop: false, handled };
        }
        let (dx, dy) = (to.x - origin.x, to.y - origin.y);
        if (dx * dx + dy * dy).sqrt() <= self.slop {
            return PanMove::default();
        }
        self.dragging = true;
        let handled = self.target.is_some_and(|t| {
            let a = ui.dispatch_pan_start(t, origin);
            let b = ui.dispatch_pan_update(t, to);
            a | b
        });
        PanMove { crossed_slop: true, handled }
    }

    /// The pointer was released at `at`: `pan_end` if the pan started. Returns
    /// whether the press had become a drag (the release is then not a tap).
    pub fn release(&mut self, ui: &mut Ui, at: Offset) -> bool {
        let dragging = self.dragging;
        if let Some(t) = self.active_target() {
            ui.dispatch_pan_end(t, at);
        }
        *self = Self::default();
        dragging
    }

    /// Forget the press without delivering anything (window lost, etc.).
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wobble_inside_the_slop_is_not_a_drag() {
        let mut ui = Ui::new();
        let mut g = PanGesture::default();
        g.press(None, Offset::new(10.0, 10.0), MOUSE_PAN_SLOP);
        assert!(g.is_pressed());
        assert_eq!(g.moved(&mut ui, Offset::new(12.0, 11.0)), PanMove::default());
        assert_eq!(
            g.moved(&mut ui, Offset::new(10.0, 18.0)),
            PanMove::default(),
            "exactly the slop is still a tap"
        );
        assert!(!g.is_dragging());
        assert!(!g.release(&mut ui, Offset::new(12.0, 11.0)), "the release is a tap");
        assert!(!g.is_pressed());
    }

    #[test]
    fn crossing_the_slop_once_turns_the_press_into_a_drag() {
        let mut ui = Ui::new();
        let mut g = PanGesture::default();
        g.press(None, Offset::new(0.0, 0.0), MOUSE_PAN_SLOP);
        assert!(g.moved(&mut ui, Offset::new(9.0, 0.0)).crossed_slop);
        assert!(!g.moved(&mut ui, Offset::new(0.0, 0.0)).crossed_slop, "crossed only once");
        assert!(g.is_dragging());
        assert!(g.release(&mut ui, Offset::ZERO), "a drag's release is not a tap");
    }

    #[test]
    fn touch_uses_flutters_wider_pan_slop() {
        let mut ui = Ui::new();
        let mut g = PanGesture::default();
        g.press(None, Offset::ZERO, TOUCH_PAN_SLOP);
        assert!(!g.moved(&mut ui, Offset::new(12.0, 0.0)).crossed_slop);
        assert!(g.moved(&mut ui, Offset::new(19.0, 0.0)).crossed_slop);
    }

    #[test]
    fn moves_without_a_press_do_nothing() {
        let mut ui = Ui::new();
        let mut g = PanGesture::default();
        assert_eq!(g.moved(&mut ui, Offset::new(100.0, 0.0)), PanMove::default());
    }
}
