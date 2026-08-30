//! [`TreeView`] — the desktop tree / file-explorer widget. Nodes are **controlled**:
//! each carries its own `expanded`/`selected` flags and callbacks, so the developer
//! rebuilds the tree from their own state.

use pebbles_core::IntoCallback;
use pebbles_foundation::palette;
use pebbles_render::{IconData, IconKind};

use pebbles_core::context::{BuildContext, Callback};
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};
use crate::widgets::{Container, GestureDetector, Padding, SizedBox, column, row, text};

use crate::components::icon;

/// One node in a [`TreeView`].
#[derive(Clone, Default)]
pub struct TreeNode {
    label: String,
    icon: Option<IconData>,
    children: Vec<TreeNode>,
    expanded: bool,
    selected: bool,
    on_toggle: Option<Callback>,
    on_select: Option<Callback>,
}

/// Create a [`TreeNode`] with a label.
pub fn tree_node(label: impl Into<String>) -> TreeNode {
    TreeNode { label: label.into(), ..Default::default() }
}

impl TreeNode {
    pub fn icon(mut self, kind: impl Into<IconData>) -> Self {
        self.icon = Some(kind.into());
        self
    }
    pub fn children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Fired when a node with children is toggled.
    pub fn on_toggle(mut self, cb: impl IntoCallback) -> Self {
        self.on_toggle = Some(cb.into_callback());
        self
    }
    /// Fired when a node is selected.
    pub fn on_select(mut self, cb: impl IntoCallback) -> Self {
        self.on_select = Some(cb.into_callback());
        self
    }
}

/// A hierarchical tree / file explorer.
#[derive(Clone)]
pub struct TreeView {
    roots: Vec<TreeNode>,
    indent: f64,
}

/// Create a [`TreeView`] from root nodes.
pub fn tree_view(roots: Vec<TreeNode>) -> TreeView {
    TreeView { roots, indent: 16.0 }
}

impl TreeView {
    /// Set the per-level indentation (default 16px).
    pub fn indent(mut self, indent: f64) -> Self {
        self.indent = indent;
        self
    }
}

fn emit(node: TreeNode, depth: f64, indent: f64, out: &mut Vec<AnyWidget>) {
    let th = theme();
    let c = th.colors;
    let has_children = !node.children.is_empty();

    let mut cells: Vec<AnyWidget> = Vec::new();
    cells.push(SizedBox::spacer(depth * indent, 0.0).into_widget());
    if has_children {
        let chevron = if node.expanded { IconKind::ChevronDown } else { IconKind::ChevronRight };
        cells.push(icon(chevron).size(16.0).color(c.muted_foreground).into_widget());
    } else {
        cells.push(SizedBox::spacer(16.0, 0.0).into_widget());
    }
    cells.push(SizedBox::spacer(4.0, 0.0).into_widget());
    if let Some(kind) = node.icon {
        cells.push(icon(kind).size(16.0).color(c.muted_foreground).into_widget());
        cells.push(SizedBox::spacer(6.0, 0.0).into_widget());
    }
    let text_color = if node.selected { c.accent_foreground } else { c.foreground };
    cells.push(text(node.label).size(13.0).color(text_color).into_widget());

    // Row is left-packed but fills the width (default MainAxisSize::Max) so the
    // selection highlight spans the whole row.
    let content = Padding::new(
        pebbles_foundation::EdgeInsets::symmetric(6.0, 5.0),
        row(cells),
    );
    let bg = if node.selected { c.accent } else { palette::TRANSPARENT };
    let mut gesture = GestureDetector::new(Container::new().color(bg).child(content));
    if has_children {
        if let Some(cb) = node.on_toggle {
            gesture = gesture.on_tap(cb);
        }
    } else if let Some(cb) = node.on_select {
        gesture = gesture.on_tap(cb);
    }
    out.push(gesture.into_widget());

    if node.expanded {
        for child in node.children {
            emit(child, depth + 1.0, indent, out);
        }
    }
}

pebbles_core::stateless_widget!(TreeView);

impl StatelessWidget for TreeView {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let indent = self.indent;
        let mut rows = Vec::new();
        for node in std::mem::take(&mut self.roots) {
            emit(node, 0.0, indent, &mut rows);
        }
        column(rows)
            .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Stretch)
            .main_axis_min()
            .into_widget()
    }
}
