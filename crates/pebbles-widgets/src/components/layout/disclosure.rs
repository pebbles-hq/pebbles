//! Disclosure components: [`Accordion`] (multiple sections) and [`Collapsible`]
//! (a single open/closed section). Both are **self-managing** function components:
//! they own their open/closed state (seed it, then read toggles through a
//! callback); the chevron rotates with a tween and headers give hover feedback.

use std::rc::Rc;

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize};
use pebbles_render::IconKind;

use pebbles_core::children;
use pebbles_core::context::action;
use crate::theme::{mix, theme};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::widgets::{
    Container, GestureDetector, Padding, Transform, column, gap_h, row, spacer, text,
};
use pebbles_core::{animated, component_props, create_signal};

use crate::components::icon;

#[derive(Clone)]
struct Section {
    title: String,
    content: AnyWidget,
}

/// A vertical stack of collapsible sections. Self-managing: each section's
/// open/closed state lives inside the component (seed it with
/// [`default_open`](Accordion::default_open)); toggles are reported through
/// [`on_toggle`](Accordion::on_toggle).
#[derive(Clone, Default)]
pub struct Accordion {
    sections: Vec<Section>,
    multiple: bool,
    default_open: Vec<usize>,
    on_toggle: Option<Rc<dyn Fn(usize, bool)>>,
}

/// Create an empty [`Accordion`]; add sections with [`Accordion::item`].
pub fn accordion() -> Accordion {
    Accordion::default()
}

impl Accordion {
    /// Add a section (closed by default — see
    /// [`default_open`](Accordion::default_open)).
    pub fn item(mut self, title: impl Into<String>, content: impl IntoWidget) -> Self {
        self.sections.push(Section { title: title.into(), content: content.into_widget() });
        self
    }
    /// Allow several sections open at once (default `false` — opening one closes
    /// the others).
    pub fn multiple(mut self, yes: bool) -> Self {
        self.multiple = yes;
        self
    }
    /// Start section `index` open (call once per section).
    pub fn default_open(mut self, index: usize) -> Self {
        self.default_open.push(index);
        self
    }
    /// Reports every toggle with the section index and its new open state.
    pub fn on_toggle(mut self, f: impl Fn(usize, bool) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}

/// Props for one accordion section.
struct SectionProps {
    title: String,
    content: AnyWidget,
    open: pebbles_core::Signal<Vec<bool>>,
    index: usize,
    multiple: bool,
    on_toggle: Option<Rc<dyn Fn(usize, bool)>>,
}

/// One section: a hover-highlighting header with a tweening chevron, and the
/// content when open.
fn render_section(p: &SectionProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let is_open = p.open.get()[p.index];
    let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
    let rot = animated(if is_open { 1.0 } else { 0.0 }, 0.18);

    let open = p.open;
    let index = p.index;
    let multiple = p.multiple;
    let on = p.on_toggle.clone();
    let toggle = action(move || {
        open.update(|v| {
            if multiple {
                v[index] = !v[index];
            } else {
                let was = v[index];
                v.fill(false);
                v[index] = !was;
            }
        });
        if let Some(f) = &on {
            f(index, open.get()[index]);
        }
    });

    let header = GestureDetector::new(
        Container::new()
            .color(mix(c.background, c.muted, 0.5 * hv as f32))
            .child(
                Padding::new(
                    EdgeInsets::symmetric(4.0, 12.0),
                    row(children![
                        text(p.title.clone()).size(14.0).weight(500.0).color(c.foreground),
                        spacer(),
                        Transform::rotate(rot * std::f64::consts::PI, icon(IconKind::ChevronDown).size(18.0).color(c.muted_foreground)),
                    ])
                    .main_axis_alignment(MainAxisAlignment::SpaceBetween),
                ),
            ),
    )
    .cursor(pebbles_render::Cursor::Pointer)
    .on_hover_enter(move || hovered.set(true))
    .on_hover_exit(move || hovered.set(false))
    .on_tap(toggle);

    let mut items: Vec<AnyWidget> = vec![header.into_widget()];
    if is_open {
        items.push(Padding::new(EdgeInsets::only(4.0, 0.0, 4.0, 12.0), p.content.clone()).into_widget());
    }
    column(items).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).into_widget()
}

impl IntoWidget for Accordion {
    fn into_widget(self) -> AnyWidget {
        component_props(render_accordion, self).into_widget()
    }
}

fn render_accordion(p: &Accordion) -> AnyWidget {
    let c = theme().colors;
    let open = create_signal({
        let mut v = vec![false; p.sections.len()];
        for &i in &p.default_open {
            if i < v.len() {
                v[i] = true;
            }
        }
        v
    });

    let mut children_vec = Vec::new();
    let last = p.sections.len().saturating_sub(1);
    for (i, section) in p.sections.iter().enumerate() {
        children_vec.push(
            component_props(
                render_section,
                SectionProps {
                    title: section.title.clone(),
                    content: section.content.clone(),
                    open,
                    index: i,
                    multiple: p.multiple,
                    on_toggle: p.on_toggle.clone(),
                },
            )
            .into_widget(),
        );
        if i != last {
            children_vec.push(Container::new().color(c.border).height(1.0).into_widget());
        }
    }
    column(children_vec).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).into_widget()
}

/// A single collapsible section — shadcn's `Collapsible`. Self-managing: it owns
/// its open/closed state (seed it with [`open`](Collapsible::open)); tapping the
/// header toggles it and reports through [`on_toggle`](Collapsible::on_toggle).
/// Supply a custom [`trigger`](Collapsible::trigger) to replace the default
/// title-and-chevron header entirely.
#[derive(Clone, Default)]
pub struct Collapsible {
    title: String,
    content: Option<AnyWidget>,
    trigger: Option<AnyWidget>,
    open: bool,
    on_toggle: Option<Rc<dyn Fn(bool)>>,
}

/// Create a [`Collapsible`] with a title and content (closed by default).
pub fn collapsible(title: impl Into<String>, content: impl IntoWidget) -> Collapsible {
    Collapsible {
        title: title.into(),
        content: Some(content.into_widget()),
        ..Default::default()
    }
}

impl Collapsible {
    /// Whether it starts open (default `false`).
    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }
    /// A custom header widget, replacing the default title + chevron row.
    pub fn trigger(mut self, trigger: impl IntoWidget) -> Self {
        self.trigger = Some(trigger.into_widget());
        self
    }
    /// Called with the new open state on each toggle.
    pub fn on_toggle(mut self, f: impl Fn(bool) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(f));
        self
    }
}

struct CollapsibleProps {
    title: String,
    content: AnyWidget,
    trigger: Option<AnyWidget>,
    open: bool,
    on_toggle: Option<Rc<dyn Fn(bool)>>,
}

impl IntoWidget for Collapsible {
    fn into_widget(mut self) -> AnyWidget {
        let content = self.content.take().unwrap_or_else(|| gap_h(0.0).into_widget());
        component_props(
            render_collapsible,
            CollapsibleProps {
                title: self.title,
                content,
                trigger: self.trigger,
                open: self.open,
                on_toggle: self.on_toggle,
            },
        )
        .into_widget()
    }
}

fn render_collapsible(p: &CollapsibleProps) -> AnyWidget {
    let c = theme().colors;
    let open = create_signal(p.open);
    let is_open = open.get();
    let on = p.on_toggle.clone();
    let toggle = action(move || {
        open.update(|o| *o = !*o);
        if let Some(f) = &on {
            f(open.peek());
        }
    });

    let header_inner: AnyWidget = match &p.trigger {
        Some(t) => t.clone(),
        None => Padding::new(
            EdgeInsets::symmetric(4.0, 12.0),
            row(children![
                text(p.title.clone()).size(14.0).weight(500.0).color(c.foreground),
                spacer(),
                icon(if is_open { IconKind::ChevronUp } else { IconKind::ChevronDown })
                    .size(18.0)
                    .color(c.muted_foreground),
            ])
            .main_axis_alignment(MainAxisAlignment::SpaceBetween),
        )
        .into_widget(),
    };
    let header = GestureDetector::new(header_inner)
        .cursor(pebbles_render::Cursor::Pointer)
        .on_tap(toggle);

    let mut items: Vec<AnyWidget> = vec![header.into_widget()];
    if is_open {
        items.push(Padding::new(EdgeInsets::only(4.0, 0.0, 4.0, 12.0), p.content.clone()).into_widget());
    }
    column(items).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).into_widget()
}
