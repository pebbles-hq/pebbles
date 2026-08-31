//! [`ToggleGroup`] — a set of [`Toggle`](super::Toggle) cells with single- or
//! multi-select behavior. Controlled: the selected indices come in via `.value`/
//! `.values`, and changes are reported through `.on_changed(&[usize])`.
//!
//! ```ignore
//! toggle_group_labels(["Left", "Center", "Right"])
//!     .value(align)                         // single-select (default)
//!     .on_changed(move |sel| set_align(sel[0]))
//! ```

use std::rc::Rc;
use pebbles_foundation::{MainAxisSize};

use pebbles_foundation::CrossAxisAlignment;
use pebbles_render::{Border, BorderRadius, BoxDecoration};

use crate::components::{ToggleSize, ToggleVariant, toggle};
use crate::theme::theme;
use crate::widgets::{Container, row, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A joined set of toggle cells. Build with [`toggle_group`] / [`toggle_group_labels`].
pub struct ToggleGroup {
    cells: Vec<AnyWidget>,
    values: Vec<usize>,
    multiple: bool,
    allow_empty: bool,
    variant: ToggleVariant,
    size: ToggleSize,
    disabled: bool,
    spacing: f64,
    on_changed: Option<Rc<dyn Fn(&[usize])>>,
}

fn make(cells: Vec<AnyWidget>) -> ToggleGroup {
    ToggleGroup {
        cells,
        values: Vec::new(),
        multiple: false,
        allow_empty: false,
        variant: ToggleVariant::Outline,
        size: ToggleSize::default(),
        disabled: false,
        spacing: 0.0,
        on_changed: None,
    }
}

/// Create an empty [`ToggleGroup`]; add cells with [`item`](ToggleGroup::item), e.g.
/// icons. For text labels use [`toggle_group_labels`].
pub fn toggle_group() -> ToggleGroup {
    make(Vec::new())
}

/// Create a [`ToggleGroup`] whose cells are text labels.
pub fn toggle_group_labels<I, S>(labels: I) -> ToggleGroup
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    make(labels.into_iter().map(|l| text(l.into()).size(13.0).into_widget()).collect())
}

impl ToggleGroup {
    /// Append a cell widget (e.g. an icon). The house `.item()` builder pattern.
    pub fn item(mut self, cell: impl IntoWidget) -> Self {
        self.cells.push(cell.into_widget());
        self
    }
    /// Allow multiple cells selected at once (default: single-select).
    pub fn multiple(mut self, multiple: bool) -> Self {
        self.multiple = multiple;
        self
    }
    /// In single-select mode, allow clicking the selected cell to clear it (default
    /// `false` — a selection is always kept).
    pub fn allow_empty(mut self, allow_empty: bool) -> Self {
        self.allow_empty = allow_empty;
        self
    }
    /// The selected index (single-select convenience).
    pub fn value(mut self, index: usize) -> Self {
        self.values = vec![index];
        self
    }
    /// The selected indices (multi-select).
    pub fn values(mut self, values: impl Into<Vec<usize>>) -> Self {
        self.values = values.into();
        self
    }
    pub fn variant(mut self, variant: ToggleVariant) -> Self {
        self.variant = variant;
        self
    }
    pub fn size(mut self, size: ToggleSize) -> Self {
        self.size = size;
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    pub fn spacing(mut self, spacing: f64) -> Self {
        self.spacing = spacing;
        self
    }
    /// Reports the new selection whenever a cell is toggled.
    pub fn on_changed(mut self, f: impl Fn(&[usize]) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }

    /// Compute the selection that results from toggling cell `i`.
    fn next_selection(&self, i: usize) -> Vec<usize> {
        let selected = self.values.contains(&i);
        if self.multiple {
            let mut next: Vec<usize> = self.values.iter().copied().filter(|&v| v != i).collect();
            if !selected {
                next.push(i);
                next.sort_unstable();
            }
            next
        } else if selected {
            if self.allow_empty { Vec::new() } else { vec![i] }
        } else {
            vec![i]
        }
    }
}

impl IntoWidget for ToggleGroup {
    fn into_widget(mut self) -> AnyWidget {
        let th = theme();
        let cells = std::mem::take(&mut self.cells);
        let group = Rc::new(self);
        let n = cells.len();

        let cell_widget = |i: usize, cell: AnyWidget, radius: f64| -> AnyWidget {
            let pressed = group.values.contains(&i);
            let g = group.clone();
            let mut t = toggle(pressed, cell)
                .variant(group.variant)
                .size(group.size)
                .disabled(group.disabled)
                .radius(radius);
            if let Some(cb) = group.on_changed.clone() {
                t = t.on_changed(move || {
                    let next = g.next_selection(i);
                    cb(&next);
                });
            }
            t.into_widget()
        };

        if group.spacing > 0.0 {
            // Detached cells: each keeps its own shape, just spaced apart.
            let mut out: Vec<AnyWidget> = Vec::with_capacity(n);
            for (i, cell) in cells.into_iter().enumerate() {
                out.push(cell_widget(i, cell, th.radius));
            }
            return row(out).spacing(group.spacing).main_axis_size(MainAxisSize::Min).into_widget();
        }

        // Joined segmented strip (the shadcn look): flatten every cell's radius,
        // divide with a hairline, and clip the whole strip to a rounded frame.
        let mut kids: Vec<AnyWidget> = Vec::with_capacity(n * 2);
        for (i, cell) in cells.into_iter().enumerate() {
            kids.push(cell_widget(i, cell, 0.0));
            if i + 1 < n {
                kids.push(Container::new().color(th.colors.border).width(1.0).into_widget());
            }
        }
        Container::new()
            .decoration(
                BoxDecoration::new()
                    .border(Border::new(th.colors.border, 1.0))
                    .radius(BorderRadius::all(th.radius)),
            )
            .clip()
            .child(
                row(kids)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
            )
            .into_widget()
    }
}
