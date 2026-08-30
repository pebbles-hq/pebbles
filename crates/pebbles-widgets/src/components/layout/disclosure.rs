//! Disclosure components: [`Accordion`] (multiple sections) and [`Collapsible`]
//! (a single open/closed section). Controlled via `expanded` props + callbacks.

use std::rc::Rc;

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisAlignment};
use pebbles_render::IconKind;

use pebbles_core::children;
use pebbles_core::context::{BuildContext, Callback, action};
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};
use crate::widgets::{Container, GestureDetector, Padding, SizedBox, column, row, spacer, text};
use pebbles_core::{component_props, create_signal};

use crate::components::icon;

#[derive(Clone)]
struct Section {
    title: String,
    content: AnyWidget,
    expanded: bool,
    on_toggle: Option<Callback>,
}

/// A vertical stack of collapsible sections.
#[derive(Clone)]
pub struct Accordion {
    sections: Vec<Section>,
}

/// Create an empty [`Accordion`]; add sections with [`Accordion::item`].
pub fn accordion() -> Accordion {
    Accordion { sections: Vec::new() }
}

impl Accordion {
    /// Add a section.
    pub fn item(
        mut self,
        title: impl Into<String>,
        content: impl IntoWidget,
        expanded: bool,
        on_toggle: Callback,
    ) -> Self {
        self.sections.push(Section {
            title: title.into(),
            content: content.into_widget(),
            expanded,
            on_toggle: Some(on_toggle),
        });
        self
    }
}

fn section_widget(section: Section, th: crate::Theme) -> AnyWidget {
    let header = GestureDetector::new(
        Padding::new(
            EdgeInsets::symmetric(4.0, 12.0),
            row(children![
                text(section.title).size(14.0).weight(500.0).color(th.colors.foreground),
                spacer(),
                icon(if section.expanded { IconKind::ChevronUp } else { IconKind::ChevronDown })
                    .size(18.0)
                    .color(th.colors.muted_foreground),
            ])
            .main_axis_alignment(MainAxisAlignment::SpaceBetween),
        ),
    );
    let header = match section.on_toggle {
        Some(cb) => header.on_tap(cb),
        None => header,
    };

    let mut items = vec![pebbles_core::widget::IntoWidget::into_widget(header)];
    if section.expanded {
        items.push(
            Padding::new(EdgeInsets::only(4.0, 0.0, 4.0, 12.0), section.content).into_widget(),
        );
    }
    column(items).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().into_widget()
}

pebbles_core::stateless_widget!(Accordion);

impl StatelessWidget for Accordion {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let th = theme();
        let mut children_vec = Vec::new();
        let last = self.sections.len().saturating_sub(1);
        for (i, section) in std::mem::take(&mut self.sections).into_iter().enumerate() {
            children_vec.push(section_widget(section, th));
            if i != last {
                children_vec.push(Container::new().color(th.colors.border).height(1.0).into_widget());
            }
        }
        column(children_vec).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().into_widget()
    }
}

/// A single collapsible section — shadcn's `Collapsible`. Self-managing: it owns
/// its open/closed state (seed it with [`open`](Collapsible::open)); tapping the
/// header toggles it and reports through [`on_toggle`](Collapsible::on_toggle).
/// Supply a custom [`trigger`](Collapsible::trigger) to replace the default
/// title-and-chevron header entirely.
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
        trigger: None,
        open: false,
        on_toggle: None,
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
        let content = self.content.take().unwrap_or_else(|| SizedBox::spacer(0.0, 0.0).into_widget());
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
    column(items).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().into_widget()
}
