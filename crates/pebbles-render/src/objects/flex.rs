//! [`RenderFlex`] — the workhorse behind `Row` and `Column`. Implements Flutter's
//! two-pass flex layout: lay out inflexible children first, distribute the leftover
//! main-axis space to flexible children by their flex factor, then position
//! everything per the main- and cross-axis alignment.

use pebbles_foundation::{
    Axis, CrossAxisAlignment, FlexFit, MainAxisAlignment, MainAxisSize, Offset, Size,
};

use crate::RenderId;
use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

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
}

impl RenderFlex {
    pub fn new(
        axis: Axis,
        main_axis_alignment: MainAxisAlignment,
        cross_axis_alignment: CrossAxisAlignment,
        main_axis_size: MainAxisSize,
        spacing: f64,
    ) -> Self {
        RenderFlex { axis, main_axis_alignment, cross_axis_alignment, main_axis_size, spacing }
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
    fn layout(&mut self, cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
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

        // Position children along the main axis per the alignment.
        let free = (final_main - content_main).max(0.0);
        let (leading, between) = match self.main_axis_alignment {
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

        let mut pos = leading;
        for &(child, _) in &children {
            let child_size = cx.child_size(child);
            let child_main = self.main_of(child_size);
            let child_cross = self.cross_of(child_size);
            let cross_pos = match self.cross_axis_alignment {
                CrossAxisAlignment::Start | CrossAxisAlignment::Stretch => 0.0,
                CrossAxisAlignment::End => final_cross - child_cross,
                CrossAxisAlignment::Center | CrossAxisAlignment::Baseline => {
                    (final_cross - child_cross) / 2.0
                }
            };
            cx.set_child_offset(child, self.make_offset(pos, cross_pos));
            pos += child_main + between + self.spacing;
        }

        size
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        for child in cx.children() {
            cx.paint_child(child, offset + cx.child_offset(child));
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderFlex"
    }
}
