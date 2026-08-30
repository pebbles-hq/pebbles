//! The focus system: a [`FocusNode`] per focusable widget, a single primary-focus
//! target, keyboard **activation** (Enter/Space), **Tab traversal**, and
//! **onFocusChange** callbacks.
//!
//! Focus is reactive: the current target lives in a global [`Signal`], so a widget
//! reading [`FocusNode::is_focused`] re-renders (e.g. to show a focus ring) when
//! focus moves. A node's identity is `(window, element id)` — the window keeps two
//! windows' nodes distinct even though their element ids collide. There is one
//! primary focus across all windows (like the OS: one focused widget at a time),
//! but Tab traversal stays within a window.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::element::ElementId;
use crate::keyboard::KeyInput;
use crate::reactive::{Signal, create_root_signal, current_owner, current_window, instance_id};

/// A focus node's identity: `(window, element id)`.
type FocusKey = (u32, ElementId);

struct FocusManager {
    /// Reactive source of truth for the currently-focused node.
    focus: Signal<Option<FocusKey>>,
    /// Traversal order (registration order ≈ tree order).
    order: Vec<FocusKey>,
    /// Enter/Space activation handlers.
    activation: HashMap<FocusKey, Rc<dyn Fn()>>,
    /// `onFocusChange(bool)` handlers.
    on_change: HashMap<FocusKey, Rc<dyn Fn(bool)>>,
    /// Text-editor key handlers — the routing target for [`dispatch_key`].
    edit: HashMap<FocusKey, Rc<dyn Fn(KeyInput)>>,
}

thread_local! {
    static MGR: RefCell<Option<FocusManager>> = const { RefCell::new(None) };
}

fn with_mgr<R>(f: impl FnOnce(&mut FocusManager) -> R) -> R {
    MGR.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            // App-owned regardless of the current render context (`create_root_signal`),
            // so the global focus signal is never captured by whatever component ran first.
            *cell = Some(FocusManager {
                focus: create_root_signal(None),
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
pub fn focus_signal() -> Signal<Option<FocusKey>> {
    with_mgr(|m| m.focus)
}

/// Move primary focus, firing `onFocusChange(false)` on the old node and
/// `onFocusChange(true)` on the new one. `None` clears focus (the common call from
/// the shell / a tap on empty space).
pub fn set_focus(next: Option<FocusKey>) {
    let (sig, old, old_cb, new_cb) = with_mgr(|m| {
        let old = m.focus.peek();
        let old_cb = old.and_then(|k| m.on_change.get(&k).cloned());
        let new_cb = next.and_then(|k| m.on_change.get(&k).cloned());
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

/// A handle to a focusable node — its component's `(window, element id)`.
#[derive(Clone, Copy)]
pub struct FocusNode {
    window: u32,
    id: ElementId,
}

/// Create (or recover) the focus node for the current component. Automatically
/// unregisters from the focus system when the component unmounts, so navigation
/// never leaves stale nodes (whose callbacks would touch freed signals) behind.
pub fn create_focus() -> FocusNode {
    let id = current_owner().expect("create_focus must be called inside a component");
    crate::reactive::create_cleanup(move || unregister(id));
    FocusNode { window: current_window(), id }
}

impl FocusNode {
    fn key(&self) -> FocusKey {
        (self.window, self.id)
    }

    /// Whether this node currently has primary focus (reactive read).
    pub fn is_focused(&self) -> bool {
        focus_signal().get() == Some(self.key())
    }

    /// Request primary focus for this node.
    pub fn request_focus(&self) {
        set_focus(Some(self.key()));
    }

    /// This node's stable element id.
    pub fn id(&self) -> ElementId {
        self.id
    }

    /// This node's globally-unique instance id (matches render-tree source ids),
    /// used to key per-field state such as the text-edit layout registry. Unique
    /// across windows, so two windows' text fields never share a registry slot.
    pub fn raw_id(&self) -> u64 {
        instance_id(self.window, self.id)
    }

    /// Register this node as a text editor: while focused, keyboard edit intents
    /// ([`KeyInput`]) route to `handler`. Called each render (idempotent).
    pub fn register_editor(&self, handler: Rc<dyn Fn(KeyInput)>) {
        let key = self.key();
        with_mgr(|m| {
            m.edit.insert(key, handler);
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
        let key = self.key();
        with_mgr(|m| {
            if !m.order.contains(&key) {
                m.order.push(key);
            }
            m.activation.insert(key, activation);
            match on_change {
                Some(c) => {
                    m.on_change.insert(key, c);
                }
                None => {
                    m.on_change.remove(&key);
                }
            }
        });
        if autofocus && focus_signal().peek().is_none() {
            self.request_focus();
        }
    }
}

/// Remove a node from the focus system (on unmount). Called during dispose, when the
/// current window is the node's own window.
pub fn unregister(id: ElementId) {
    let key = (current_window(), id);
    let was_focused = with_mgr(|m| {
        m.order.retain(|&x| x != key);
        m.activation.remove(&key);
        m.on_change.remove(&key);
        m.edit.remove(&key);
        m.focus.peek() == Some(key)
    });
    if was_focused {
        set_focus(None);
    }
}

/// Whether the currently-focused node is a text editor (so the shell routes
/// character keys to it instead of treating Space/Enter as activation).
pub fn focused_is_editor() -> bool {
    let focused = focus_signal().peek();
    with_mgr(|m| focused.is_some_and(|k| m.edit.contains_key(&k)))
}

/// Route a keyboard edit intent to the focused editor. Returns whether it was
/// handled (i.e. an editor was focused).
pub fn dispatch_key(key: KeyInput) -> bool {
    let handler = {
        let focused = focus_signal().peek();
        with_mgr(|m| focused.and_then(|k| m.edit.get(&k).cloned()))
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
        with_mgr(|m| focused.and_then(|k| m.activation.get(&k).cloned()))
    };
    match action {
        Some(a) => {
            a();
            true
        }
        None => false,
    }
}

/// Move focus to the next focusable in `window` (Tab). `forward = false` moves to the
/// previous. Traversal stays within the one window so Tab never jumps to another.
pub fn focus_move(window: u32, forward: bool) -> bool {
    let next = with_mgr(|m| {
        let nodes: Vec<FocusKey> = m.order.iter().copied().filter(|&(w, _)| w == window).collect();
        if nodes.is_empty() {
            return None;
        }
        let len = nodes.len();
        let cur = m.focus.peek().and_then(|k| nodes.iter().position(|&x| x == k));
        let idx = match cur {
            Some(i) if forward => (i + 1) % len,
            Some(i) => (i + len - 1) % len,
            None => 0,
        };
        Some(nodes[idx])
    });
    if next.is_some() {
        set_focus(next);
        true
    } else {
        false
    }
}
