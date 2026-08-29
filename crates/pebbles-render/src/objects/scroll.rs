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

/// A scrollable viewport.
pub struct RenderScroll {
    pub axis: Axis,
    /// Displayed scroll offset (eased toward `target` by the spring).
    pub offset: f64,
    /// Where the offset is heading — wheel/keyboard move this; the spring animates
    /// `offset` toward it for a smooth, momentum-like glide.
    pub target: f64,
    /// Current spring velocity (px/s).
    pub velocity: f64,
    /// Snap increment: after settling, the target rounds to a multiple of this
    /// (0 = no snapping).
    pub snap: f64,
    /// Maximum scrollable offset, computed each layout as `content - viewport`.
    pub max_offset: f64,
    /// Viewport extent along `axis`, computed each layout.
    pub viewport_extent: f64,
    pub scrollbar: ScrollbarStyle,
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
        }
    }

    /// Move the scroll *target* by `delta` (the spring eases the offset there).
    /// Returns whether the target moved (false when already pinned at an edge — the
    /// caller uses that to bubble the scroll to a parent).
    pub fn scroll_by(&mut self, delta: f64) -> bool {
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

    /// Jump the target to an absolute offset (spring animates there).
    pub fn scroll_to(&mut self, offset: f64) {
        self.target = offset.clamp(0.0, self.max_offset);
    }

    /// Advance the spring by `dt` seconds. Returns whether it is still moving.
    pub fn tick(&mut self, dt: f64) -> bool {
        let dt = dt.clamp(0.0, 0.05); // guard against long stalls
        let stiffness = 240.0_f64;
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
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
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
        self.offset = self.offset.clamp(0.0, self.max_offset);
        self.target = self.target.clamp(0.0, self.max_offset);

        let child_offset = match self.axis {
            Axis::Vertical => Offset::new(0.0, -self.offset),
            Axis::Horizontal => Offset::new(-self.offset, 0.0),
        };
        cx.set_child_offset(child, child_offset);

        constraints.constrain(viewport)
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let size = cx.size();
        let Some(child) = cx.children().first().copied() else { return };

        // Clip + paint the scrolled content.
        let clip = Rect::from_origin_size(offset.to_point(), size);
        cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &clip);
        cx.paint_child(child, offset + cx.child_offset(child));
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

    fn debug_name(&self) -> &'static str {
        "RenderScroll"
    }
}

// ===========================================================================
// RenderList — a CONTROLLED viewport for virtualized lists.
// ===========================================================================

/// A viewport whose offset is **controlled** by the widget layer (a signal), used
/// by the build-on-demand `ListView`. It lays its child out at the full content
/// extent, offsets it by `offset`, clips, publishes its metrics for the widget to
/// clamp against, and paints the same customizable scrollbar. Wheel + scrollbar
/// drag are routed to the widget's offset signal (see `pebbles_core::scroll`), not
/// mutated here — that's what makes only the visible items rebuild on scroll.
pub struct RenderList {
    pub axis: Axis,
    /// Current offset (input from the widget's signal).
    pub offset: f64,
    /// Total scrollable content extent (input — the widget knows count × extent).
    pub content_extent: f64,
    /// Stable id for metrics publishing + scroll routing.
    pub id: u64,
    pub scrollbar: ScrollbarStyle,
    viewport_extent: f64,
}

impl RenderList {
    pub fn new(axis: Axis) -> Self {
        RenderList {
            axis,
            offset: 0.0,
            content_extent: 0.0,
            id: 0,
            scrollbar: ScrollbarStyle::default(),
            viewport_extent: 0.0,
        }
    }

    fn max_offset(&self) -> f64 {
        (self.content_extent - self.viewport_extent).max(0.0)
    }
    /// The measured viewport extent along the scroll axis.
    pub fn viewport(&self) -> f64 {
        self.viewport_extent
    }
    /// Whether the offset is pinned at the edge in the direction of `delta`.
    pub fn at_edge(&self, delta: f64) -> bool {
        (delta < 0.0 && self.offset <= 0.0) || (delta > 0.0 && self.offset >= self.max_offset())
    }
    pub fn scrollable(&self) -> bool {
        self.max_offset() > 0.5 && self.scrollbar.policy != ScrollbarPolicy::Hidden
    }
    fn thumb_len(&self) -> f64 {
        if self.content_extent <= 0.0 {
            return self.viewport_extent;
        }
        let ratio = (self.viewport_extent / self.content_extent).clamp(0.0, 1.0);
        (self.viewport_extent * ratio).max(self.scrollbar.min_thumb).min(self.viewport_extent)
    }
    fn thumb_pos(&self) -> f64 {
        let travel = (self.viewport_extent - self.thumb_len()).max(0.0);
        let max = self.max_offset();
        if max <= 0.0 { 0.0 } else { travel * (self.offset / max) }
    }

    /// Whether a local point lands on the scrollbar strip.
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

    /// Fraction `0.0..=1.0` of the scroll range for a local scrollbar point (so the
    /// thumb centers on the pointer).
    pub fn fraction_at(&self, local: Offset) -> f64 {
        let pos = match self.axis {
            Axis::Vertical => local.y,
            Axis::Horizontal => local.x,
        };
        let thumb = self.thumb_len();
        let travel = (self.viewport_extent - thumb).max(1.0);
        ((pos - thumb / 2.0) / travel).clamp(0.0, 1.0)
    }
}

impl RenderObject for RenderList {
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        let viewport = constraints.biggest();
        let cross = match self.axis {
            Axis::Vertical => viewport.width,
            Axis::Horizontal => viewport.height,
        };
        self.viewport_extent = match self.axis {
            Axis::Vertical => viewport.height,
            Axis::Horizontal => viewport.width,
        };
        crate::scroll_metrics::store(self.id, self.viewport_extent, self.content_extent, cross);

        if let Some(child) = cx.children().first().copied() {
            // Lay the child out at the full content extent so its absolutely
            // positioned items sit at their true offsets.
            let child_constraints = match self.axis {
                Axis::Vertical => BoxConstraints {
                    min_width: viewport.width,
                    max_width: viewport.width,
                    min_height: self.content_extent,
                    max_height: self.content_extent,
                },
                Axis::Horizontal => BoxConstraints {
                    min_width: self.content_extent,
                    max_width: self.content_extent,
                    min_height: viewport.height,
                    max_height: viewport.height,
                },
            };
            cx.layout_child(child, child_constraints);
            let off = self.offset.clamp(0.0, self.max_offset());
            let child_offset = match self.axis {
                Axis::Vertical => Offset::new(0.0, -off),
                Axis::Horizontal => Offset::new(-off, 0.0),
            };
            cx.set_child_offset(child, child_offset);
        }
        constraints.constrain(viewport)
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let size = cx.size();
        if let Some(child) = cx.children().first().copied() {
            let clip = Rect::from_origin_size(offset.to_point(), size);
            cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &clip);
            cx.paint_child(child, offset + cx.child_offset(child));
            cx.scene.pop_layer();
        }

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
                (
                    Rect::new(x0, offset.y + m, x1, offset.y + size.height - m),
                    Rect::new(
                        x0,
                        offset.y + self.thumb_pos(),
                        x1,
                        offset.y + self.thumb_pos() + self.thumb_len(),
                    ),
                )
            }
            Axis::Horizontal => {
                let y1 = offset.y + size.height - m;
                let y0 = y1 - sb.thickness;
                (
                    Rect::new(offset.x + m, y0, offset.x + size.width - m, y1),
                    Rect::new(
                        offset.x + self.thumb_pos(),
                        y0,
                        offset.x + self.thumb_pos() + self.thumb_len(),
                        y1,
                    ),
                )
            }
        };
        cx.scene.fill(Fill::NonZero, Affine::IDENTITY, sb.track_color, None, &RoundedRect::from_rect(track, sb.radius));
        if self.scrollable() {
            cx.scene.fill(Fill::NonZero, Affine::IDENTITY, sb.thumb_color, None, &RoundedRect::from_rect(thumb, sb.radius));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderList"
    }
}
