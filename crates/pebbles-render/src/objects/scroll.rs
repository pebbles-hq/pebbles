//! [`RenderScroll`] — a single-child viewport that clips its content, offsets it
//! along one axis, and paints a customizable scrollbar. The offset is mutated
//! imperatively (wheel events + scrollbar drag, routed from the shell) and clamped
//! to the content extent during layout.

use pebbles_foundation::{Axis, Color, Offset, Rect, Size};
use vello::kurbo::{Affine, RoundedRect};
use vello::peniko::{Fill, Mix};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

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
    pub track_color: Color,
    pub radius: f64,
}

impl Default for ScrollbarStyle {
    fn default() -> Self {
        ScrollbarStyle {
            policy: ScrollbarPolicy::Auto,
            thickness: 8.0,
            min_thumb: 28.0,
            margin: 2.0,
            thumb_color: Color::from_rgba8(0x94, 0xa3, 0xb8, 0xcc), // slate-400
            track_color: Color::from_rgba8(0x94, 0xa3, 0xb8, 0x1f),
            radius: 999.0,
        }
    }
}

/// The physics of a scroll view: how the offset eases toward its target, how a
/// fling decays, and whether drags may pull past the edges (rubber-banding).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollPhysics {
    /// Spring stiffness for the offset→target ease (critically damped).
    pub stiffness: f64,
    /// Per-frame fling friction: velocity *= (1 − friction)^(dt·60).
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
    pub on_arm: Option<std::rc::Rc<dyn Fn()>>,
    /// Fired once when an ARMED pull is released — the app's `on_refresh`.
    pub on_arm_release: Option<std::rc::Rc<dyn Fn()>>,
    /// Fired once when a pull is released without ever arming (clears the
    /// half-rotated spinner state).
    pub on_release: Option<std::rc::Rc<dyn Fn()>>,
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

/// A scrollable viewport.
pub struct RenderScroll {
    pub axis: Axis,
    /// Displayed scroll offset (eased toward `target` by the spring).
    pub offset: f64,
    /// Where the offset is heading — wheel/keyboard move this; the spring animates
    /// `offset` toward it for a smooth, momentum-like glide.
    pub target: f64,
    /// Current spring velocity (px/s) — the offset→target ease. During a fling
    /// this tracks the moving target; the fling's own speed lives in
    /// the private `fling_velocity` field.
    pub velocity: f64,
    /// Decaying fling speed (px/s) while [`flinging`](Self::flinging).
    fling_velocity: f64,
    /// Whether a fling is in progress (the target advances, the spring follows).
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
        }
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
        self.drag_real -= delta;
        self.offset = if self.physics.overscroll { self.band(self.drag_real) } else {
            self.drag_real.clamp(0.0, self.max_offset)
        };
        self.target = self.offset;
        self._sample(now);
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
        if let Some(r) = &mut self.refresh {
            if self.offset <= -r.threshold && !r.fired_arm {
                r.fired_arm = true;
                if let Some(cb) = r.on_arm.clone() {
                    cb();
                }
            }
        }
    }

    /// At drag end: fire the armed-release callback (the app's refresh) or the
    /// plain-release callback (clears the half-armed spinner).
    pub fn refresh_end(&mut self) {
        if let Some(r) = &mut self.refresh {
            if r.fired_arm {
                let cb = if self.offset <= -r.threshold {
                    r.on_arm_release.clone()
                } else {
                    r.on_release.clone()
                };
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
            let (t0, o0) = self.fling_samples
                [(self.fling_cursor + 4 - self.fling_samples_len) % 4];
            let dt = now - t0;
            let v = if dt > 1e-4 { (self.offset - o0) / dt } else { 0.0 };
            self.fling_velocity = if self.physics.overscroll { v } else { v.clamp(-4000.0, 4000.0) };
            if self.snap > 0.0 && self.fling_velocity.abs() < 200.0 {
                // Weak flings settle on the nearest snap point immediately.
                self.target = ((self.target / self.snap).round() * self.snap)
                    .clamp(0.0, self.max_offset);
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
        let dt = dt.clamp(0.0, 0.05); // guard against long stalls
        // Fling: the target keeps moving while the fling velocity decays.
        if self.flinging {
            let decay = (1.0 - self.physics.friction).powf(dt * 60.0);
            self.fling_velocity *= decay;
            self.target =
                (self.target + self.fling_velocity * dt).clamp(0.0, self.max_offset);
            if self.fling_velocity.abs() < 0.5 {
                self.flinging = false;
                self.fling_velocity = 0.0;
                if self.snap > 0.0 {
                    self.target = ((self.target / self.snap).round() * self.snap)
                        .clamp(0.0, self.max_offset);
                }
            }
        }
        let stiffness = self.physics.stiffness.max(1.0);
        let damping = 2.0 * stiffness.sqrt(); // critically damped — no overshoot
        let x = self.offset - self.target;
        let accel = -stiffness * x - damping * self.velocity;
        self.velocity += accel * dt;
        self.offset += self.velocity * dt;
        if x.abs() < 0.1 && self.velocity.abs() < 0.5 {
            self.offset = self.target;
            self.velocity = 0.0;
            false
        } else {
            true
        }
    }

    /// Whether a scrollbar thumb is currently drawn (content overflows).
    pub fn scrollable(&self) -> bool {
        self.max_offset > 0.5 && self.scrollbar.policy != ScrollbarPolicy::Hidden
    }

    /// Thumb length for the current content/viewport ratio.
    fn thumb_len(&self) -> f64 {
        let content = self.viewport_extent + self.max_offset;
        if content <= 0.0 {
            return self.viewport_extent;
        }
        let ratio = (self.viewport_extent / content).clamp(0.0, 1.0);
        (self.viewport_extent * ratio).max(self.scrollbar.min_thumb).min(self.viewport_extent)
    }

    /// Thumb start position along the axis for the current offset.
    fn thumb_pos(&self) -> f64 {
        let travel = (self.viewport_extent - self.thumb_len()).max(0.0);
        if self.max_offset <= 0.0 {
            0.0
        } else {
            travel * (self.offset / self.max_offset)
        }
    }

    /// Whether a local point (relative to this object's origin) lands on the
    /// scrollbar strip — with a generous grab margin.
    pub fn scrollbar_hit(&self, local: Offset, size: Size) -> bool {
        if !self.scrollable() {
            return false;
        }
        let grab = self.scrollbar.thickness.max(16.0);
        match self.axis {
            Axis::Vertical => local.x >= size.width - grab && local.x <= size.width,
            Axis::Horizontal => local.y >= size.height - grab && local.y <= size.height,
        }
    }

    /// Set the offset so the thumb centers on a local point (scrollbar drag/click).
    /// Direct — no spring, no snap — because the thumb tracks the pointer 1:1.
    pub fn set_offset_from_point(&mut self, local: Offset, _size: Size) -> bool {
        let pos = match self.axis {
            Axis::Vertical => local.y,
            Axis::Horizontal => local.x,
        };
        let thumb = self.thumb_len();
        let travel = (self.viewport_extent - thumb).max(1.0);
        let frac = ((pos - thumb / 2.0) / travel).clamp(0.0, 1.0);
        let next = frac * self.max_offset;
        let changed = (next - self.offset).abs() > f64::EPSILON;
        self.offset = next;
        self.target = next;
        self.velocity = 0.0;
        changed
    }
}

impl RenderObject for RenderScroll {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let viewport = constraints.biggest();
        let Some(child) = cx.children().first().copied() else {
            return constraints.constrain(viewport);
        };

        // Unbounded along the scroll axis, tight on the cross axis.
        let child_constraints = match self.axis {
            Axis::Vertical => BoxConstraints {
                min_width: viewport.width,
                max_width: viewport.width,
                min_height: 0.0,
                max_height: f64::INFINITY,
            },
            Axis::Horizontal => BoxConstraints {
                min_width: 0.0,
                max_width: f64::INFINITY,
                min_height: viewport.height,
                max_height: viewport.height,
            },
        };
        let content = cx.layout_child(child, child_constraints);

        let (viewport_extent, content_extent) = match self.axis {
            Axis::Vertical => (viewport.height, content.height),
            Axis::Horizontal => (viewport.width, content.width),
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

        let child_offset = match self.axis {
            Axis::Vertical => Offset::new(0.0, -self.offset),
            Axis::Horizontal => Offset::new(-self.offset, 0.0),
        };
        cx.set_child_offset(child, child_offset);

        constraints.constrain(viewport)
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let size = cx.size();
        let Some(child) = cx.children().first().copied() else { return };

        // Clip + paint the scrolled content. The clip narrows the culling window
        // too, so offscreen content is never encoded (not merely clipped away).
        let clip = Rect::from_origin_size(offset.to_point(), size);
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &clip);
        cx.paint_child_clipped(child, offset + cx.child_offset(child), clip);
        cx.scene.pop_layer();

        // Scrollbar.
        let sb = &self.scrollbar;
        let show_track = sb.policy == ScrollbarPolicy::Always || self.scrollable();
        if sb.policy == ScrollbarPolicy::Hidden || !show_track {
            return;
        }
        let m = sb.margin;
        let (track, thumb) = match self.axis {
            Axis::Vertical => {
                let x1 = offset.x + size.width - m;
                let x0 = x1 - sb.thickness;
                let track = Rect::new(x0, offset.y + m, x1, offset.y + size.height - m);
                let thumb = Rect::new(
                    x0,
                    offset.y + self.thumb_pos(),
                    x1,
                    offset.y + self.thumb_pos() + self.thumb_len(),
                );
                (track, thumb)
            }
            Axis::Horizontal => {
                let y1 = offset.y + size.height - m;
                let y0 = y1 - sb.thickness;
                let track = Rect::new(offset.x + m, y0, offset.x + size.width - m, y1);
                let thumb = Rect::new(
                    offset.x + self.thumb_pos(),
                    y0,
                    offset.x + self.thumb_pos() + self.thumb_len(),
                    y1,
                );
                (track, thumb)
            }
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
                sb.thumb_color,
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

