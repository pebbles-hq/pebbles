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

use crate::components::{ToggleSize, ToggleVariant, toggle};
use crate::widgets::{row, text};
use pebbles_core::widget::{AnyWidget, IntoChildren, IntoWidget};

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
        spacing: 6.0,
        on_changed: None,
    }
}

/// Create a [`ToggleGroup`] from arbitrary cell widgets (e.g. icons).
pub fn toggle_group(cells: impl IntoChildren) -> ToggleGroup {
    make(cells.into_children())
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
        let cells = std::mem::take(&mut self.cells);
        let group = Rc::new(self);
        let mut out: Vec<AnyWidget> = Vec::with_capacity(cells.len());
        for (i, cell) in cells.into_iter().enumerate() {
            let pressed = group.values.contains(&i);
            let g = group.clone();
            let mut t = toggle(pressed, cell)
                .variant(group.variant)
                .size(group.size)
                .disabled(group.disabled);
            if let Some(cb) = group.on_changed.clone() {
                t = t.on_changed(move || {
                    let next = g.next_selection(i);
                    cb(&next);
                });
            }
            out.push(t.into_widget());
        }
        row(out).spacing(group.spacing).main_axis_min().into_widget()
    }
}
