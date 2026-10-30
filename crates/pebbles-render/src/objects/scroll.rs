//! [`RenderScroll`] — a single-child viewport that clips its content, offsets it
//! along one axis, and paints a customizable scrollbar. The offset is mutated
//! imperatively (wheel events + scrollbar drag, routed from the shell) and clamped
//! to the content extent during layout.

use std::cell::RefCell;
use std::rc::Rc;

use kurbo::{Affine, RoundedRect};
use pebbles_foundation::{Axis, Color, Offset, Rect, Size};
use peniko::{Fill, Mix};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// A snapshot of a scroll view's position at the instant a [`ScrollNotification`]
/// fires — Flutter's `ScrollMetrics`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollMetrics {
    /// The scroll axis.
    pub axis: Axis,
    /// The current offset in px (Flutter's `pixels`).
    pub pixels: f64,
    /// The minimum offset — always `0` here (Flutter's `minScrollExtent`).
    pub min: f64,
    /// The maximum scrollable offset, `content − viewport` (Flutter's `maxScrollExtent`).
    pub max: f64,
    /// The viewport extent along the axis (Flutter's `viewportDimension`).
    pub viewport: f64,
}

impl ScrollMetrics {
    /// Px already scrolled past the leading edge (Flutter's `extentBefore`).
    pub fn extent_before(&self) -> f64 {
        (self.pixels - self.min).max(0.0)
    }
    /// Px still scrollable past the trailing edge (Flutter's `extentAfter`).
    pub fn extent_after(&self) -> f64 {
        (self.max - self.pixels).max(0.0)
    }
    /// Whether the offset is pinned at the start.
    pub fn at_start(&self) -> bool {
        self.pixels <= self.min + 0.5
    }
    /// Whether the offset is pinned at the end.
    pub fn at_end(&self) -> bool {
        self.pixels >= self.max - 0.5
    }
    /// Progress through the scrollable range, `0.0..=1.0` (0 when not scrollable).
    pub fn fraction(&self) -> f64 {
        if self.max <= self.min {
            0.0
        } else {
            ((self.pixels - self.min) / (self.max - self.min)).clamp(0.0, 1.0)
        }
    }
}

/// What happened to a scroll view — Pebbles' flattened `ScrollNotification` kind
/// (Flutter dispatches these as distinct notification subclasses).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollEvent {
    /// A scroll activity began (drag, wheel, fling, or programmatic move).
    Start,
    /// The offset moved by `delta` px this frame.
    Update {
        /// Signed change in `pixels` (positive = toward the end).
        delta: f64,
    },
    /// The scroll activity settled.
    End,
    /// A drag pulled `overscroll` px past an edge (overscroll physics only).
    Overscroll {
        /// Px past the edge (positive past the end, negative past the start).
        overscroll: f64,
    },
}

/// A scroll notification: the [`metrics`](Self::metrics) at the moment plus what
/// [`event`](Self::event) fired. Delivered to a scroll view's `on_scroll` callback.
#[derive(Clone, Copy, Debug)]
pub struct ScrollNotification {
    pub metrics: ScrollMetrics,
    pub event: ScrollEvent,
}

/// When the scrollbar is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScrollbarPolicy {
    /// Only when the content overflows (the default).
    #[default]
    Auto,
    /// Always draw the track (thumb still only when scrollable).
    Always,
    /// Never draw a scrollbar.
    Hidden,
}

/// Customizable scrollbar appearance.
#[derive(Clone, Copy, Debug)]
pub struct ScrollbarStyle {
    pub policy: ScrollbarPolicy,
    /// Painted thickness of the bar.
    pub thickness: f64,
    /// Minimum thumb length so it stays grabbable on very long content.
    pub min_thumb: f64,
    /// Inset from the viewport edges.
    pub margin: f64,
    pub thumb_color: Color,
    /// The thumb while the pointer is over the bar (VS Code `scrollbarSlider.hoverBackground`).
    pub thumb_hover_color: Color,
    /// The thumb while it is being dragged (VS Code `scrollbarSlider.activeBackground`).
    pub thumb_active_color: Color,
    pub track_color: Color,
    pub radius: f64,
}

impl ScrollbarStyle {
    /// The strip the bar occupies across the scroll axis (thickness plus both margins).
    /// It is both what the bar paints over and what it claims for pointer presses, so
    /// content kept clear of it never loses a click to the bar.
    pub fn gutter(&self) -> f64 {
        self.thickness + 2.0 * self.margin
    }

    /// The bar geometry along the scroll axis for a `viewport` showing `content`
    /// scrolled to `offset`: `(track_start, track_len, thumb_start, thumb_len)`. The
    /// thumb lives inside the track (inset by `margin`), its length is
    /// `track × viewport / content` (never below `min_thumb`), and its start maps
    /// `offset / (content − viewport)` onto the track's free travel.
    pub fn geometry(&self, viewport: f64, content: f64, offset: f64) -> (f64, f64, f64, f64) {
        let track_start = self.margin;
        let track_len = (viewport - 2.0 * self.margin).max(0.0);
        let ratio = if content > 0.0 { (viewport / content).clamp(0.0, 1.0) } else { 1.0 };
        let thumb_len = (track_len * ratio).max(self.min_thumb.min(track_len)).min(track_len);
        let max = (content - viewport).max(0.0);
        let travel = (track_len - thumb_len).max(0.0);
        let frac = if max > 0.0 { (offset / max).clamp(0.0, 1.0) } else { 0.0 };
        (track_start, track_len, track_start + travel * frac, thumb_len)
    }

    /// The scroll offset that puts the thumb's start at `thumb_start` (inverse of
    /// [`geometry`](Self::geometry)), clamped to `[0, content − viewport]`.
    pub fn offset_for_thumb(&self, viewport: f64, content: f64, thumb_start: f64) -> f64 {
        let (track_start, track_len, _, thumb_len) = self.geometry(viewport, content, 0.0);
        let travel = track_len - thumb_len;
        let max = (content - viewport).max(0.0);
        if travel <= 0.0 {
            return 0.0;
        }
        (((thumb_start - track_start) / travel).clamp(0.0, 1.0)) * max
    }
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        ScrollbarStyle {
            policy: ScrollbarPolicy::Auto,
            thickness: 8.0,
            min_thumb: 28.0,
            margin: 2.0,
            thumb_color: Color::from_rgba8(0x94, 0xa3, 0xb8, 0xcc), // slate-400
            thumb_hover_color: Color::from_rgba8(0x94, 0xa3, 0xb8, 0xe6),
            thumb_active_color: Color::from_rgba8(0x94, 0xa3, 0xb8, 0xff),
            track_color: Color::from_rgba8(0x94, 0xa3, 0xb8, 0x1f),
            radius: 999.0,
        }
    }
}

/// The physics of a scroll view: how the offset eases toward its target, how a
/// fling decays, and whether drags may pull past the edges (rubber-banding).
///
/// The model is first-order and closed-form (browser / Flutter style), so it is
/// dt-exact and cannot oscillate: wheel/keyboard/settle use exponential smoothing
/// (`offset += (target−offset)·(1−e^(−√stiffness·dt))`) and a fling uses analytic
/// friction (`v(t)=v0·e^(−r·t)`, `r = −60·ln(1−friction)`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollPhysics {
    /// Smoothing rate for the offset→target ease: the per-second rate is
    /// `√stiffness` (never overshoots — first-order lag, not a spring).
    pub stiffness: f64,
    /// Fling friction: velocity decays as `e^(−r·t)` with `r = −60·ln(1−friction)`
    /// per second (so a value keeps the same feel it had as a per-frame-at-60 Hz
    /// decay, but is now frame-rate independent).
    pub friction: f64,
    /// Whether content drags may pull past `[0, max]` with resistance (excess/3),
    /// springing back on release.
    pub overscroll: bool,
}

impl Default for ScrollPhysics {
    fn default() -> Self {
        ScrollPhysics { stiffness: 240.0, friction: 0.015, overscroll: false }
    }
}

/// The pull-to-refresh trigger attached to a scroll view (A5). The drag driver
/// calls the callbacks as the pointer pulls past the top edge and releases.
#[derive(Clone)]
pub struct RefreshState {
    /// Pull distance (negative offset) that arms the indicator.
    pub threshold: f64,
    /// Fired once when the pull crosses `threshold` (the spinner rotates in).
    pub on_arm: Option<Rc<dyn Fn()>>,
    /// Fired once when an ARMED pull is released — the app's `on_refresh`.
    pub on_arm_release: Option<Rc<dyn Fn()>>,
    /// Fired once when a pull is released without ever arming (clears the
    /// half-rotated spinner state).
    pub on_release: Option<Rc<dyn Fn()>>,
    /// Whether the arm threshold has been crossed in the current drag.
    pub fired_arm: bool,
}

impl RefreshState {
    pub fn new(threshold: f64) -> Self {
        RefreshState {
            threshold: threshold.max(1.0),
            on_arm: None,
            on_arm_release: None,
            on_release: None,
            fired_arm: false,
        }
    }
}

/// A handle to command a [`RenderScroll`] imperatively and read its live metrics — the
/// scroll equivalent of a text `Editor` handle.
///
/// Clone it, hand one copy to `scroll_view(...).controller(ctrl)`, and keep the other to
/// drive it: [`ensure_visible`](Self::ensure_visible) brings a span into view (the basis
/// for caret autoscroll in the code editor) and [`scroll_to`](Self::scroll_to) jumps to an
/// absolute offset. A programmatic move snaps directly (no spring), so the target is in
/// view on the very next frame. Reads ([`offset`](Self::offset) / [`max`](Self::max) /
/// [`viewport`](Self::viewport)) reflect the last laid-out frame.
#[derive(Clone, Default)]
pub struct ScrollHandle(Rc<RefCell<ScrollHandleState>>);

#[derive(Default)]
struct ScrollHandleState {
    /// A requested absolute target offset, consumed by the next layout.
    pending: Option<f64>,
    offset: f64,
    max: f64,
    viewport: f64,
}

impl ScrollHandle {
    /// A fresh, unattached controller.
    pub fn new() -> Self {
        Self::default()
    }
    /// The current displayed offset (from the last laid-out frame).
    pub fn offset(&self) -> f64 {
        self.0.borrow().offset
    }
    /// The maximum scrollable offset (`content − viewport`) from the last frame.
    pub fn max(&self) -> f64 {
        self.0.borrow().max
    }
    /// The viewport extent along the scroll axis from the last frame.
    pub fn viewport(&self) -> f64 {
        self.0.borrow().viewport
    }
    /// Jump the offset to `offset` (clamped into range at the next layout).
    pub fn scroll_to(&self, offset: f64) {
        self.0.borrow_mut().pending = Some(offset);
    }
    /// Scroll the minimum amount so the span `start..end` (content coordinates along the
    /// scroll axis) is fully visible, keeping `margin` px of breathing room. No-op when it
    /// already fits.
    pub fn ensure_visible(&self, start: f64, end: f64, margin: f64) {
        let (off, vp) = {
            let s = self.0.borrow();
            (s.offset, s.viewport)
        };
        if vp <= 0.0 {
            // Not laid out yet — just aim the start near the top.
            self.scroll_to((start - margin).max(0.0));
            return;
        }
        let target = if start < off + margin {
            (start - margin).max(0.0)
        } else if end > off + vp - margin {
            end - vp + margin
        } else {
            return;
        };
        self.0.borrow_mut().pending = Some(target);
    }
    /// Publish this frame's metrics and take any pending target (called by `RenderScroll`
    /// during layout). Returns the requested offset, if one was queued.
    fn sync(&self, offset: f64, max: f64, viewport: f64) -> Option<f64> {
        let mut s = self.0.borrow_mut();
        s.offset = offset;
        s.max = max;
        s.viewport = viewport;
        s.pending.take()
    }
}

/// A scrollable viewport.
pub struct RenderScroll {
    pub axis: Axis,
    /// Displayed scroll offset (eased toward `target`, or driven by a fling).
    pub offset: f64,
    /// Where the offset is heading — wheel/keyboard/programmatic moves set this;
    /// first-order smoothing eases `offset` toward it for a browser-style glide.
    pub target: f64,
    /// The offset's current speed (px/s), reported for consumers — the fling speed
    /// while a fling is active, else the smoothing's effective rate.
    pub velocity: f64,
    /// Decaying fling speed (px/s) while [`flinging`](Self::flinging).
    fling_velocity: f64,
    /// Whether a fling is in progress (analytic friction drives `offset` directly).
    flinging: bool,
    /// Snap increment: after settling, the target rounds to a multiple of this
    /// (0 = no snapping).
    pub snap: f64,
    /// Maximum scrollable offset, computed each layout as `content - viewport`.
    pub max_offset: f64,
    /// Viewport extent along `axis`, computed each layout.
    pub viewport_extent: f64,
    pub scrollbar: ScrollbarStyle,
    /// Opt-in pan-to-scroll: when set, a pointer drag over this viewport moves the
    /// content 1:1 (the shell arbitrates against draggable descendants).
    pub drag_scroll: bool,
    /// Force the child to be at least the viewport size along the scroll axis, so it
    /// fills the viewport when smaller and overflows (scrolls) when larger. Default
    /// `false` (the child sizes to its content). Used by the data `Table` so a
    /// content-sized grid still fills 100% of the width.
    pub fill_viewport: bool,
    /// Spring stiffness, fling friction and overscroll behavior.
    pub physics: ScrollPhysics,
    /// Pull-to-refresh trigger, when the owning widget installed one.
    pub refresh: Option<RefreshState>,
    /// True while a content drag is in progress (offset follows the pointer).
    dragging: bool,
    /// Pointer position along `axis` at the last drag update (delta source).
    drag_last: f64,
    /// The UNBANDED offset during a drag — the rubber-band transform applies on
    /// top of this, so re-applying it to an already-banded offset can't compound.
    drag_real: f64,
    /// Recent (time, offset) samples for the fling velocity estimate.
    fling_samples: [(f64, f64); 4],
    fling_samples_len: usize,
    fling_cursor: usize,
    /// Scroll-notification sink (Flutter's `ScrollNotification` / `NotificationListener`).
    /// Fired on Start/Update/End/Overscroll as the offset moves.
    pub on_scroll: Option<Rc<dyn Fn(ScrollNotification)>>,
    /// Imperative handle for programmatic scrolling + metric readback (e.g. caret
    /// autoscroll). The owning widget installs it via `.controller(..)`.
    pub controller: Option<ScrollHandle>,
    /// True while a scroll activity is live — gates one Start and one End per activity.
    was_moving: bool,
    /// While the scrollbar is dragged: where along the thumb it was grabbed (px from
    /// the thumb's start), so the thumb keeps that point under the pointer.
    bar_grab: Option<f64>,
    /// Whether the pointer is over the scrollbar strip (hover state).
    bar_hover: bool,
}

impl RenderScroll {
    pub fn new(axis: Axis) -> Self {
        RenderScroll {
            axis,
            offset: 0.0,
            target: 0.0,
            velocity: 0.0,
            snap: 0.0,
            max_offset: 0.0,
            viewport_extent: 0.0,
            scrollbar: ScrollbarStyle::default(),
            drag_scroll: false,
            fill_viewport: false,
            physics: ScrollPhysics::default(),
            refresh: None,
            dragging: false,
            drag_last: 0.0,
            drag_real: 0.0,
            fling_velocity: 0.0,
            flinging: false,
            fling_samples: [(0.0, 0.0); 4],
            fling_samples_len: 0,
            fling_cursor: 0,
            on_scroll: None,
            controller: None,
            was_moving: false,
            bar_grab: None,
            bar_hover: false,
        }
    }

    /// Current position snapshot for a notification.
    fn metrics(&self) -> ScrollMetrics {
        ScrollMetrics {
            axis: self.axis,
            pixels: self.offset,
            min: 0.0,
            max: self.max_offset,
            viewport: self.viewport_extent,
        }
    }

    /// Fire a scroll notification (no-op when nothing is listening).
    fn emit(&self, event: ScrollEvent) {
        if let Some(cb) = &self.on_scroll {
            cb(ScrollNotification { metrics: self.metrics(), event });
        }
    }

    /// Mark a scroll activity as started, firing exactly one [`ScrollEvent::Start`].
    fn begin_activity(&mut self) {
        if !self.was_moving {
            self.was_moving = true;
            self.emit(ScrollEvent::Start);
        }
    }

    /// Emit a self-contained, balanced `Start → Update → End` triple for a programmatic
    /// scroll (`scroll_to`/`ensure_visible`), which snaps directly instead of animating.
    /// Any activity already open is closed first (its `Start` was emitted earlier), so a
    /// listener pairing `Start`/`End` never sees an `End` without a matching `Start`.
    fn notify_programmatic(&mut self, delta: f64) {
        if self.was_moving {
            self.was_moving = false;
            self.emit(ScrollEvent::End);
        }
        self.was_moving = true;
        self.emit(ScrollEvent::Start);
        self.emit(ScrollEvent::Update { delta });
        self.was_moving = false;
        self.emit(ScrollEvent::End);
    }

    /// Move the scroll *target* by `delta` (the spring eases the offset there).
    /// Returns whether the target moved (false when already pinned at an edge — the
    /// caller uses that to bubble the scroll to a parent). An explicit scroll stops
    /// any fling in progress.
    pub fn scroll_by(&mut self, delta: f64) -> bool {
        self.flinging = false;
        self.fling_velocity = 0.0;
        let mut next = (self.target + delta).clamp(0.0, self.max_offset);
        if self.snap > 0.0 {
            next = ((next / self.snap).round() * self.snap).clamp(0.0, self.max_offset);
        }
        let changed = (next - self.target).abs() > f64::EPSILON;
        self.target = next;
        if changed {
            // Wheel/keyboard move the target; the spring animates `offset` there,
            // so Update/End are emitted from `tick` — this just opens the activity.
            self.begin_activity();
        }
        changed
    }

    /// Whether the target is already at the far edge in the direction of `delta`
    /// (so a scroll should bubble to an ancestor scroll view).
    pub fn at_edge(&self, delta: f64) -> bool {
        (delta < 0.0 && self.target <= 0.0) || (delta > 0.0 && self.target >= self.max_offset)
    }

    /// Jump the target to an absolute offset (spring animates there). Stops any
    /// fling in progress.
    pub fn scroll_to(&mut self, offset: f64) {
        self.flinging = false;
        self.fling_velocity = 0.0;
        self.target = offset.clamp(0.0, self.max_offset);
    }

    // ----- content drag (pan-to-scroll) -----------------------------------

    /// The rubber-band transform applied to an unbanded offset (A4 overscroll:
    /// content past the edges moves at 1/3 the pointer speed).
    fn band(&self, v: f64) -> f64 {
        if v < 0.0 {
            v / 3.0
        } else if v > self.max_offset {
            self.max_offset + (v - self.max_offset) / 3.0
        } else {
            v
        }
    }

    /// Begin a content drag: the pointer is at `at` along the scroll axis.
    /// Returns whether this viewport accepts drags.
    pub fn drag_begin(&mut self, at: f64, now: f64) -> bool {
        if !self.drag_scroll {
            return false;
        }
        self.dragging = true;
        self.drag_last = at;
        self.drag_real = self.offset;
        self.velocity = 0.0;
        self.flinging = false;
        self.fling_velocity = 0.0;
        self.fling_samples_len = 0;
        self.fling_cursor = 0;
        self._sample(now);
        self.begin_activity();
        true
    }

    /// The pointer moved to `at`. Moves the content 1:1 (rubber-banded past the
    /// edges when `physics.overscroll`). Returns whether the offset changed.
    pub fn drag_move(&mut self, at: f64, now: f64) -> bool {
        if !self.dragging {
            return false;
        }
        let delta = at - self.drag_last; // positive = content moves with the pointer
        self.drag_last = at;
        let before = self.offset;
        self.drag_real -= delta;
        self.offset = if self.physics.overscroll {
            self.band(self.drag_real)
        } else {
            self.drag_real.clamp(0.0, self.max_offset)
        };
        self.target = self.offset;
        self._sample(now);
        let moved = self.offset - before;
        if moved.abs() > f64::EPSILON {
            self.emit(ScrollEvent::Update { delta: moved });
        }
        // Rubber-banded past an edge → an overscroll notification (Flutter parity).
        let over = if self.offset < 0.0 {
            self.offset
        } else if self.offset > self.max_offset {
            self.offset - self.max_offset
        } else {
            0.0
        };
        if over != 0.0 {
            self.emit(ScrollEvent::Overscroll { overscroll: over });
        }
        true
    }

    fn _sample(&mut self, now: f64) {
        let slot = &mut self.fling_samples[self.fling_cursor];
        *slot = (now, self.offset);
        self.fling_cursor = (self.fling_cursor + 1) % 4;
        self.fling_samples_len = self.fling_samples_len.saturating_add(1).min(4);
    }

    /// After a drag update: fire the pull-to-refresh arm callback once the pull
    /// crosses the threshold (A5).
    pub fn refresh_update(&mut self) {
        if let Some(r) = &mut self.refresh
            && self.offset <= -r.threshold
            && !r.fired_arm
        {
            r.fired_arm = true;
            if let Some(cb) = r.on_arm.clone() {
                cb();
            }
        }
    }

    /// At drag end: fire the armed-release callback (the app's refresh) or the
    /// plain-release callback (clears the half-armed spinner).
    pub fn refresh_end(&mut self) {
        if let Some(r) = &mut self.refresh {
            if r.fired_arm {
                let cb =
                    if self.offset <= -r.threshold { r.on_arm_release.clone() } else { r.on_release.clone() };
                if let Some(cb) = cb {
                    cb();
                }
            } else if let Some(cb) = r.on_release.clone() {
                cb();
            }
            r.fired_arm = false;
        }
    }

    /// End the drag: estimates the fling velocity from the last samples, springs
    /// back from any overscroll, and (unless past an edge) flings.
    pub fn drag_end(&mut self, now: f64) -> bool {
        if !self.dragging {
            return false;
        }
        self.dragging = false;
        self._sample(now);
        let past_start = self.offset < 0.0;
        let past_end = self.offset > self.max_offset;
        self.target = self.offset.clamp(0.0, self.max_offset);
        self.velocity = 0.0; // the spring starts from rest at the release point
        // Fling velocity from the oldest still-recorded sample.
        if !past_start && !past_end {
            let (t0, o0) = self.fling_samples[(self.fling_cursor + 4 - self.fling_samples_len) % 4];
            let dt = now - t0;
            let v = if dt > 1e-4 { (self.offset - o0) / dt } else { 0.0 };
            self.fling_velocity = if self.physics.overscroll { v } else { v.clamp(-4000.0, 4000.0) };
            if self.snap > 0.0 && self.fling_velocity.abs() < 200.0 {
                // Weak flings settle on the nearest snap point immediately.
                self.target = ((self.target / self.snap).round() * self.snap).clamp(0.0, self.max_offset);
                self.fling_velocity = 0.0;
            }
            self.flinging = self.fling_velocity.abs() >= 0.5;
        } else {
            self.fling_velocity = 0.0;
            self.flinging = false;
        }
        self.fling_samples_len = 0;
        true
    }

    /// Advance the spring (and any fling) by `dt` seconds. Returns whether it is
    /// still moving.
    pub fn tick(&mut self, dt: f64) -> bool {
        if self.dragging {
            return false; // the pointer drives the offset; nothing to animate
        }
        let before = self.offset;
        let dt = dt.clamp(0.0, 0.05); // guard against long stalls
        if dt <= 0.0 {
            return self.flinging || (self.offset - self.target).abs() >= 0.1;
        }
        let max = self.max_offset;

        // ── Fling: Flutter-style analytic friction, applied straight to the offset.
        // Velocity decays as v(t)=v0·e^(−r·t); the exact distance over this frame is
        // ∫₀ᵈᵗ v0·e^(−r·t) dt = v0·(1−e^(−r·dt))/r. Being closed-form it is dt-EXACT
        // (identical glide at 60/120/144 Hz or a stalled frame) and, being first-order,
        // it cannot ring — so there is nothing to shake. `friction` is read as the old
        // per-frame-at-60 Hz decay so existing tunings keep their feel: r = −60·ln(1−f).
        if self.flinging {
            let r = (-60.0 * (1.0 - self.physics.friction).ln()).max(1e-3);
            let decay = (-r * dt).exp();
            let dist = self.fling_velocity * (1.0 - decay) / r;
            self.fling_velocity *= decay;
            let mut next = self.offset + dist;
            if !self.physics.overscroll {
                // Clamp at the edges; hitting one ends the fling cleanly.
                if next <= 0.0 {
                    next = 0.0;
                    self.fling_velocity = 0.0;
                } else if next >= max {
                    next = max;
                    self.fling_velocity = 0.0;
                }
            } else if next < 0.0 || next > max {
                // Overscroll: keep this frame's small overshoot, then let the
                // first-order bounce below ease it back — no second bounce, no ring.
                self.fling_velocity = 0.0;
            }
            self.offset = next;
            self.target = next.clamp(0.0, max);
            if self.fling_velocity.abs() < 0.5 {
                self.flinging = false;
                self.fling_velocity = 0.0;
                if self.snap > 0.0 {
                    self.target = ((self.target / self.snap).round() * self.snap).clamp(0.0, max);
                }
            }
        }

        // ── Settle / smooth-scroll / overscroll bounce: first-order exponential ease
        // toward the goal. `offset += (goal−offset)·(1−e^(−r·dt))` is the continuous
        // form of a browser "smooth scroll": monotone (it NEVER overshoots), so it
        // cannot oscillate, and dt-exact via the exp. The fling owns the offset while
        // active; this drives wheel/keyboard glides, programmatic settles, and the
        // spring-back from an overscrolled edge. r = √stiffness (per second).
        if !self.flinging {
            let goal = self.target.clamp(0.0, max);
            let r = self.physics.stiffness.max(1.0).sqrt();
            let f = 1.0 - (-r * dt).exp();
            self.offset += (goal - self.offset) * f;
            if (goal - self.offset).abs() < 0.1 {
                self.offset = goal;
            }
        }

        let settled = !self.flinging && (self.offset - self.target.clamp(0.0, max)).abs() < 0.1;
        self.velocity = if self.flinging { self.fling_velocity } else { (self.offset - before) / dt };

        // Emit Update on real movement (opening the activity if a programmatic
        // `scroll_to` started it), and End exactly once when it settles.
        let delta = self.offset - before;
        if delta.abs() > 1e-9 {
            self.begin_activity();
            self.emit(ScrollEvent::Update { delta });
        }
        if settled && self.was_moving {
            self.was_moving = false;
            self.velocity = 0.0;
            self.emit(ScrollEvent::End);
        }
        !settled
    }

    /// Whether a scrollbar thumb is currently drawn (content overflows).
    pub fn scrollable(&self) -> bool {
        self.max_offset > 0.5 && self.scrollbar.policy != ScrollbarPolicy::Hidden
    }

    /// The bar geometry along the axis: `(track_start, track_len, thumb_start, thumb_len)`.
    fn bar_geometry(&self) -> (f64, f64, f64, f64) {
        let content = self.viewport_extent + self.max_offset;
        self.scrollbar.geometry(self.viewport_extent, content, self.offset)
    }

    /// The pointer position along the scroll axis.
    fn along(&self, local: Offset) -> f64 {
        match self.axis {
            Axis::Vertical => local.y,
            Axis::Horizontal => local.x,
        }
    }

    /// Whether a local point (relative to this object's origin) lands on the
    /// scrollbar strip — exactly the strip the bar paints in (thickness + margins),
    /// so content kept clear of it never loses a press to the bar.
    pub fn scrollbar_hit(&self, local: Offset, size: Size) -> bool {
        if !self.scrollable() {
            return false;
        }
        let grab = self.scrollbar.gutter();
        match self.axis {
            Axis::Vertical => {
                local.x >= size.width - grab
                    && local.x <= size.width
                    && local.y >= 0.0
                    && local.y <= size.height
            }
            Axis::Horizontal => {
                local.y >= size.height - grab
                    && local.y <= size.height
                    && local.x >= 0.0
                    && local.x <= size.width
            }
        }
    }

    /// Whether a local point is on the thumb itself (not the bare track).
    pub fn thumb_hit(&self, local: Offset, size: Size) -> bool {
        let (_, _, start, len) = self.bar_geometry();
        let p = self.along(local);
        self.scrollbar_hit(local, size) && p >= start && p <= start + len
    }

    /// A press on the scrollbar at `local` (desktop scrollbar semantics, VS Code /
    /// Chromium): on the thumb it grabs the thumb where it was pressed (no jump); on the
    /// track it jumps so the thumb centres on the press, then keeps dragging from there.
    pub fn bar_press(&mut self, local: Offset, size: Size) -> bool {
        let (_, _, start, len) = self.bar_geometry();
        let grab = if self.thumb_hit(local, size) { self.along(local) - start } else { len / 2.0 };
        self.bar_grab = Some(grab);
        self.set_offset_from_point(local, size);
        true
    }

    /// Whether the scrollbar thumb is being dragged (active state).
    pub fn bar_active(&self) -> bool {
        self.bar_grab.is_some()
    }

    /// Whether the pointer is over the scrollbar (hover state).
    pub fn bar_hovered(&self) -> bool {
        self.bar_hover
    }

    /// Set the hover state; returns whether it changed (needs a repaint).
    pub fn set_bar_hover(&mut self, hover: bool) -> bool {
        let changed = self.bar_hover != hover;
        self.bar_hover = hover;
        changed
    }

    /// Move the thumb so the grabbed point sits under `local` (scrollbar drag/click).
    /// Direct — no spring, no snap — because the thumb tracks the pointer 1:1. The
    /// pointer may leave the bar: only its position along the axis counts.
    pub fn set_offset_from_point(&mut self, local: Offset, _size: Size) -> bool {
        let (_, _, _, len) = self.bar_geometry();
        let grab = self.bar_grab.unwrap_or(len / 2.0);
        let content = self.viewport_extent + self.max_offset;
        let next = self.scrollbar.offset_for_thumb(self.viewport_extent, content, self.along(local) - grab);
        self.flinging = false;
        self.fling_velocity = 0.0;
        let delta = next - self.offset;
        let changed = delta.abs() > f64::EPSILON;
        self.offset = next;
        self.target = next;
        self.velocity = 0.0;
        if changed {
            self.begin_activity(); // scrollbar drag: End fires from `end_scroll_activity`
            self.emit(ScrollEvent::Update { delta });
        }
        changed
    }

    /// Close an in-progress scroll activity (scrollbar-drag release), firing a
    /// final [`ScrollEvent::End`] if one is open. Idempotent.
    pub fn end_scroll_activity(&mut self) {
        self.bar_grab = None;
        if self.was_moving {
            self.was_moving = false;
            self.emit(ScrollEvent::End);
        }
    }
}

impl RenderObject for RenderScroll {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let viewport = constraints.biggest();
        let Some(child) = cx.children().first().copied() else {
            return constraints.constrain(viewport);
        };

        // The cross axis is tight to the viewport when it's bounded, but shrink-wraps
        // the content when it isn't (e.g. a horizontal scroll inside a vertical page
        // scroll, where the incoming height is unbounded — tightening to it would
        // collapse or explode the box).
        let w_bounded = constraints.has_bounded_width();
        let h_bounded = constraints.has_bounded_height();
        // With `fill_viewport`, the child is at least the viewport size along the scroll
        // axis (fills when smaller, overflows/scrolls when larger).
        let child_constraints = match self.axis {
            Axis::Vertical => BoxConstraints {
                min_width: if w_bounded { viewport.width } else { 0.0 },
                max_width: if w_bounded { viewport.width } else { f64::INFINITY },
                min_height: if self.fill_viewport && h_bounded { viewport.height } else { 0.0 },
                max_height: f64::INFINITY,
            },
            Axis::Horizontal => BoxConstraints {
                min_width: if self.fill_viewport && w_bounded { viewport.width } else { 0.0 },
                max_width: f64::INFINITY,
                min_height: if h_bounded { viewport.height } else { 0.0 },
                max_height: if h_bounded { viewport.height } else { f64::INFINITY },
            },
        };
        let content = cx.layout_child(child, child_constraints);

        // Along the scroll axis the viewport is the available extent — or, when that
        // axis is unbounded (a scroll view inside a shrink-wrapping column), the
        // content extent (CSS `height: auto; overflow: auto`: nothing to scroll)
        // instead of an infinite box. On the cross axis it's the bounded viewport
        // size, or the content size when unbounded.
        let (viewport_extent, content_extent, cross) = match self.axis {
            Axis::Vertical => (
                if h_bounded { viewport.height } else { content.height },
                content.height,
                if w_bounded { viewport.width } else { content.width },
            ),
            Axis::Horizontal => (
                if w_bounded { viewport.width } else { content.width },
                content.width,
                if h_bounded { viewport.height } else { content.height },
            ),
        };
        self.viewport_extent = viewport_extent;
        self.max_offset = (content_extent - viewport_extent).max(0.0);
        // During a content drag the offset may rubber-band past the edges (A4
        // overscroll) — clamping here would fight the pointer. The clamp applies
        // once the drag ends (the spring then pulls it back).
        if !self.dragging {
            self.offset = self.offset.clamp(0.0, self.max_offset);
            self.target = self.target.clamp(0.0, self.max_offset);
        }

        // Publish metrics to the controller and apply any queued programmatic scroll.
        // A programmatic move snaps directly (no spring) so the target is in view now, and
        // fires a scroll notification (Update + End) — the same signal a wheel move sends —
        // so listeners (e.g. a virtualized editor tracking the offset) re-render at the new
        // position. Without this, `scroll_to`/`ensure_visible` would move the offset but any
        // offset-driven rendering would stay stale.
        if let Some(ctrl) = &self.controller
            && let Some(t) = ctrl.sync(self.offset, self.max_offset, self.viewport_extent)
        {
            let t = t.clamp(0.0, self.max_offset);
            // Report the APPLIED offset, so a second request before the next
            // layout measures from where the view now is (not one frame stale).
            ctrl.0.borrow_mut().offset = t;
            let delta = t - self.offset;
            self.offset = t;
            self.target = t;
            self.velocity = 0.0;
            if delta.abs() > 1e-9 {
                self.notify_programmatic(delta);
            }
        }

        let child_offset = match self.axis {
            Axis::Vertical => Offset::new(0.0, -self.offset),
            Axis::Horizontal => Offset::new(-self.offset, 0.0),
        };
        cx.set_child_offset(child, child_offset);

        let own = match self.axis {
            Axis::Vertical => Size::new(cross, viewport_extent),
            Axis::Horizontal => Size::new(viewport_extent, cross),
        };
        constraints.constrain(own)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        // Wheel/drag/fling moves can repaint without a re-layout: keep the
        // controller's offset current so `ensure_visible` measures from the
        // position actually on screen.
        if let Some(ctrl) = &self.controller
            && let Ok(mut st) = ctrl.0.try_borrow_mut()
        {
            st.offset = self.offset;
        }
        let size = cx.size();
        let Some(child) = cx.children().first().copied() else { return };

        // Clip + paint the scrolled content. The clip narrows the culling window
        // too, so offscreen content is never encoded (not merely clipped away).
        let clip = Rect::from_origin_size(offset.to_point(), size);
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &clip);
        // The content is a repaint boundary: re-recorded only when it changed or the
        // scroll moved what the viewport culls.
        cx.paint_child_clipped_as_boundary(child, offset + cx.child_offset(child), clip);
        cx.scene.pop_layer();

        // Scrollbar.
        let sb = &self.scrollbar;
        let show_track = sb.policy == ScrollbarPolicy::Always || self.scrollable();
        if sb.policy == ScrollbarPolicy::Hidden || !show_track {
            return;
        }
        let m = sb.margin;
        let (ts, tl, hs, hl) = self.bar_geometry();
        let (track, thumb) = match self.axis {
            Axis::Vertical => {
                let x1 = offset.x + size.width - m;
                let x0 = x1 - sb.thickness;
                (
                    Rect::new(x0, offset.y + ts, x1, offset.y + ts + tl),
                    Rect::new(x0, offset.y + hs, x1, offset.y + hs + hl),
                )
            }
            Axis::Horizontal => {
                let y1 = offset.y + size.height - m;
                let y0 = y1 - sb.thickness;
                (
                    Rect::new(offset.x + ts, y0, offset.x + ts + tl, y1),
                    Rect::new(offset.x + hs, y0, offset.x + hs + hl, y1),
                )
            }
        };
        let thumb_color = if self.bar_active() {
            sb.thumb_active_color
        } else if self.bar_hover {
            sb.thumb_hover_color
        } else {
            sb.thumb_color
        };
        cx.scene.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            sb.track_color,
            None,
            &RoundedRect::from_rect(track, sb.radius),
        );
        if self.scrollable() {
            cx.scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                thumb_color,
                None,
                &RoundedRect::from_rect(thumb, sb.radius),
            );
        }
    }

    fn clips_children(&self) -> bool {
        true // the viewport clips its content; culling caps at this box
    }

    fn debug_name(&self) -> &'static str {
        "RenderScroll"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(max: f64) -> RenderScroll {
        let mut s = RenderScroll::new(Axis::Vertical);
        s.viewport_extent = 500.0;
        s.max_offset = max;
        s
    }

    // Deterministic jittery frame times (12–24 ms) — the exact condition the old
    // stiff-spring + Euler integrator trembled under. A strictly monotone glide
    // with no overshoot is the mathematical signature of "no oscillation".
    fn jitter(i: usize) -> f64 {
        [0.0166, 0.0200, 0.0125, 0.0235, 0.0140, 0.0180][i % 6]
    }

    #[test]
    fn smooth_scroll_is_monotone_and_never_overshoots() {
        let mut s = mk(5000.0);
        s.scroll_by(1200.0);
        let target = s.target;
        assert!(target > 0.0);
        let mut prev = s.offset;
        let mut moving = true;
        for i in 0..600 {
            moving = s.tick(jitter(i));
            // A spring overshoot would reverse direction or pass the target.
            assert!(s.offset >= prev - 1e-6, "offset reversed: {prev} -> {}", s.offset);
            assert!(s.offset <= target + 1e-6, "overshot target: {} > {target}", s.offset);
            prev = s.offset;
            if !moving {
                break;
            }
        }
        assert!(!moving, "glide terminated");
        assert!((s.offset - target).abs() < 0.5, "settled on the target: {}", s.offset);
    }

    #[test]
    fn fling_is_monotone_and_settles() {
        let mut s = mk(100_000.0);
        s.fling_velocity = 3000.0;
        s.flinging = true;
        s.target = s.offset;
        let mut prev = s.offset;
        let mut frames = 0;
        for i in 0..3000 {
            let moving = s.tick(jitter(i));
            assert!(s.offset >= prev - 1e-6, "fling reversed direction (rang)");
            prev = s.offset;
            frames += 1;
            if !moving {
                break;
            }
        }
        assert!(!s.flinging, "fling ended");
        assert!(s.offset > 100.0, "fling covered real distance: {}", s.offset);
        assert!(frames < 3000, "fling terminated in bounded time");
    }

    #[test]
    fn glide_is_frame_rate_independent() {
        // The same wheel move at 60 Hz and at 144 Hz must land in the same place —
        // the closed-form model is dt-exact, unlike the old Euler spring.
        let mut a = mk(5000.0);
        a.scroll_by(800.0);
        let mut b = mk(5000.0);
        b.scroll_by(800.0);
        for _ in 0..2000 {
            if !a.tick(1.0 / 60.0) {
                break;
            }
        }
        for _ in 0..4000 {
            if !b.tick(1.0 / 144.0) {
                break;
            }
        }
        assert!(
            (a.offset - b.offset).abs() < 1.0,
            "60 Hz vs 144 Hz land together: {} vs {}",
            a.offset,
            b.offset
        );
    }

    #[test]
    fn programmatic_scroll_notifications_are_balanced() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let log: Rc<RefCell<Vec<ScrollEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let sink = log.clone();
        let mut s = mk(5000.0);
        s.on_scroll = Some(Rc::new(move |n| sink.borrow_mut().push(n.event)));

        // From idle: a programmatic snap emits its own Start → Update → End.
        s.notify_programmatic(120.0);
        // With an activity already open (a Start with no End yet), the programmatic snap
        // must close it and still end balanced — never an End without a matching Start.
        s.begin_activity();
        s.notify_programmatic(80.0);

        let ev = log.borrow();
        let mut depth = 0i32;
        for e in ev.iter() {
            match e {
                ScrollEvent::Start => depth += 1,
                ScrollEvent::End => {
                    depth -= 1;
                    assert!(depth >= 0, "End with no matching Start: {:?}", *ev);
                }
                _ => {}
            }
        }
        assert_eq!(depth, 0, "unbalanced Start/End: {:?}", *ev);
        assert!(
            ev.iter().any(|e| matches!(e, ScrollEvent::Update { .. })),
            "programmatic scroll emitted an Update: {:?}",
            *ev
        );
    }

    #[test]
    fn a_long_stall_frame_does_not_explode() {
        // A 500 ms hitch (dt clamped) must not fling the offset past its target or
        // NaN — the failure mode of an unclamped stiff Euler spring.
        let mut s = mk(3000.0);
        s.scroll_by(600.0);
        s.tick(0.5);
        assert!(s.offset.is_finite());
        assert!(s.offset >= 0.0 && s.offset <= 600.0 + 1e-6, "stayed in range: {}", s.offset);
    }
}
