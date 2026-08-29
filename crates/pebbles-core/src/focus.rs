//! The focus system: a [`FocusNode`] per focusable widget, a single primary-focus
//! target, keyboard **activation** (Enter/Space), **Tab traversal**, and
//! **onFocusChange** callbacks.
//!
//! Focus is reactive: the current target lives in a global [`Signal`], so a widget
//! reading [`FocusNode::is_focused`] re-renders (e.g. to show a focus ring) when
//! focus moves. A node's identity is its component's element id, so it is stable
//! across re-renders.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::element::ElementId;
use crate::keyboard::KeyInput;
use crate::reactive::{Signal, create_signal, current_owner};

struct FocusManager {
    /// Reactive source of truth for the currently-focused node.
    focus: Signal<Option<ElementId>>,
    /// Traversal order (registration order ≈ tree order).
    order: Vec<ElementId>,
    /// Enter/Space activation handlers.
    activation: HashMap<ElementId, Rc<dyn Fn()>>,
    /// `onFocusChange(bool)` handlers.
    on_change: HashMap<ElementId, Rc<dyn Fn(bool)>>,
    /// Text-editor key handlers — the routing target for [`dispatch_key`].
    edit: HashMap<ElementId, Rc<dyn Fn(KeyInput)>>,
}

thread_local! {
    static MGR: RefCell<Option<FocusManager>> = const { RefCell::new(None) };
}

fn with_mgr<R>(f: impl FnOnce(&mut FocusManager) -> R) -> R {
    MGR.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            // NOTE: must be created outside any component (see `init`) so the focus
            // signal is global, not owned by whatever component ran first.
            *cell = Some(FocusManager {
                focus: create_signal(None),
                order: Vec::new(),
                activation: HashMap::new(),
                on_change: HashMap::new(),
                edit: HashMap::new(),
            });
        }
        f(cell.as_mut().unwrap())
    })
}

/// Initialize the focus system. Call once at app startup, before the widget tree
/// runs, so the global focus signal isn't captured by a component.
pub fn init() {
    with_mgr(|_| {});
}

/// The global focus signal (reactive).
pub fn focus_signal() -> Signal<Option<ElementId>> {
    with_mgr(|m| m.focus)
}

/// Move primary focus, firing `onFocusChange(false)` on the old node and
/// `onFocusChange(true)` on the new one.
pub fn set_focus(next: Option<ElementId>) {
    let (sig, old, old_cb, new_cb) = with_mgr(|m| {
        let old = m.focus.peek();
        let old_cb = old.and_then(|id| m.on_change.get(&id).cloned());
        let new_cb = next.and_then(|id| m.on_change.get(&id).cloned());
        (m.focus, old, old_cb, new_cb)
    });
    if old == next {
        return;
    }
    sig.set(next);
    if let Some(cb) = old_cb {
        cb(false);
    }
    if let Some(cb) = new_cb {
        cb(true);
    }
}

/// A handle to a focusable node. Its id is the owning component's element id.
#[derive(Clone, Copy)]
pub struct FocusNode {
    id: ElementId,
}

/// Create (or recover) the focus node for the current component. Automatically
/// unregisters from the focus system when the component unmounts, so navigation
/// never leaves stale nodes (whose callbacks would touch freed signals) behind.
pub fn create_focus() -> FocusNode {
    let id = current_owner().expect("create_focus must be called inside a component");
    crate::reactive::create_cleanup(move || unregister(id));
    FocusNode { id }
}

impl FocusNode {
    /// Whether this node currently has primary focus (reactive read).
    pub fn is_focused(&self) -> bool {
        focus_signal().get() == Some(self.id)
    }

    /// Request primary focus for this node.
    pub fn request_focus(&self) {
        set_focus(Some(self.id));
    }

    /// This node's stable id.
    pub fn id(&self) -> ElementId {
        self.id
    }

    /// This node's stable id as a `u64` (matches render-tree source ids), used to
    /// key per-field state such as the text-edit layout registry.
    pub fn raw_id(&self) -> u64 {
        use slotmap::Key;
        self.id.data().as_ffi()
    }

    /// Register this node as a text editor: while focused, keyboard edit intents
    /// ([`KeyInput`]) route to `handler`. Called each render (idempotent).
    pub fn register_editor(&self, handler: Rc<dyn Fn(KeyInput)>) {
        let id = self.id;
        with_mgr(|m| {
            m.edit.insert(id, handler);
        });
    }

    /// Register this node's keyboard-activation + focus-change handlers (called each
    /// render; idempotent). `autofocus` grabs focus if nothing is focused yet.
    pub fn register(
        &self,
        activation: Rc<dyn Fn()>,
        on_change: Option<Rc<dyn Fn(bool)>>,
        autofocus: bool,
    ) {
        let id = self.id;
        with_mgr(|m| {
            if !m.order.contains(&id) {
                m.order.push(id);
            }
            m.activation.insert(id, activation);
            match on_change {
                Some(c) => {
                    m.on_change.insert(id, c);
                }
                None => {
                    m.on_change.remove(&id);
                }
            }
        });
        if autofocus && focus_signal().peek().is_none() {
            self.request_focus();
        }
    }
}

/// Remove a node from the focus system (on unmount).
pub fn unregister(id: ElementId) {
    let was_focused = with_mgr(|m| {
        m.order.retain(|&x| x != id);
        m.activation.remove(&id);
        m.on_change.remove(&id);
        m.edit.remove(&id);
        m.focus.peek() == Some(id)
    });
    if was_focused {
        set_focus(None);
    }
}

/// Whether the currently-focused node is a text editor (so the shell routes
/// character keys to it instead of treating Space/Enter as activation).
pub fn focused_is_editor() -> bool {
    let focused = focus_signal().peek();
    with_mgr(|m| focused.is_some_and(|id| m.edit.contains_key(&id)))
}

/// Route a keyboard edit intent to the focused editor. Returns whether it was
/// handled (i.e. an editor was focused).
pub fn dispatch_key(key: KeyInput) -> bool {
    let handler = {
        let focused = focus_signal().peek();
        with_mgr(|m| focused.and_then(|id| m.edit.get(&id).cloned()))
    };
    match handler {
        Some(h) => {
            h(key);
            true
        }
        None => false,
    }
}

/// Activate the focused node (Enter/Space). Returns whether anything was activated.
pub fn activate_focused() -> bool {
    let action = {
        let focused = focus_signal().peek();
        with_mgr(|m| focused.and_then(|id| m.activation.get(&id).cloned()))
    };
    match action {
        Some(a) => {
            a();
            true
        }
        None => false,
    }
}

/// Move focus to the next focusable (Tab). `forward = false` moves to the previous.
pub fn focus_move(forward: bool) -> bool {
    let next = with_mgr(|m| {
        if m.order.is_empty() {
            return None;
        }
        let len = m.order.len();
        let cur = m.focus.peek().and_then(|id| m.order.iter().position(|&x| x == id));
        let idx = match cur {
            Some(i) if forward => (i + 1) % len,
            Some(i) => (i + len - 1) % len,
            None => 0,
        };
        Some(m.order[idx])
    });
    if next.is_some() {
        set_focus(next);
        true
    } else {
        false
    }
}
