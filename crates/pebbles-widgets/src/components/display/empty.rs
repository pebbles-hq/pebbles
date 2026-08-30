//! [`Empty`] — an empty-state placeholder (shadcn's `Empty`): a centered icon,
//! title, description and optional action.

use pebbles_foundation::EdgeInsets;
use pebbles_render::{BorderRadius, BoxDecoration, IconData};

use crate::components::icon;
use crate::theme::theme;
use crate::widgets::{Container, SizedBox, center, column, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// An empty-state block. Build with [`empty`]; all parts are optional.
pub struct Empty {
    icon: Option<IconData>,
    title: Option<String>,
    description: Option<String>,
    action: Option<AnyWidget>,
}

/// Create an [`Empty`] state.
pub fn empty() -> Empty {
    Empty { icon: None, title: None, description: None, action: None }
}

impl Empty {
    /// A glyph shown in a muted circle above the text.
    pub fn icon(mut self, icon: impl Into<IconData>) -> Self {
        self.icon = Some(icon.into());
        self
    }
    pub fn title(mut self, s: impl Into<String>) -> Self {
        self.title = Some(s.into());
        self
    }
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    /// An action widget (e.g. a button) below the text.
    pub fn action(mut self, w: impl IntoWidget) -> Self {
        self.action = Some(w.into_widget());
        self
    }
}

impl IntoWidget for Empty {
    fn into_widget(self) -> AnyWidget {
        let c = theme().colors;
        let mut items: Vec<AnyWidget> = Vec::new();
        if let Some(ic) = self.icon {
            items.push(
                Container::new()
                    .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(999.0)))
                    .padding(EdgeInsets::all(14.0))
                    .child(icon(ic).size(24.0).color(c.muted_foreground))
                    .into_widget(),
            );
            items.push(SizedBox::spacer(0.0, 14.0).into_widget());
        }
        if let Some(t) = self.title {
            items.push(text(t).size(15.0).semibold().color(c.foreground).into_widget());
        }
        if let Some(d) = self.description {
            items.push(SizedBox::spacer(0.0, 4.0).into_widget());
            items.push(text(d).size(13.0).line_height(1.4).color(c.muted_foreground).into_widget());
        }
        if let Some(a) = self.action {
            items.push(SizedBox::spacer(0.0, 16.0).into_widget());
            items.push(a);
        }
        center(
            column(items)
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Center)
                .main_axis_min(),
        )
        .into_widget()
    }
}
