//! [`ListTile`] — a list row (shadcn's Item): an optional leading widget, a
//! title + optional subtitle, an optional trailing widget. Every slot takes a
//! widget, and a universal [`Style`](crate::Style) covers the surface —
//! background, border, radius, shadow, padding, size, margin, min-height, … —
//! with the Style's **text** props (color, size, weight) driving the title.
//! Rows are clickable via [`on_tap`](ListTile::on_tap) (hover feedback + pointer
//! cursor), can be marked [`selected`](ListTile::selected), made
//! [`dense`](ListTile::dense), and [`disabled`](ListTile::disabled).

use std::rc::Rc;

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::Cursor;

use crate::style::{Style, styled};
use crate::theme::{mix, theme};
use crate::widgets::{Expanded, GestureDetector, column, gap_h, gap_w, row, spacer, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, component_props, create_signal};

/// A list row. Build with [`list_tile`].
#[derive(Clone, Default)]
pub struct ListTile {
    leading: Option<AnyWidget>,
    title: String,
    subtitle: Option<String>,
    trailing: Option<AnyWidget>,
    on_tap: Option<Rc<dyn Fn()>>,
    selected: bool,
    dense: bool,
    disabled: bool,
    content_padding: Option<EdgeInsets>,
    leading_gap: f64,
    color: Option<Color>,
    selected_color: Option<Color>,
    style: Option<Style>,
}

/// Create a [`ListTile`] with a title.
pub fn list_tile(title: impl Into<String>) -> ListTile {
    ListTile { title: title.into(), leading_gap: 12.0, ..Default::default() }
}

impl ListTile {
    pub fn leading(mut self, leading: impl IntoWidget) -> Self {
        self.leading = Some(leading.into_widget());
        self
    }
    pub fn subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }
    pub fn trailing(mut self, trailing: impl IntoWidget) -> Self {
        self.trailing = Some(trailing.into_widget());
        self
    }
    /// Make the row clickable: pointer cursor, hover feedback, and `f` on tap.
    pub fn on_tap(mut self, f: impl Fn() + 'static) -> Self {
        self.on_tap = Some(Rc::new(f));
        self
    }
    /// Mark the row as selected (a tinted background, default accent-based).
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// The selected-row background tint (defaults to an accent-tinted background).
    pub fn selected_color(mut self, color: Color) -> Self {
        self.selected_color = Some(color);
        self
    }
    /// Compact vertical padding (default `(12, 10)` → `(12, 6)`).
    pub fn dense(mut self, dense: bool) -> Self {
        self.dense = dense;
        self
    }
    /// Dim the row and disable its tap (only meaningful with [`on_tap`](ListTile::on_tap)).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Override the row padding (defaults to `(12, 10)`, or `(12, 6)` dense).
    pub fn content_padding(mut self, insets: EdgeInsets) -> Self {
        self.content_padding = Some(insets);
        self
    }
    /// The gap between the leading widget and the title block (default 12).
    pub fn leading_gap(mut self, gap: f64) -> Self {
        self.leading_gap = gap;
        self
    }
    /// The row background (Flutter's `tileColor`).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// Merge a [`Style`](crate::Style) over the row: box props (background,
    /// border, radius, shadow, padding, size, margin, min-height, …) style the
    /// surface; text props (color, size, weight) style the title. User wins.
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

struct TileProps {
    leading: Option<AnyWidget>,
    title: String,
    subtitle: Option<String>,
    trailing: Option<AnyWidget>,
    on_tap: Option<Rc<dyn Fn()>>,
    selected: bool,
    dense: bool,
    disabled: bool,
    content_padding: Option<EdgeInsets>,
    leading_gap: f64,
    color: Option<Color>,
    selected_color: Option<Color>,
    style: Option<Style>,
}

impl IntoWidget for ListTile {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_tile,
            TileProps {
                leading: self.leading,
                title: self.title,
                subtitle: self.subtitle,
                trailing: self.trailing,
                on_tap: self.on_tap,
                selected: self.selected,
                dense: self.dense,
                disabled: self.disabled,
                content_padding: self.content_padding,
                leading_gap: self.leading_gap,
                color: self.color,
                selected_color: self.selected_color,
                style: self.style,
            },
        )
        .into_widget()
    }
}

fn render_tile(p: &TileProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let interactive = p.on_tap.is_some() && !p.disabled;
    let hv =
        if interactive { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) } else { 0.0 };

    // Surface: tile color → selected tint → the user Style (wins) → hover tint
    // (computed after everything, per the styling contract).
    let mut bg = p.color.unwrap_or(c.background);
    if p.selected {
        bg = mix(bg, p.selected_color.unwrap_or(c.accent), 0.14);
    }
    let default_pad = if p.dense {
        EdgeInsets::symmetric(12.0, 6.0)
    } else {
        EdgeInsets::symmetric(12.0, 10.0)
    };
    let base = crate::style::style()
        .background(bg)
        .padding(p.content_padding.unwrap_or(default_pad));
    let merged = base.merge(p.style.clone().unwrap_or_default());
    // Read the Style's text props first (merged is consumed to build the surface).
    let title_color = if p.disabled { c.muted_foreground } else { merged.color.unwrap_or(c.foreground) };
    let title_size = merged.font_size.unwrap_or(14.0);
    let title_weight = merged.font_weight.unwrap_or(500.0);
    let final_bg = mix(merged.background.unwrap_or(c.background), c.foreground, 0.05 * hv as f32);
    let surface = merged.background(final_bg);

    let mut title_col: Vec<AnyWidget> = vec![
        text(p.title.clone())
            .size(title_size)
            .weight(title_weight)
            .color(title_color)
            .into_widget(),
    ];
    if let Some(sub) = &p.subtitle {
        title_col.push(gap_h(2.0).into_widget());
        title_col.push(text(sub.clone()).size(12.0).color(c.muted_foreground).into_widget());
    }

    let mut items: Vec<AnyWidget> = Vec::new();
    if let Some(leading) = &p.leading {
        items.push(leading.clone());
        items.push(gap_w(p.leading_gap).into_widget());
    }
    items.push(
        Expanded::new(
            column(title_col).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min),
        )
        .into_widget(),
    );
    if let Some(trailing) = &p.trailing {
        items.push(trailing.clone());
    } else {
        items.push(spacer().into_widget());
    }

    let content = row(items).cross_axis_alignment(CrossAxisAlignment::Center);
    let out = styled(content, surface);

    if p.disabled {
        return GestureDetector::new(out).cursor(Cursor::NotAllowed).into_widget();
    }
    if !interactive {
        return out.into_widget();
    }

    let on_tap = p.on_tap.clone();
    let g = GestureDetector::new(out)
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false))
        .on_tap(move || {
            if let Some(f) = &on_tap {
                f();
            }
        });
    crate::widgets::semantics(crate::widgets::SemanticsRole::Button, p.title.clone(), g)
        .disabled(p.disabled)
        .into_widget()
}
