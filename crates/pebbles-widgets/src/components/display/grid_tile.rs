//! [`GridTile`] + [`grid_tile_bar`] — a grid cell with an optional caption bar
//! overlaid on its top and/or bottom edge (Flutter's `GridTile` / `GridTileBar`). The
//! classic use is a photo tile with a translucent title strip.

use pebbles_foundation::{Color, EdgeInsets, MainAxisSize, palette};
use pebbles_render::{BoxDecoration, IconData};

use crate::components::icon;
use crate::widgets::{Container, column, positioned, row, spacer, stack, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A grid cell: a `child` with an optional header and/or footer bar overlaid on its
/// edges. Built by [`grid_tile`].
#[derive(Clone)]
pub struct GridTile {
    child: AnyWidget,
    header: Option<AnyWidget>,
    footer: Option<AnyWidget>,
}

/// See [`GridTile`]. Wrap `child` (usually an image or a colored box).
pub fn grid_tile(child: impl IntoWidget) -> GridTile {
    GridTile { child: child.into_widget(), header: None, footer: None }
}

impl GridTile {
    /// A bar overlaid on the top edge (usually a [`grid_tile_bar`]).
    pub fn header(mut self, header: impl IntoWidget) -> Self {
        self.header = Some(header.into_widget());
        self
    }
    /// A bar overlaid on the bottom edge (usually a [`grid_tile_bar`]).
    pub fn footer(mut self, footer: impl IntoWidget) -> Self {
        self.footer = Some(footer.into_widget());
        self
    }
}

impl IntoWidget for GridTile {
    fn into_widget(mut self) -> AnyWidget {
        let mut kids: Vec<AnyWidget> = vec![self.child.clone()];
        if let Some(h) = self.header.take() {
            kids.push(positioned(h).top(0.0).left(0.0).right(0.0).into_widget());
        }
        if let Some(f) = self.footer.take() {
            kids.push(positioned(f).bottom(0.0).left(0.0).right(0.0).into_widget());
        }
        stack(kids).into_widget()
    }
}

/// A caption bar for a [`GridTile`] — a translucent dark strip with a title, optional
/// subtitle, and optional leading/trailing widgets. Flutter's `GridTileBar`.
#[derive(Clone)]
pub struct GridTileBar {
    title: String,
    subtitle: Option<String>,
    leading: Option<AnyWidget>,
    trailing: Option<AnyWidget>,
}

/// See [`GridTileBar`].
pub fn grid_tile_bar(title: impl Into<String>) -> GridTileBar {
    GridTileBar { title: title.into(), subtitle: None, leading: None, trailing: None }
}

impl GridTileBar {
    /// A muted second line under the title.
    pub fn subtitle(mut self, s: impl Into<String>) -> Self {
        self.subtitle = Some(s.into());
        self
    }
    /// A leading widget (e.g. an [`icon`](crate::icon)).
    pub fn leading(mut self, w: impl IntoWidget) -> Self {
        self.leading = Some(w.into_widget());
        self
    }
    /// A trailing widget (e.g. an icon button).
    pub fn trailing(mut self, w: impl IntoWidget) -> Self {
        self.trailing = Some(w.into_widget());
        self
    }
    /// Convenience: a leading [`icon`](crate::icon).
    pub fn leading_icon(self, kind: impl Into<IconData>) -> Self {
        let w = icon(kind).size(18.0).color(palette::WHITE);
        self.leading(w)
    }
}

impl IntoWidget for GridTileBar {
    fn into_widget(mut self) -> AnyWidget {
        let white = palette::WHITE;
        let faint = Color::new([1.0, 1.0, 1.0, 0.75]);
        let mut titles: Vec<AnyWidget> =
            vec![text(self.title.clone()).size(13.0).semibold().color(white).into_widget()];
        if let Some(sub) = self.subtitle.take() {
            titles.push(text(sub).size(11.5).color(faint).into_widget());
        }
        let mut kids: Vec<AnyWidget> = Vec::new();
        if let Some(lead) = self.leading.take() {
            kids.push(lead);
            kids.push(crate::widgets::gap_w(10.0).into_widget());
        }
        kids.push(
            column(titles)
                .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
                .into_widget(),
        );
        kids.push(spacer().into_widget());
        if let Some(trail) = self.trailing.take() {
            kids.push(trail);
        }
        Container::new()
            .decoration(BoxDecoration::new().color(Color::from_rgba8(0, 0, 0, 140)))
            .padding(EdgeInsets::symmetric(12.0, 8.0))
            .child(row(kids).cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Center))
            .into_widget()
    }
}
