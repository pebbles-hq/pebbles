//! [`draggable_scrollable_sheet`] — a bottom-anchored panel whose height is a
//! fraction of the available space, resized by dragging its top handle and
//! (optionally) snapping to a set of stops. Its content lives in a scroll view, so
//! it scrolls once it overflows. Flutter's `DraggableScrollableSheet`.
//!
//! Coordinated *content*-drag (dragging the list itself past the top to grow the
//! sheet, à la Flutter's shared `ScrollController`) is a follow-up; here the top
//! **handle** is the resize affordance and the body scrolls independently.

use std::rc::Rc;

use pebbles_foundation::{Alignment, CrossAxisAlignment, EdgeInsets, MainAxisSize, Size};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor};

use crate::theme::theme;
use crate::widgets::{Container, Expanded, GestureDetector, column, layout_builder, scroll_view, spacer};
use pebbles_core::context::{action, action_event};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{component_props, create_signal};

/// A draggable, resizable bottom sheet. Build with [`draggable_scrollable_sheet`].
#[derive(Clone)]
pub struct DraggableScrollableSheet {
    content: AnyWidget,
    initial: f64,
    min: f64,
    max: f64,
    snaps: Vec<f64>,
}

/// Create a [`DraggableScrollableSheet`] around `content`. Sizes are fractions of
/// the available height (`0.0..=1.0`): initial `0.5`, min `0.25`, max `1.0`.
pub fn draggable_scrollable_sheet(content: impl IntoWidget) -> DraggableScrollableSheet {
    DraggableScrollableSheet {
        content: content.into_widget(),
        initial: 0.5,
        min: 0.25,
        max: 1.0,
        snaps: Vec::new(),
    }
}

impl DraggableScrollableSheet {
    /// The starting height fraction (default `0.5`).
    pub fn initial(mut self, f: f64) -> Self {
        self.initial = f.clamp(0.0, 1.0);
        self
    }
    /// The smallest height fraction the drag allows (default `0.25`).
    pub fn min(mut self, f: f64) -> Self {
        self.min = f.clamp(0.0, 1.0);
        self
    }
    /// The largest height fraction the drag allows (default `1.0`).
    pub fn max(mut self, f: f64) -> Self {
        self.max = f.clamp(0.0, 1.0);
        self
    }
    /// Snap stops (fractions) the sheet settles to on release. Empty = free resize.
    pub fn snap(mut self, stops: impl IntoIterator<Item = f64>) -> Self {
        self.snaps = stops.into_iter().map(|f| f.clamp(0.0, 1.0)).collect();
        self
    }
}

struct Props {
    content: AnyWidget,
    initial: f64,
    min: f64,
    max: f64,
    snaps: Rc<Vec<f64>>,
}

impl IntoWidget for DraggableScrollableSheet {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render,
            Props {
                content: self.content,
                initial: self.initial,
                min: self.min.min(self.max),
                max: self.max.max(self.min),
                snaps: Rc::new(self.snaps),
            },
        )
        .into_widget()
    }
}

/// The stop nearest `v` (or `v` itself when there are none).
fn nearest(v: f64, stops: &[f64]) -> f64 {
    stops.iter().copied().min_by(|a, b| (a - v).abs().total_cmp(&(b - v).abs())).unwrap_or(v)
}

fn render(p: &Props) -> AnyWidget {
    let c = theme().colors;
    // Height fraction (controlled by the drag) and the drag anchor (start-y, start-frac).
    let frac = create_signal(p.initial.clamp(p.min, p.max));
    let anchor = create_signal::<Option<(f64, f64)>>(None);
    let (min, max) = (p.min, p.max);
    let snaps = p.snaps.clone();
    let content = p.content.clone();

    layout_builder(move |size: Size| {
        let avail = size.height.max(1.0);
        let h = (frac.get() * avail).max(0.0);

        // The grab handle: a centered pill in a tall-enough hit strip.
        let pill = Container::new()
            .decoration(BoxDecoration::new().color(c.muted_foreground).radius(BorderRadius::all(999.0)))
            .width(40.0)
            .height(4.0);
        let handle_strip = Container::new()
            .alignment(Alignment::CENTER)
            .padding(EdgeInsets::symmetric(0.0, 10.0))
            .child(pill);

        let start = action_event(move |e| anchor.set(Some((e.global.y, frac.peek()))));
        let update = action_event(move |e| {
            if let Some((start_y, start_frac)) = anchor.peek() {
                // Drag up (negative dy) grows the sheet.
                let dy = e.global.y - start_y;
                frac.set((start_frac - dy / avail).clamp(min, max));
            }
        });
        let snaps2 = snaps.clone();
        let end = action(move || {
            if !snaps2.is_empty() {
                frac.set(nearest(frac.peek(), &snaps2).clamp(min, max));
            }
            anchor.set(None);
        });
        let handle = GestureDetector::new(handle_strip)
            .cursor(Cursor::Pointer)
            .on_pan_start(start)
            .on_pan_update(update)
            .on_pan_end(end);

        let panel = Container::new()
            .decoration(BoxDecoration::new().color(c.card).border(Border::new(c.border, 1.0)).radius(
                BorderRadius { top_left: 16.0, top_right: 16.0, bottom_right: 0.0, bottom_left: 0.0 },
            ))
            .height(h)
            .clip()
            .child(
                column(vec![handle.into_widget(), Expanded::new(scroll_view(content.clone())).into_widget()])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Max),
            );

        // Push the panel to the bottom of the available space.
        Container::new().height(avail).child(
            column(vec![spacer().into_widget(), panel.into_widget()])
                .cross_axis_alignment(CrossAxisAlignment::Stretch),
        )
    })
    .into_widget()
}
