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

use std::sync::Mutex;

use accesskit::{
    Action, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, Node, NodeId,
    Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate,
};
use accesskit_winit::Adapter;
use pebbles_core::Ui;
use pebbles_render::{SemanticsNode, SemanticsRole};
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::Window;

/// The root window node id. Element ids come from `slotmap` FFI keys whose version
/// nibble is always ≥ 1, so they never collide with 0.
const ROOT: NodeId = NodeId(0);

/// The AT-driven actions we honor (D1). Other actions are dropped.
#[derive(Clone, Copy)]
enum AtAction {
    /// Move keyboard focus to the node.
    Focus,
    /// Activate the node (a screen-reader "click").
    Click,
}

/// Actions requested by assistive technology, queued off the (possibly non-UI) AT
/// thread and drained on the UI thread each frame ([`drain_actions`]). A plain
/// `Mutex<Vec<..>>` because the accesskit handler may run off-thread.
static ACTIONS: Mutex<Vec<(u64, AtAction)>> = Mutex::new(Vec::new());

fn queue_action(node: u64, action: AtAction) {
    if let Ok(mut q) = ACTIONS.lock() {
        q.push((node, action));
    }
}

/// Drain queued AT actions against `ui` (window `window`), applying each on the UI
/// thread: `Focus` moves keyboard focus; `Click` synthesizes a tap at the node's
/// published semantics-bounds center (which fires the widget's `on_pressed`/`on_tap`).
/// Returns whether anything was applied (the caller then rebuilds/repaints).
///
/// Callable **without a live adapter** — the shell drains here each frame, and tests
/// push a fabricated action then call this directly.
pub(crate) fn drain_actions(ui: &mut Ui, window: u32) -> bool {
    let pending = match ACTIONS.lock() {
        Ok(mut q) if !q.is_empty() => std::mem::take(&mut *q),
        _ => return false,
    };
    ui.make_current();
    // `semantics_tree()` returns an owned Vec, so `ui` is free to mutate in the loop.
    let nodes = ui.render_tree().semantics_tree();
    let mut acted = false;
    for (node, action) in pending {
        match action {
            AtAction::Focus => {
                if pebbles_core::focus::focus_by_source(window, node) {
                    acted = true;
                }
            }
            AtAction::Click => {
                if let Some(b) = nodes.iter().find(|n| n.id == node).map(|n| n.bounds) {
                    let center = pebbles_foundation::Offset::new(
                        (b.x0 + b.x1) / 2.0,
                        (b.y0 + b.y1) / 2.0,
                    );
                    // Mirror a real tap: down → tap → up (the shell's own sequence).
                    ui.dispatch_pointer_down(center);
                    ui.dispatch_tap(center);
                    ui.dispatch_pointer_up(center);
                    acted = true;
                }
            }
        }
    }
    acted
}

struct NoActivation;
impl ActivationHandler for NoActivation {
    // Return no initial tree — the UI thread pushes the full tree on the next frame
    // via `update_if_active`.
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        None
    }
}

struct QueueAction;
impl ActionHandler for QueueAction {
    // D1: queue the request (this may run off the UI thread) for the UI thread to
    // apply in `drain_actions`. Only Focus + Click/Default are honored; the rest are
    // dropped. SetValue (Slider/TextField) is a documented v2 follow-up.
    fn do_action(&mut self, request: ActionRequest) {
        let action = match request.action {
            Action::Focus => AtAction::Focus,
            Action::Click => AtAction::Click,
            _ => return,
        };
        queue_action(request.target.0, action);
    }
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
        let adapter = Adapter::with_direct_handlers(
            event_loop,
            window,
            NoActivation,
            QueueAction,
            NoDeactivation,
        );
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
        SemanticsRole::MenuItem => Role::MenuItem,
        SemanticsRole::Menu => Role::Menu,
        SemanticsRole::Tab => Role::Tab,
        SemanticsRole::TabList => Role::TabList,
        SemanticsRole::Dialog => Role::Dialog,
        SemanticsRole::Alert => Role::Alert,
        SemanticsRole::ProgressBar => Role::ProgressIndicator,
        SemanticsRole::List => Role::List,
        SemanticsRole::ListItem => Role::ListItem,
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use pebbles_core::{IntoWidget, component};
    use pebbles_foundation::{Size, palette};
    use pebbles_render::TextEnv;
    use pebbles_widgets::{View, button};

    thread_local! {
        static FIRED: Cell<u32> = const { Cell::new(0) };
    }

    fn app() -> impl IntoWidget {
        button("Save").on_pressed(|| FIRED.with(|f| f.set(f.get() + 1)))
    }

    #[test]
    fn at_click_fires_on_pressed_without_an_adapter() {
        pebbles_widgets::overlay::init();
        pebbles_core::focus::init();
        FIRED.with(|f| f.set(0));
        if let Ok(mut q) = ACTIONS.lock() {
            q.clear();
        }

        let mut ui = Ui::new();
        let mut env = TextEnv::new();
        ui.make_current();
        ui.mount_root(View::new(palette::WHITE, component(app)).into_widget());
        ui.layout(&mut env, Size::new(200.0, 80.0));

        // The button's semantics node id (what an AT would target).
        let node = {
            let tree = ui.render_tree().semantics_tree();
            tree.iter()
                .find(|n| n.props.role == SemanticsRole::Button)
                .expect("button node")
                .id
        };

        // Fabricate an AT "click" and drain it directly — no live adapter needed.
        queue_action(node, AtAction::Click);
        let window = ui.window_id();
        assert!(drain_actions(&mut ui, window), "the click was applied");
        assert_eq!(FIRED.with(Cell::get), 1, "on_pressed fired via the synthesized tap");
    }
}
