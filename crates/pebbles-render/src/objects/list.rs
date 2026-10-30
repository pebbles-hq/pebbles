//! [`RenderList`] — a **controlled** viewport for virtualized lists: the offset is
//! an input (the widget's signal), not mutated here. Split from `scroll.rs`; the
//! imperative sibling is [`RenderScroll`](super::scroll::RenderScroll).

use kurbo::{Affine, RoundedRect};
use pebbles_foundation::{Axis, Offset, Rect, Size};
use peniko::{Fill, Mix};

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

use super::scroll::{ScrollbarPolicy, ScrollbarStyle};

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
    /// Whether a touch/pointer drag over the viewport scrolls it (finger pan). On by
    /// default so virtualized lists finger-scroll on touch; the wheel + scrollbar paths
    /// are unaffected either way.
    pub drag_scroll: bool,
    viewport_extent: f64,
    /// While the scrollbar is dragged: the grab point along the thumb (px from its start).
    pub bar_grab: Option<f64>,
    /// Whether the pointer is over the scrollbar strip (hover state).
    pub bar_hover: bool,
}

impl RenderList {
    pub fn new(axis: Axis) -> Self {
        RenderList {
            axis,
            offset: 0.0,
            content_extent: 0.0,
            id: 0,
            scrollbar: ScrollbarStyle::default(),
            drag_scroll: true,
            viewport_extent: 0.0,
            bar_grab: None,
            bar_hover: false,
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
    /// `(track_start, track_len, thumb_start, thumb_len)` along the axis.
    fn bar_geometry(&self) -> (f64, f64, f64, f64) {
        self.scrollbar.geometry(self.viewport_extent, self.content_extent, self.offset)
    }

    fn along(&self, local: Offset) -> f64 {
        match self.axis {
            Axis::Vertical => local.y,
            Axis::Horizontal => local.x,
        }
    }

    /// Whether a local point lands on the scrollbar strip (exactly the painted strip:
    /// thickness + margins).
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

    /// A press on the scrollbar: on the thumb, grab it where pressed (no jump); on the
    /// track, centre the thumb on the press and drag from there.
    pub fn bar_press(&mut self, local: Offset, size: Size) {
        let (_, _, start, len) = self.bar_geometry();
        let p = self.along(local);
        let on_thumb = self.scrollbar_hit(local, size) && p >= start && p <= start + len;
        self.bar_grab = Some(if on_thumb { p - start } else { len / 2.0 });
    }

    /// Fraction `0.0..=1.0` of the scroll range that keeps the grabbed point of the
    /// thumb under the pointer at `local`.
    pub fn fraction_at(&self, local: Offset) -> f64 {
        let (_, _, _, len) = self.bar_geometry();
        let grab = self.bar_grab.unwrap_or(len / 2.0);
        let max = self.max_offset();
        if max <= 0.0 {
            return 0.0;
        }
        self.scrollbar.offset_for_thumb(self.viewport_extent, self.content_extent, self.along(local) - grab)
            / max
    }
}

impl RenderObject for RenderList {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
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

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        let size = cx.size();
        if let Some(child) = cx.children().first().copied() {
            let clip = Rect::from_origin_size(offset.to_point(), size);
            cx.scene.push_layer(Fill::NonZero, Mix::Normal, 1.0, Affine::IDENTITY, &clip);
            cx.paint_child_clipped(child, offset + cx.child_offset(child), clip);
            cx.scene.pop_layer();
        }

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
        let thumb_color = if self.bar_grab.is_some() {
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
        "RenderList"
    }
}
