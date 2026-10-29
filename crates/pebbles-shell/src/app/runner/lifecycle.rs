//! Window-system lifecycle decisions shared by the main and every secondary window (P12, G26,
//! 08 §5.4, §6.7): what a `Resized` / `ScaleFactorChanged` / `Occluded` / `Focused` event does to
//! a window's occlusion and live-resize state, when an idle loop must rebuild a lost GPU, and
//! what a repeated `resumed` does. Pure, so the hardware conditions (a drag onto a monitor with
//! another scale, a lock screen, a minimize, a driver loss while nothing redraws, a resume from
//! sleep) are simulated in unit tests by injecting the events.

use pebbles_foundation::Size;
use web_time::Instant;

use super::frame::{Occlusion, ResizeSync};

/// The subset of `WindowEvent` that changes a window's visibility or size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Lifecycle {
    /// `WindowEvent::Resized` (physical px; `0×0` = minimized on some platforms).
    Resized(u32, u32),
    /// `WindowEvent::ScaleFactorChanged` (the window moved to a monitor with another scale, or
    /// the user changed the scale). winit applies its suggested physical size (the same logical
    /// size at the new scale) unless the handler overrides it; Pebbles never does.
    ScaleFactorChanged,
    /// `WindowEvent::Occluded`.
    Occluded(bool),
    /// `WindowEvent::Focused`.
    Focused(bool),
}

/// What the caller does after [`apply`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Effects {
    /// Request a frame for the window.
    pub redraw: bool,
    /// Dismiss the window's anchored popover (its anchor moved).
    pub hide_overlay: bool,
}

/// Apply one lifecycle event to a window's state. `frame_id` is the window's latest packet id
/// (an `Occluded` acquire from an older packet is then stale, see [`Occlusion`]).
pub(super) fn apply(
    occlusion: &mut Occlusion,
    resize_sync: &mut ResizeSync,
    frame_id: u64,
    event: Lifecycle,
) -> Effects {
    match event {
        // 0×0 (minimized): nothing to configure or draw; the restore's non-zero `Resized`
        // (or `Occluded(false)` / `Focused(true)`) brings the window back.
        Lifecycle::Resized(w, h) if w == 0 || h == 0 => Effects { redraw: false, hide_overlay: true },
        // A new size or scale: visible now; the next packet carries the new physical size (the
        // surface is reconfigured from it, 08 §17.3 b), layout runs at the new logical size and
        // text is rasterized at the new device scale (glyph keys carry the device-pixel size).
        Lifecycle::Resized(..) | Lifecycle::ScaleFactorChanged => {
            occlusion.shown(frame_id);
            resize_sync.pending = true;
            Effects { redraw: true, hide_overlay: true }
        }
        Lifecycle::Occluded(occluded) => {
            Effects { redraw: occlusion.system(occluded, frame_id), hide_overlay: false }
        }
        Lifecycle::Focused(true) => {
            let was = occlusion.is_occluded();
            occlusion.shown(frame_id);
            Effects { redraw: was, hide_overlay: false }
        }
        Lifecycle::Focused(false) => Effects::default(),
    }
}

/// The logical size layout runs at: physical px / scale, `None` while 0-sized.
pub(super) fn logical_size(physical: (u32, u32), scale: f64) -> Option<Size> {
    if physical.0 == 0 || physical.1 == 0 || !(scale.is_finite() && scale > 0.0) {
        return None;
    }
    Some(Size::new(f64::from(physical.0) / scale, f64::from(physical.1) / scale))
}

/// What `about_to_wait` does about the GPU (08 §6.5–§6.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GpuWake {
    /// Healthy: nothing to do.
    Idle,
    /// Poisoned, but the §6.7 backoff has not elapsed: sleep until then.
    WakeAt(Instant),
    /// Poisoned and due: rebuild now, on this turn. Not via a redraw: a hidden (occluded,
    /// minimized, locked) window builds no frame, so a redraw-driven recovery never ran and the
    /// loop re-requested redraws forever (a busy spin on a dead device behind a lock screen).
    RecoverNow,
}

/// Decide [`GpuWake`] from the poison state, the next allowed retry and the time.
pub(super) fn gpu_wake(poisoned: bool, retry_at: Option<Instant>, now: Instant) -> GpuWake {
    match (poisoned, retry_at) {
        (false, _) => GpuWake::Idle,
        (true, Some(t)) if t > now => GpuWake::WakeAt(t),
        (true, _) => GpuWake::RecoverNow,
    }
}

/// What `ApplicationHandler::resumed` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Resume {
    /// First start (or the platform destroyed the window while suspended): create it.
    CreateWindow,
    /// Resumed with the window still there (system sleep, app switch on mobile): treat every
    /// window as visible and repaint it — a frame dropped while suspended must not stay stuck.
    RepaintAll,
}

pub(super) fn on_resumed(has_window: bool) -> Resume {
    if has_window { Resume::RepaintAll } else { Resume::CreateWindow }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[derive(Default)]
    struct Win {
        occlusion: Occlusion,
        resize_sync: ResizeSync,
        frame_id: u64,
        /// Physical size and scale as winit reports them.
        phys: (u32, u32),
        scale: f64,
    }

    impl Win {
        fn new(logical: (u32, u32), scale: f64) -> Self {
            let phys = ((f64::from(logical.0) * scale) as u32, (f64::from(logical.1) * scale) as u32);
            Self { phys, scale, ..Self::default() }
        }
        fn send(&mut self, e: Lifecycle) -> Effects {
            apply(&mut self.occlusion, &mut self.resize_sync, self.frame_id, e)
        }
        /// A frame: built only when visible and non-zero; clears the resize sync.
        fn frame(&mut self) -> Option<Size> {
            if self.occlusion.is_occluded() {
                return None;
            }
            let logical = logical_size(self.phys, self.scale)?;
            self.frame_id += 1;
            self.resize_sync.pending = false;
            Some(logical)
        }
        /// winit's `ScaleFactorChanged`: the new scale and its suggested size (same logical).
        fn move_to_monitor(&mut self, scale: f64) -> Effects {
            let logical = logical_size(self.phys, self.scale).unwrap();
            self.scale = scale;
            self.phys = ((logical.width * scale).round() as u32, (logical.height * scale).round() as u32);
            self.send(Lifecycle::ScaleFactorChanged)
        }
    }

    /// P12: a secondary window dragged across 100 % → 150 % → 100 % monitors repaints at each
    /// scale with an unchanged logical size, and its surface is resized (the next packet waits
    /// for the present at the new physical size).
    #[test]
    fn mixed_dpi_drag_keeps_logical_size_and_repaints() {
        let mut w = Win::new((800, 600), 1.0);
        assert_eq!(w.frame(), Some(Size::new(800.0, 600.0)));
        for scale in [1.5, 1.0, 1.75, 1.25] {
            let fx = w.move_to_monitor(scale);
            assert_eq!(fx, Effects { redraw: true, hide_overlay: true }, "scale {scale}");
            assert!(w.resize_sync.pending, "the surface is reconfigured at the new size");
            let logical = w.frame().expect("visible window repaints");
            assert!(
                (logical.width - 800.0).abs() < 1.0 && (logical.height - 600.0).abs() < 1.0,
                "{logical:?}"
            );
            assert!(!w.resize_sync.pending);
        }
        assert_eq!(w.phys, (1000, 750), "physical size follows the 125 % monitor");
    }

    /// A scale change while hidden (moved while minimized / on a locked screen) makes the window
    /// visible again — never a stuck frame at the old scale.
    #[test]
    fn scale_change_while_occluded_repaints() {
        let mut w = Win::new((640, 480), 2.0);
        assert!(!w.send(Lifecycle::Occluded(true)).redraw);
        assert_eq!(w.frame(), None);
        assert!(w.move_to_monitor(1.0).redraw);
        assert_eq!(w.frame(), Some(Size::new(640.0, 480.0)));
    }

    /// G26: minimize (0×0 and/or Occluded) then restore always ends with a frame.
    #[test]
    fn minimize_restore_never_sticks() {
        let mut w = Win::new((800, 600), 1.0);
        w.frame();
        // X11/Windows: minimize reports 0×0.
        let saved = w.phys;
        w.phys = (0, 0);
        assert!(!w.send(Lifecycle::Resized(0, 0)).redraw, "nothing to draw at 0×0");
        assert_eq!(w.frame(), None, "a 0-sized window builds no frame and configures nothing");
        w.phys = saved;
        assert!(w.send(Lifecycle::Resized(saved.0, saved.1)).redraw);
        assert!(w.frame().is_some());
        // Wayland/macOS: minimize reports Occluded; restore Occluded(false).
        w.send(Lifecycle::Occluded(true));
        assert_eq!(w.frame(), None);
        assert!(w.send(Lifecycle::Occluded(false)).redraw);
        assert!(w.frame().is_some());
    }

    /// G26: lock → unlock. The compositor occludes the window; a late `Occluded` acquire from a
    /// frame built before the unlock must not re-hide it; focus on unlock repaints.
    #[test]
    fn lock_unlock_never_sticks() {
        let t = Instant::now();
        let mut w = Win::new((800, 600), 1.0);
        w.frame();
        w.send(Lifecycle::Focused(false));
        w.send(Lifecycle::Occluded(true));
        assert_eq!(w.frame(), None);
        // Unlock: some compositors only send focus.
        assert!(w.send(Lifecycle::Focused(true)).redraw);
        assert!(!w.occlusion.acquire_occluded(w.frame_id, t), "stale occluded acquire ignored");
        assert!(w.frame().is_some());
        // A focus change on a visible window costs no frame (idle stays idle).
        assert!(!w.send(Lifecycle::Focused(true)).redraw);
    }

    /// Every event sequence ends either with a frame or in a state a later visibility event
    /// clears: after any prefix, `Occluded(false)` always yields a frame.
    #[test]
    fn any_sequence_recovers_on_visibility() {
        let events = [
            Lifecycle::Resized(0, 0),
            Lifecycle::Resized(300, 200),
            Lifecycle::ScaleFactorChanged,
            Lifecycle::Occluded(true),
            Lifecycle::Occluded(false),
            Lifecycle::Focused(true),
            Lifecycle::Focused(false),
        ];
        for a in events {
            for b in events {
                for c in events {
                    let mut w = Win::new((300, 200), 1.0);
                    for e in [a, b, c] {
                        if let Lifecycle::Resized(x, y) = e {
                            w.phys = (x, y);
                        }
                        w.send(e);
                    }
                    w.phys = (300, 200);
                    w.send(Lifecycle::Occluded(false));
                    assert!(w.frame().is_some(), "{a:?} {b:?} {c:?}");
                }
            }
        }
    }

    #[test]
    fn logical_size_guards() {
        assert_eq!(logical_size((0, 10), 1.0), None);
        assert_eq!(logical_size((10, 10), 0.0), None);
        assert_eq!(logical_size((10, 10), f64::NAN), None);
        assert_eq!(logical_size((300, 150), 1.5), Some(Size::new(200.0, 100.0)));
    }

    /// P11/G22: an idle loop with a lost device rebuilds on the turn the loss is seen (no
    /// redraw needed, so a hidden window recovers too), then waits out the §6.7 backoff
    /// between failed attempts, then rebuilds again.
    #[test]
    fn idle_lost_device_recovers_without_a_frame() {
        let t0 = Instant::now();
        assert_eq!(gpu_wake(false, None, t0), GpuWake::Idle);
        assert_eq!(gpu_wake(true, None, t0), GpuWake::RecoverNow, "first loss: rebuild now");
        let retry = t0 + Duration::from_secs(1);
        assert_eq!(gpu_wake(true, Some(retry), t0), GpuWake::WakeAt(retry), "failed rebuild: backoff");
        assert_eq!(gpu_wake(true, Some(retry), retry), GpuWake::RecoverNow);
        assert_eq!(gpu_wake(false, Some(retry), t0), GpuWake::Idle, "healthy after the rebuild");
        // The first rebuild is attempted well within the 3 s budget (immediately), and every
        // retry within the 10 s ceiling of the backoff.
        for failures in 0..20 {
            assert!(super::super::render::reset_backoff(failures) <= Duration::from_secs(10));
        }
    }

    /// Suspend → resume with the window alive repaints every window; a first `resumed` creates.
    #[test]
    fn resume_repaints() {
        assert_eq!(on_resumed(false), Resume::CreateWindow);
        assert_eq!(on_resumed(true), Resume::RepaintAll);
        // Resume after a suspend that hid the window: RepaintAll marks it shown.
        let mut w = Win::new((400, 300), 1.0);
        w.send(Lifecycle::Occluded(true));
        if on_resumed(true) == Resume::RepaintAll {
            w.occlusion.shown(w.frame_id);
        }
        assert!(w.frame().is_some());
    }
}
