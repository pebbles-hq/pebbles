//! [`RenderFlex`] — the workhorse behind `Row` and `Column`. Implements Flutter's
//! two-pass flex layout: lay out inflexible children first, distribute the leftover
//! main-axis space to flexible children by their flex factor, then position
//! everything per the main- and cross-axis alignment.

use pebbles_foundation::{
    Axis, CrossAxisAlignment, FlexFit, MainAxisAlignment, MainAxisSize, Offset, Size, TextBaseline,
    TextDirection, VerticalDirection,
};

use crate::RenderId;
use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{IntrinsicCx, LayoutCx, PaintCx};

/// Dev-mode overflow-log throttle: emit each unique overflow signature at most
/// once per ~3 seconds so a persistent overflow doesn't flood the log every frame.
fn overflow_should_log(sig: (&'static str, i64, usize)) -> bool {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};
    thread_local! {
        static SEEN: RefCell<HashMap<(&'static str, i64, usize), Instant>> =
            RefCell::new(HashMap::new());
    }
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        let now = Instant::now();
        match seen.get(&sig) {
            Some(&last) if now.duration_since(last) < Duration::from_secs(3) => false,
            _ => {
                seen.insert(sig, now);
                true
            }
        }
    })
}

/// Layout data attached to a flex child by an `Expanded`/`Flexible` widget.
#[derive(Clone, Copy, Debug)]
pub struct FlexParentData {
    /// Flex factor; `0` means the child is inflexible (sizes to its content).
    pub flex: u32,
    /// Whether the child must fill (`Tight`) or may under-fill (`Loose`) its share.
    pub fit: FlexFit,
}

impl Default for FlexParentData {
    fn default() -> Self {
        FlexParentData { flex: 0, fit: FlexFit::Tight }
    }
}

/// A flexible box that lays children along a main [`Axis`].
pub struct RenderFlex {
    pub axis: Axis,
    pub main_axis_alignment: MainAxisAlignment,
    pub cross_axis_alignment: CrossAxisAlignment,
    pub main_axis_size: MainAxisSize,
    /// Fixed gap inserted between adjacent children (Flutter's `Flex.spacing`).
    pub spacing: f64,
    /// Which vertical direction is "start" (`Down` = top-down, `Up` = bottom-up).
    /// Affects a Column's main axis and a Row's cross axis.
    pub vertical_direction: VerticalDirection,
    /// The baseline to align on when `cross_axis_alignment == Baseline`
    /// (alphabetic vs ideographic — parley's baseline serves both today).
    pub text_baseline: TextBaseline,
}

impl RenderFlex {
    pub fn new(
        axis: Axis,
        main_axis_alignment: MainAxisAlignment,
        cross_axis_alignment: CrossAxisAlignment,
        main_axis_size: MainAxisSize,
        spacing: f64,
        vertical_direction: VerticalDirection,
        text_baseline: TextBaseline,
    ) -> Self {
        RenderFlex {
            axis,
            main_axis_alignment,
            cross_axis_alignment,
            main_axis_size,
            spacing,
            vertical_direction,
            text_baseline,
        }
    }

    /// Whether the main axis runs in reverse: a `Column` with `Up`, or (D2) a `Row`
    /// under a right-to-left ambient [`TextDirection`] — RTL reverses a Row's child
    /// order and mirrors its Start/End alignment.
    fn main_reversed(&self) -> bool {
        match self.axis {
            Axis::Vertical => self.vertical_direction == VerticalDirection::Up,
            Axis::Horizontal => crate::direction::text_direction() == TextDirection::Rtl,
        }
    }

    /// Whether the cross axis runs in reverse (`Row` with `Up`).
    fn cross_reversed(&self) -> bool {
        self.axis == Axis::Horizontal && self.vertical_direction == VerticalDirection::Up
    }

    fn main_of(&self, size: Size) -> f64 {
        match self.axis {
            Axis::Horizontal => size.width,
            Axis::Vertical => size.height,
        }
    }

    fn cross_of(&self, size: Size) -> f64 {
        match self.axis {
            Axis::Horizontal => size.height,
            Axis::Vertical => size.width,
        }
    }

    fn make_size(&self, main: f64, cross: f64) -> Size {
        match self.axis {
            Axis::Horizontal => Size::new(main, cross),
            Axis::Vertical => Size::new(cross, main),
        }
    }

    fn make_offset(&self, main: f64, cross: f64) -> Offset {
        match self.axis {
            Axis::Horizontal => Offset::new(main, cross),
            Axis::Vertical => Offset::new(cross, main),
        }
    }

    /// Build child constraints from main/cross bounds, mapped onto width/height.
    fn child_constraints(
        &self,
        main_min: f64,
        main_max: f64,
        cross_min: f64,
        cross_max: f64,
    ) -> BoxConstraints {
        match self.axis {
            Axis::Horizontal => BoxConstraints {
                min_width: main_min,
                max_width: main_max,
                min_height: cross_min,
                max_height: cross_max,
            },
            Axis::Vertical => BoxConstraints {
                min_width: cross_min,
                max_width: cross_max,
                min_height: main_min,
                max_height: main_max,
            },
        }
    }
}

impl RenderObject for RenderFlex {
    fn layout(&mut self, cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        let main_max = self.main_of(constraints.biggest());
        let cross_max = self.cross_of(constraints.biggest());
        let cross_bounded = cross_max.is_finite();
        let stretch = self.cross_axis_alignment == CrossAxisAlignment::Stretch;

        // Snapshot children + their flex data before any mutable layout call.
        let children: Vec<(RenderId, FlexParentData)> = cx
            .children()
            .into_iter()
            .map(|c| (c, cx.child_parent_data::<FlexParentData>(c).copied().unwrap_or_default()))
            .collect();

        // Fixed inter-child spacing (Flutter's Flex.spacing): reserved before flex
        // distribution and inserted between adjacent children when positioning.
        let n = children.len();
        let total_gap = if n > 1 { self.spacing * (n - 1) as f64 } else { 0.0 };

        let (cross_min_child, cross_max_child) = if stretch && cross_bounded {
            (cross_max, cross_max)
        } else {
            (0.0, cross_max)
        };

        // Pass 1: inflexible children take their natural main size.
        let mut allocated_main = 0.0_f64;
        let mut max_cross = 0.0_f64;
        let mut total_flex = 0u32;
        for &(child, data) in &children {
            if data.flex > 0 {
                total_flex += data.flex;
                continue;
            }
            let c = self.child_constraints(0.0, f64::INFINITY, cross_min_child, cross_max_child);
            let size = cx.layout_child(child, c);
            allocated_main += self.main_of(size);
            max_cross = max_cross.max(self.cross_of(size));
        }

        // Pass 2: flexible children share the remaining main-axis space (after the
        // fixed gaps are reserved).
        if total_flex > 0 && main_max.is_finite() {
            let free = (main_max - allocated_main - total_gap).max(0.0);
            let space_per_flex = free / total_flex as f64;
            for &(child, data) in &children {
                if data.flex == 0 {
                    continue;
                }
                let extent = space_per_flex * data.flex as f64;
                let (main_min, main_max_c) = match data.fit {
                    FlexFit::Tight => (extent, extent),
                    FlexFit::Loose => (0.0, extent),
                };
                let c =
                    self.child_constraints(main_min, main_max_c, cross_min_child, cross_max_child);
                let size = cx.layout_child(child, c);
                allocated_main += self.main_of(size);
                max_cross = max_cross.max(self.cross_of(size));
            }
        }

        // Resolve our own size — the content is the children plus the fixed gaps.
        let content_main = allocated_main + total_gap;
        let main_size = match self.main_axis_size {
            MainAxisSize::Max if main_max.is_finite() => main_max,
            _ => content_main,
        };
        let cross_size = if stretch && cross_bounded { cross_max } else { max_cross };
        let size = constraints.constrain(self.make_size(main_size, cross_size));
        let final_main = self.main_of(size);
        let final_cross = self.cross_of(size);

        // Flutter-style overflow detection (dev mode only): the children want more
        // main-axis room than we were given. This is the classic "A RenderFlex
        // overflowed by N pixels" — the content is clipped/painted out of bounds.
        // Only meaningful when our main axis is actually bounded (a scroll view
        // hands down an infinite main extent on purpose — that's not overflow).
        if pebbles_foundation::log::dev_mode() && main_max.is_finite() {
            let overflow = content_main - final_main;
            if overflow > 0.5 {
                let axis = if self.axis == Axis::Horizontal { "horizontal" } else { "vertical" };
                let widget = if self.axis == Axis::Horizontal { "Row" } else { "Column" };
                // Throttle: the same overflow re-fires every frame during layout.
                // Key by (widget, rounded overflow, child count) and emit at most
                // once per ~3s per unique signature — enough to notice, not flood.
                let sig = (widget, overflow.round() as i64, n);
                if overflow_should_log(sig) {
                    pebbles_foundation::log::warn(
                        pebbles_foundation::log::Cat::Layout,
                        format!(
                            "{widget} overflowed by {overflow:.1}px on the {axis} axis \
                             (children need {content_main:.1}px, only {final_main:.1}px available; \
                             {n} children). Wrap it in a scroll view, use Expanded/Flexible, or shrink a child.",
                        ),
                    );
                }
            }
        }

        // Position children along the main axis per the alignment. A reversed main
        // axis (`Column` with `Up`) swaps Start↔End and lays children bottom-up.
        let free = (final_main - content_main).max(0.0);
        let eff_align = if self.main_reversed() {
            match self.main_axis_alignment {
                MainAxisAlignment::Start => MainAxisAlignment::End,
                MainAxisAlignment::End => MainAxisAlignment::Start,
                other => other,
            }
        } else {
            self.main_axis_alignment
        };
        let (leading, between) = match eff_align {
            MainAxisAlignment::Start => (0.0, 0.0),
            MainAxisAlignment::End => (free, 0.0),
            MainAxisAlignment::Center => (free / 2.0, 0.0),
            MainAxisAlignment::SpaceBetween => {
                if n > 1 { (0.0, free / (n - 1) as f64) } else { (0.0, 0.0) }
            }
            MainAxisAlignment::SpaceAround => {
                let b = if n > 0 { free / n as f64 } else { 0.0 };
                (b / 2.0, b)
            }
            MainAxisAlignment::SpaceEvenly => {
                let b = free / (n as f64 + 1.0);
                (b, b)
            }
        };

        // Baseline alignment: children sit on the tallest baseline among them.
        let baseline = self.axis == Axis::Horizontal
            && self.cross_axis_alignment == CrossAxisAlignment::Baseline;
        let max_baseline = if baseline {
            children.iter().filter_map(|&(c, _)| cx.child_baseline(c)).fold(0.0_f64, f64::max)
        } else {
            0.0
        };

        let mut pos = leading;
        let placed: Box<dyn Iterator<Item = (RenderId, FlexParentData)>> = if self.main_reversed() {
            Box::new(children.iter().rev().copied())
        } else {
            Box::new(children.iter().copied())
        };
        for (child, _) in placed {
            let child_size = cx.child_size(child);
            let child_main = self.main_of(child_size);
            let child_cross = self.cross_of(child_size);
            let cross_pos = if baseline {
                max_baseline - cx.child_baseline(child).unwrap_or(0.0)
            } else {
                match self.cross_axis_alignment {
                    CrossAxisAlignment::Start | CrossAxisAlignment::Stretch => {
                        if self.cross_reversed() { final_cross - child_cross } else { 0.0 }
                    }
                    CrossAxisAlignment::End => {
                        if self.cross_reversed() { 0.0 } else { final_cross - child_cross }
                    }
                    CrossAxisAlignment::Center | CrossAxisAlignment::Baseline => {
                        (final_cross - child_cross) / 2.0
                    }
                }
            };
            cx.set_child_offset(child, self.make_offset(pos, cross_pos));
            pos += child_main + between + self.spacing;
        }

        size
    }

    fn paint(&self, cx: &mut PaintCx<'_>, offset: Offset) {
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn intrinsic(&self, cx: &mut IntrinsicCx<'_>, axis: Axis, cross_extent: f64) -> Option<f64> {
        // On the main axis a flex is the sum of its children's intrinsic extents
        // plus the gaps between them; on the cross axis it's the largest child's.
        let children = cx.children();
        let main = self.axis == axis;
        let count = children.len() as f64;
        let gaps = if main && count > 1.0 { (count - 1.0) * self.spacing.max(0.0) } else { 0.0 };
        let mut acc = 0.0_f64;
        for child in children {
            let Some(v) = cx.child_intrinsic(child, axis, cross_extent) else {
                return None;
            };
            if main {
                acc += v;
            } else {
                acc = acc.max(v);
            }
        }
        Some(acc + gaps)
    }

    fn debug_name(&self) -> &'static str {
        "RenderFlex"
    }
}
