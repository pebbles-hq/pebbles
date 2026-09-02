//! [`Chip`] — a compact tag/entity pill with an optional leading icon and an
//! optional **delete** affordance. Flutter's `Chip` (deletable flavor): the
//! standard token widget for filters, tags, selected options and contact chips.
//! Built on the Badge surface with an `IconButton` ✕ that fires `on_deleted`.

use pebbles_render::{IconData, IconKind};

use pebbles_core::context::Callback;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use crate::components::input::icon_button;
use crate::components::icon;
use crate::style::{Style, style, styled};
use crate::theme::theme;
use crate::widgets::{gap_w, row, text};

/// A small pill with a label, an optional icon, and an optional ✕ delete button.
#[derive(Clone, Default)]
pub struct Chip {
    label: String,
    icon_kind: Option<IconData>,
    deletable: bool,
    disabled: bool,
    on_deleted: Option<Callback>,
    style: Option<Style>,
}

/// Create a [`Chip`] with a text label.
pub fn chip(label: impl Into<String>) -> Chip {
    Chip { label: label.into(), ..Default::default() }
}

impl Chip {
    /// A leading icon.
    pub fn icon(mut self, kind: impl Into<IconData>) -> Self {
        self.icon_kind = Some(kind.into());
        self
    }
    /// Show the ✕ delete affordance (fires `on_deleted` when pressed).
    pub fn deletable(mut self, deletable: bool) -> Self {
        self.deletable = deletable;
        self
    }
    /// Disable the chip (muted presentation, no delete affordance).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Called when the ✕ is pressed. The chip itself is NOT removed from the tree
    /// — the owner drops it from its list (controlled removal, like every value
    /// in the catalog).
    pub fn on_deleted(mut self, cb: impl Fn() + 'static) -> Self {
        self.on_deleted = Some(pebbles_core::action(cb));
        self
    }
    /// Merge a [`Style`] onto the pill's base presentation (user fields win).
    pub fn style(mut self, s: Style) -> Self {
        self.style = Some(s);
        self
    }
}

impl IntoWidget for Chip {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let (bg, fg, border) = if self.disabled {
            (Some(c.muted), c.muted_foreground, false)
        } else {
            (Some(c.secondary), c.secondary_foreground, true)
        };
        let mut base = style().radius_all(999.0).padding_xy(10.0, 4.0);
        if let Some(bg) = bg {
            base = base.background(bg);
        }
        if border {
            base = base.border(pebbles_render::Border::new(c.border, 1.0));
        }
        let mut row_items: Vec<AnyWidget> = Vec::new();
        if let Some(kind) = self.icon_kind.take() {
            row_items.push(icon(kind).size(14.0).color(fg).into_widget());
            row_items.push(gap_w(6.0).into_widget());
        }
        row_items.push(
            text(std::mem::take(&mut self.label))
                .size(12.5)
                .weight(500.0)
                .color(fg)
                .into_widget(),
        );
        let mut body: AnyWidget = row(row_items)
            .main_axis_size(pebbles_foundation::MainAxisSize::Min)
            .into_widget();
        if self.deletable && !self.disabled {
            if let Some(on_deleted) = self.on_deleted.take() {
                let close = icon_button(IconKind::Close).size(12.0).on_pressed(on_deleted);
                let close_styled = styled(close, style().padding_all(2.0).radius_all(999.0));
                body = row(vec![body, gap_w(6.0).into_widget(), close_styled.into_widget()])
                    .main_axis_size(pebbles_foundation::MainAxisSize::Min)
                    .into_widget();
            }
        }
        styled(body, base.merge(self.style.take().unwrap_or_default()))
    }
}
