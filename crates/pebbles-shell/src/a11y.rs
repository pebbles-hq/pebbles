//! AccessKit platform bridge for the main window.
//!
//! Pebbles builds a platform-neutral semantics tree from the render tree (see
//! [`pebbles_render::RenderTree::semantics_tree`]); this module maps that onto an
//! [`accesskit`] tree and drives an [`accesskit_winit::Adapter`], so screen readers
//! (AT-SPI / UIA / VoiceOver) can read the UI and follow focus.
//!
//! The adapter is created with *direct* no-op handlers (activation returns no initial
//! tree; action/deactivation are ignored for v1 — read access + focus announcement is
//! the goal). The real tree is pushed from the UI thread each frame via
//! [`Bridge::update`], which is the only place UI state (which isn't `Send`) is touched.

use accesskit::{
    ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId, Rect, Role,
    Toggled, TreeId, TreeInfo, TreeUpdate,
};
use accesskit_winit::Adapter;
use pebbles_render::{SemanticsNode, SemanticsRole};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// The root window node id. Element ids come from `slotmap` FFI keys whose version
/// nibble is always ≥ 1, so they never collide with 0.
const ROOT: NodeId = NodeId(0);

struct NoActivation;
impl ActivationHandler for NoActivation {
    // Return no initial tree — the UI thread pushes the full tree on the next frame
    // via `update_if_active`.
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        None
    }
}

struct NoAction;
impl ActionHandler for NoAction {
    // v1: AT-driven actions (e.g. a screen reader "clicking" a button) are not yet
    // routed back to the UI thread. Read access + focus is the deliverable.
    fn do_action(&mut self, _request: ActionRequest) {}
}

struct NoDeactivation;
impl DeactivationHandler for NoDeactivation {
    fn deactivate_accessibility(&mut self) {}
}

/// Holds the AccessKit adapter for one window and maps Pebbles semantics onto it.
pub struct Bridge {
    adapter: Adapter,
}

impl Bridge {
    /// Create the bridge for `window`. Must be called before the window is shown.
    pub fn new(event_loop: &ActiveEventLoop, window: &Window) -> Self {
        let adapter =
            Adapter::with_direct_handlers(event_loop, window, NoActivation, NoAction, NoDeactivation);
        Bridge { adapter }
    }

    /// Forward a window event to the adapter (call before the app handles it).
    pub fn process_event(&mut self, window: &Window, event: &WindowEvent) {
        self.adapter.process_event(window, event);
    }

    /// Push the current semantics tree (built on the UI thread) to the platform.
    /// `nodes` is `RenderTree::semantics_tree()`; `focus` is the focused element id
    /// (or `None`), which must match a node id to be honored.
    pub fn update(&mut self, nodes: &[SemanticsNode], focus: Option<u64>) {
        self.adapter.update_if_active(|| build_update(nodes, focus));
    }
}

fn map_role(role: SemanticsRole) -> Role {
    match role {
        SemanticsRole::Button => Role::Button,
        SemanticsRole::Checkbox => Role::CheckBox,
        SemanticsRole::Switch => Role::Switch,
        SemanticsRole::RadioButton => Role::RadioButton,
        SemanticsRole::TextInput => Role::TextInput,
        SemanticsRole::Slider => Role::Slider,
        SemanticsRole::ComboBox => Role::ComboBox,
        SemanticsRole::Link => Role::Link,
        SemanticsRole::Image => Role::Image,
        SemanticsRole::Label => Role::Label,
        SemanticsRole::Group => Role::GenericContainer,
    }
}

/// Build a full AccessKit [`TreeUpdate`]: a root window node whose children are the
/// flat list of semantics nodes, plus focus.
fn build_update(nodes: &[SemanticsNode], focus: Option<u64>) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_children(nodes.iter().map(|n| NodeId(n.id)).collect::<Vec<_>>());

    let mut out: Vec<(NodeId, Node)> = Vec::with_capacity(nodes.len() + 1);
    out.push((ROOT, root));

    for n in nodes {
        let mut node = Node::new(map_role(n.props.role));
        if !n.props.label.is_empty() {
            node.set_label(n.props.label.clone());
        }
        if let Some(v) = &n.props.value {
            node.set_value(v.clone());
        }
        if let Some(checked) = n.props.checked {
            node.set_toggled(if checked { Toggled::True } else { Toggled::False });
        }
        if n.props.disabled {
            node.set_disabled();
        }
        let b = n.bounds;
        node.set_bounds(Rect { x0: b.x0, y0: b.y0, x1: b.x1, y1: b.y1 });
        out.push((NodeId(n.id), node));
    }

    // Focus must always be a valid node; fall back to the root window.
    let focus = focus
        .filter(|id| nodes.iter().any(|n| n.id == *id))
        .map(NodeId)
        .unwrap_or(ROOT);

    TreeUpdate { nodes: out, tree: Some(TreeInfo::new(ROOT)), tree_id: TreeId::ROOT, focus }
}
