//! [`Stepper`] — a numbered step flow (Flutter's `Stepper`). Controlled: `current` is
//! a prop and tapping a step's header reports through `on_step_tapped`. Steps before
//! the current one show a check; the current step reveals its content.

use std::rc::Rc;

use pebbles_foundation::{Axis, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{BorderRadius, BoxDecoration, Cursor, IconKind};

use crate::components::icon;
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, center, column, gap_h, gap_w, row, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// One step: a title, optional subtitle, and the content shown when it is current.
pub struct Step {
    title: String,
    subtitle: Option<String>,
    content: AnyWidget,
}

/// Build a [`Step`] with a `title` and its `content`.
pub fn step(title: impl Into<String>, content: impl IntoWidget) -> Step {
    Step { title: title.into(), subtitle: None, content: content.into_widget() }
}

impl Step {
    /// A muted line under the title.
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
}

/// A numbered step flow. Built by [`stepper`]; `current` selects which step is active
/// (and whose content shows), and earlier steps render as complete.
pub struct Stepper {
    steps: Vec<Step>,
    current: usize,
    axis: Axis,
    on_step_tapped: Option<Rc<dyn Fn(usize)>>,
}

/// See [`Stepper`]. Vertical by default.
pub fn stepper(steps: Vec<Step>) -> Stepper {
    Stepper { steps, current: 0, axis: Axis::Vertical, on_step_tapped: None }
}

impl Stepper {
    /// The active step index (its content is shown; earlier steps are complete).
    pub fn current(mut self, i: usize) -> Self {
        self.current = i;
        self
    }
    /// Lay the step headers out in a horizontal row (content below).
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }
    /// Called with a step index when its header is tapped.
    pub fn on_step_tapped(mut self, f: impl Fn(usize) + 'static) -> Self {
        self.on_step_tapped = Some(Rc::new(f));
        self
    }
}

/// The numbered/checked circle for step `i` given its state relative to `current`.
fn indicator(i: usize, current: usize) -> AnyWidget {
    let c = theme().colors;
    let complete = i < current;
    let active = i == current;
    let (bg, fg) =
        if complete || active { (c.primary, c.primary_foreground) } else { (c.muted, c.muted_foreground) };
    let inner: AnyWidget = if complete {
        icon(IconKind::Check).size(15.0).color(fg).into_widget()
    } else {
        text((i + 1).to_string()).size(13.0).semibold().color(fg).into_widget()
    };
    Container::new()
        .width(28.0)
        .height(28.0)
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(999.0)))
        .child(center(inner))
        .into_widget()
}

/// A step's title + optional subtitle, made tappable if a handler is set.
fn header(s: &Step, i: usize, current: usize, on_tap: &Option<Rc<dyn Fn(usize)>>) -> AnyWidget {
    let c = theme().colors;
    let title_color = if i <= current { c.foreground } else { c.muted_foreground };
    let mut col: Vec<AnyWidget> =
        vec![text(s.title.clone()).size(14.0).semibold().color(title_color).into_widget()];
    if let Some(sub) = &s.subtitle {
        col.push(gap_h(2.0).into_widget());
        col.push(text(sub.clone()).size(12.0).color(c.muted_foreground).into_widget());
    }
    let head = column(col).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min);
    match on_tap {
        Some(f) => {
            let f = f.clone();
            GestureDetector::new(head).on_tap(move || f(i)).cursor(Cursor::Pointer).into_widget()
        }
        None => head.into_widget(),
    }
}

/// A short connector between two step indicators.
fn connector(vertical: bool, done: bool) -> AnyWidget {
    let c = theme().colors;
    let col = if done { c.primary } else { c.border };
    let b = Container::new().decoration(BoxDecoration::new().color(col));
    if vertical { b.width(2.0).height(22.0).into_widget() } else { b.height(2.0).width(40.0).into_widget() }
}

impl IntoWidget for Stepper {
    fn into_widget(self) -> AnyWidget {
        if self.axis == Axis::Horizontal { self.horizontal_layout() } else { self.vertical_layout() }
    }
}

impl Stepper {
    fn vertical_layout(self) -> AnyWidget {
        let last = self.steps.len().saturating_sub(1);
        let mut items: Vec<AnyWidget> = Vec::new();
        for (i, s) in self.steps.iter().enumerate() {
            let head_row = row(vec![
                indicator(i, self.current),
                gap_w(12.0).into_widget(),
                header(s, i, self.current, &self.on_step_tapped),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_size(MainAxisSize::Min);
            items.push(head_row.into_widget());

            if i == self.current {
                // The active step's content, indented under the header.
                items.push(gap_h(10.0).into_widget());
                items.push(
                    Container::new()
                        .padding(EdgeInsets { left: 40.0, right: 0.0, top: 0.0, bottom: 0.0 })
                        .child(s.content.clone())
                        .into_widget(),
                );
            }
            if i != last {
                items.push(
                    Container::new()
                        .padding(EdgeInsets { left: 13.0, right: 0.0, top: 8.0, bottom: 8.0 })
                        .child(connector(true, i < self.current))
                        .into_widget(),
                );
            }
        }
        column(items)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .into_widget()
    }

    fn horizontal_layout(self) -> AnyWidget {
        let last = self.steps.len().saturating_sub(1);
        let mut strip: Vec<AnyWidget> = Vec::new();
        for (i, s) in self.steps.iter().enumerate() {
            strip.push(indicator(i, self.current));
            strip.push(gap_w(8.0).into_widget());
            strip.push(header(s, i, self.current, &self.on_step_tapped));
            if i != last {
                strip.push(gap_w(12.0).into_widget());
                strip.push(connector(false, i < self.current));
                strip.push(gap_w(12.0).into_widget());
            }
        }
        let bar =
            row(strip).cross_axis_alignment(CrossAxisAlignment::Center).main_axis_size(MainAxisSize::Min);
        let content = self
            .steps
            .get(self.current)
            .map(|s| s.content.clone())
            .unwrap_or_else(|| Container::new().into_widget());
        column(vec![bar.into_widget(), gap_h(20.0).into_widget(), content])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .into_widget()
    }
}
