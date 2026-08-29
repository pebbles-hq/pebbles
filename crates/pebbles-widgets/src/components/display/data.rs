//! Data-display components: [`ListTile`] (a list row) and [`Table`] (a simple data
//! grid).

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets};
use pebbles_render::BoxDecoration;

use pebbles_core::context::BuildContext;
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};
use crate::widgets::{Container, Expanded, Padding, SizedBox, column, row, spacer, text};

/// A list row: optional leading widget, a title + optional subtitle, optional
/// trailing widget.
#[derive(Clone)]
pub struct ListTile {
    leading: Option<AnyWidget>,
    title: String,
    subtitle: Option<String>,
    trailing: Option<AnyWidget>,
}

/// Create a [`ListTile`] with a title.
pub fn list_tile(title: impl Into<String>) -> ListTile {
    ListTile { leading: None, title: title.into(), subtitle: None, trailing: None }
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
}

pebbles_core::stateless_widget!(ListTile);

impl StatelessWidget for ListTile {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let th = theme();
        let mut title_col = vec![
            text(std::mem::take(&mut self.title)).size(14.0).weight(500.0).color(th.colors.foreground).into_widget(),
        ];
        if let Some(sub) = self.subtitle.take() {
            title_col.push(SizedBox::spacer(0.0, 2.0).into_widget());
            title_col.push(text(sub).size(12.0).color(th.colors.muted_foreground).into_widget());
        }

        let mut items: Vec<AnyWidget> = Vec::new();
        if let Some(leading) = self.leading.take() {
            items.push(leading);
            items.push(SizedBox::spacer(12.0, 0.0).into_widget());
        }
        items.push(
            Expanded::new(
                column(title_col).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min(),
            )
            .into_widget(),
        );
        if let Some(trailing) = self.trailing.take() {
            items.push(trailing);
        } else {
            items.push(spacer().into_widget());
        }

        Padding::new(
            EdgeInsets::symmetric(12.0, 10.0),
            row(items).cross_axis_alignment(CrossAxisAlignment::Center),
        )
        .into_widget()
    }
}

/// A simple table: a header row plus data rows of string cells.
#[derive(Clone)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

/// Create a [`Table`] with column headers.
pub fn table(headers: Vec<String>) -> Table {
    Table { headers, rows: Vec::new() }
}

impl Table {
    /// Append a data row (cells matched to headers by position).
    pub fn row(mut self, cells: Vec<String>) -> Self {
        self.rows.push(cells);
        self
    }
}

pebbles_core::stateless_widget!(Table);

impl StatelessWidget for Table {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let th = theme();

        let make_row = |cells: Vec<AnyWidget>| -> AnyWidget {
            let wrapped: Vec<AnyWidget> =
                cells.into_iter().map(|c| Expanded::new(c).into_widget()).collect();
            Padding::new(
                EdgeInsets::symmetric(12.0, 10.0),
                row(wrapped).cross_axis_alignment(CrossAxisAlignment::Center),
            )
            .into_widget()
        };

        let mut body = Vec::new();

        // Header.
        let header_cells: Vec<AnyWidget> = std::mem::take(&mut self.headers)
            .into_iter()
            .map(|h| text(h).size(12.0).semibold().color(th.colors.muted_foreground).into_widget())
            .collect();
        body.push(
            Container::new()
                .decoration(BoxDecoration::new().color(th.colors.muted))
                .child(make_row(header_cells))
                .into_widget(),
        );

        // Data rows with separators.
        for cells in std::mem::take(&mut self.rows) {
            body.push(Container::new().color(th.colors.border).height(1.0).into_widget());
            let widgets: Vec<AnyWidget> = cells
                .into_iter()
                .map(|c| text(c).size(13.0).color(th.colors.foreground).into_widget())
                .collect();
            body.push(make_row(widgets));
        }

        column(body).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().into_widget()
    }
}
