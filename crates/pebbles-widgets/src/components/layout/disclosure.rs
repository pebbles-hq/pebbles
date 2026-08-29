//! Disclosure components: [`Accordion`] (multiple sections) and [`Collapsible`]
//! (a single open/closed section). Controlled via `expanded` props + callbacks.

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisAlignment};
use pebbles_render::IconKind;

use pebbles_core::children;
use pebbles_core::context::{BuildContext, Callback};
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};
use crate::widgets::{Container, GestureDetector, Padding, SizedBox, column, row, spacer, text};

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

/// A single collapsible section.
#[derive(Clone)]
pub struct Collapsible {
    title: String,
    content: AnyWidget,
    open: bool,
    on_toggle: Option<Callback>,
}

/// Create a [`Collapsible`].
pub fn collapsible(
    title: impl Into<String>,
    content: impl IntoWidget,
    open: bool,
    on_toggle: Callback,
) -> Collapsible {
    Collapsible {
        title: title.into(),
        content: content.into_widget(),
        open,
        on_toggle: Some(on_toggle),
    }
}

pebbles_core::stateless_widget!(Collapsible);

impl StatelessWidget for Collapsible {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let th = theme();
        let content = std::mem::replace(&mut self.content, SizedBox::spacer(0.0, 0.0).into_widget());
        section_widget(
            Section {
                title: std::mem::take(&mut self.title),
                content,
                expanded: self.open,
                on_toggle: self.on_toggle.take(),
            },
            th,
        )
    }
}
