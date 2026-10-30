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
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::element::ElementId;
use crate::keyboard::{KeyEvent, KeyEventResult, KeyInput};
use crate::reactive::{
    Signal, consume_context, create_root_signal, current_owner, current_window, instance_id, provide_context,
};

/// A focus node's identity: `(window, element id)`.
type FocusKey = (u32, ElementId);

/// A raw key handler (see [`FocusNode::on_key_event`]).
pub type RawKeyHandler = Rc<dyn Fn(&KeyEvent) -> KeyEventResult>;

/// The render-time context a `FocusScope` provides to its subtree (see
/// [`create_focus_scope`]). Focusable nodes registered inside it inherit the
/// scope id; [`focus_move`] then cycles Tab within the scope — the focus trap
/// that keeps keyboard traversal inside dialogs and sheets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScopeTag(pub u64);

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
    edit: HashMap<FocusKey, Rc<dyn Fn(KeyInput) -> bool>>,
    /// Non-editor key handlers (e.g. chart keyboard traversal). Consulted by
    /// [`dispatch_key`] AFTER editors; the handler returns whether it consumed the key.
    /// Unlike editors these do NOT set [`focused_is_editor`], so shell shortcuts and IME
    /// stay off for them.
    keys: HashMap<FocusKey, Rc<dyn Fn(KeyInput) -> bool>>,
    /// Raw key-event handlers (Flutter's `Focus.onKeyEvent`): offered every key down /
    /// repeat / up of the focused node BEFORE intents and shortcuts ([`dispatch_key_event`]).
    raw: HashMap<FocusKey, RawKeyHandler>,
    /// Editors that consume Tab for indent/outdent (code editors) rather than letting Tab
    /// traverse focus (single-line text fields). Read by the shell via [`focused_wants_tab`].
    wants_tab: HashSet<FocusKey>,
    /// Nodes whose one-shot `autofocus` has already been consumed. Autofocus must fire
    /// exactly once (on the node's first mount) — NOT every render in which nothing is
    /// focused, which would let an autofocus field yank focus back after the user blurs
    /// it (an infinite render→refocus→render oscillation, e.g. against the caret loop).
    autofocused: HashSet<FocusKey>,
    /// The innermost focus scope each node registered under (`0` = root scope).
    scope_of: HashMap<FocusKey, u64>,
    /// Per scope: the node that held focus just before focus first moved INTO the
    /// scope (from outside it). When the scope unmounts (a dialog closes) focus goes
    /// back there — Flutter's route pop / the web's dialog focus restoration.
    restore: HashMap<u64, Option<FocusKey>>,
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
                keys: HashMap::new(),
                raw: HashMap::new(),
                wants_tab: HashSet::new(),
                autofocused: HashSet::new(),
                scope_of: HashMap::new(),
                restore: HashMap::new(),
            });
        }
        f(cell.as_mut().expect("FocusManager initialized just above when cell was None"))
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

/// The FFI id of the element that currently holds focus **in `window`**, or `None`.
/// This matches the render node `source` (and thus a `SemanticsNode`'s id), so the
/// shell can tell the accessibility layer which node has keyboard focus.
pub fn focused_element_ffi(window: u32) -> Option<u64> {
    use slotmap::Key;
    focus_signal().peek().and_then(|(w, e)| (w == window).then(|| e.data().as_ffi()))
}

/// Move primary focus, firing `onFocusChange(false)` on the old node and
/// `onFocusChange(true)` on the new one. `None` clears focus (the common call from
/// the shell / a tap on empty space).
pub fn set_focus(next: Option<FocusKey>) {
    let (sig, old, old_cb, new_cb) = with_mgr(|m| {
        let old = m.focus.peek();
        let old_cb = old.and_then(|k| m.on_change.get(&k).cloned());
        let new_cb = next.and_then(|k| m.on_change.get(&k).cloned());
        // Entering a (non-root) scope from outside it: remember where focus came from.
        let scope_of = |k: Option<FocusKey>| k.and_then(|k| m.scope_of.get(&k).copied()).unwrap_or(0);
        let (from, to) = (scope_of(old), scope_of(next));
        if to != 0 && from != to {
            m.restore.entry(to).or_insert(old);
        }
        (m.focus, old, old_cb, new_cb)
    });
    if old == next {
        return;
    }
    if crate::log::dev_mode() {
        crate::log::debug(crate::log::Cat::Reactive, format!("focus {old:?} → {next:?}"));
    }
    sig.set(next);
    if let Some(cb) = old_cb {
        cb(false);
    }
    if let Some(cb) = new_cb {
        cb(true);
    }
}

/// Focus the registered node in `window` whose FFI id equals `source` (the id a
/// [`SemanticsNode`](pebbles_render::SemanticsNode) carries). Returns whether a match
/// was found and focused. Used by the shell to honor an AT-driven `Focus` action (D1).
pub fn focus_by_source(window: u32, source: u64) -> bool {
    use slotmap::Key;
    let key =
        with_mgr(|m| m.order.iter().copied().find(|(w, e)| *w == window && e.data().as_ffi() == source));
    match key {
        Some(k) => {
            set_focus(Some(k));
            true
        }
        None => false,
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
///
/// # Panics
/// Panics if called outside a component render (there is no current owner to bind the
/// node to) — this is a hook, so call it at the top level of a component.
pub fn create_focus() -> FocusNode {
    let id = current_owner().expect("create_focus must be called inside a component");
    crate::reactive::create_cleanup(move || unregister(id));
    FocusNode { window: current_window(), id }
}

/// Create a focus **scope** for the current component: while this component's
/// subtree renders, every focusable node inside it registers under this scope's
/// id, and [`focus_move`] cycles Tab within the scope — the focus trap that keeps
/// keyboard traversal inside a dialog or sheet. Scopes nest: a node belongs to
/// the innermost enclosing scope. Call at the top level of a component (a hook);
/// on unmount the scope and any surviving member entries are removed.
///
/// # Panics
/// Panics if called outside a component render (no current owner) — like
/// [`create_focus`], it is a hook and must be called at a component's top level.
pub fn create_focus_scope() -> ScopeTag {
    let window = current_window();
    let element = current_owner().expect("create_focus_scope must be called inside a component");
    let id = instance_id(window, element);
    provide_context(ScopeTag(id));
    crate::reactive::create_cleanup(move || {
        // On close, hand focus back to the node focused before the scope took it, if
        // focus is still inside the scope (or was dropped) and that node still exists.
        let back = with_mgr(|m| {
            let prev = m.restore.remove(&id).flatten();
            let cur = m.focus.peek();
            let inside = cur.is_none_or(|k| m.scope_of.get(&k) == Some(&id));
            m.scope_of.retain(|_, &mut scope| scope != id);
            // Still mounted: focusable widgets sit in `order`; code editors register only
            // their key handler.
            let live = |k: &FocusKey| m.order.contains(k) || m.edit.contains_key(k) || m.keys.contains_key(k);
            prev.filter(|k| inside && live(k))
        });
        if let Some(k) = back {
            set_focus(Some(k));
        }
    });
    ScopeTag(id)
}

/// The scope the currently-rendering component belongs to, if any.
fn current_scope() -> u64 {
    consume_context::<ScopeTag>().map(|s| s.0).unwrap_or(0)
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
        self.register_editor_bubbling(Rc::new(move |k| {
            handler(k);
            true
        }));
    }

    /// Like [`register_editor`](Self::register_editor), but `handler` reports whether it
    /// handled the intent. Returning `false` lets the key **bubble** to app-level
    /// shortcuts (Flutter `Shortcuts`/`Actions` semantics: a focused widget that does not
    /// handle a key leaves it for its ancestors). Use this so e.g. Ctrl+Shift+F reaches the
    /// app's "find in files" while a text field is focused.
    pub fn register_editor_bubbling(&self, handler: Rc<dyn Fn(KeyInput) -> bool>) {
        let key = self.key();
        with_mgr(|m| {
            m.edit.insert(key, handler);
            m.wants_tab.remove(&key);
        });
    }

    /// Register as an editor that also **consumes Tab/Shift+Tab** for indent/outdent (a code
    /// editor), so the shell sends [`KeyInput::Indent`]/[`KeyInput::Outdent`] instead of
    /// moving focus. Plain text fields use [`register_editor`](Self::register_editor) so Tab
    /// still traverses focus. Called each render (idempotent).
    pub fn register_code_editor(&self, handler: Rc<dyn Fn(KeyInput)>) {
        self.register_code_editor_bubbling(Rc::new(move |k| {
            handler(k);
            true
        }));
    }

    /// [`register_code_editor`](Self::register_code_editor) whose handler reports whether
    /// it handled the intent; unhandled intents bubble to app shortcuts (see
    /// [`register_editor_bubbling`](Self::register_editor_bubbling)).
    pub fn register_code_editor_bubbling(&self, handler: Rc<dyn Fn(KeyInput) -> bool>) {
        let key = self.key();
        with_mgr(|m| {
            m.edit.insert(key, handler);
            m.wants_tab.insert(key);
        });
    }

    /// Register a NON-editor key handler: while focused, keys route to `handler` (after
    /// editors), which returns whether it consumed the key. For focusable widgets that
    /// want arrow-key navigation (charts, custom lists) without being text editors.
    /// Called each render (idempotent).
    pub fn register_keys(&self, handler: Rc<dyn Fn(KeyInput) -> bool>) {
        let key = self.key();
        with_mgr(|m| {
            m.keys.insert(key, handler);
        });
    }

    /// Install a raw key handler (Flutter's `Focus.onKeyEvent`): while this node is
    /// focused, every key event — down, auto-repeat and up, including PageUp/PageDown,
    /// Insert, F1–F35 and bare modifiers — reaches `handler` FIRST, before the shell maps
    /// it to an editing intent ([`KeyInput`]) or an app shortcut. Return
    /// [`KeyEventResult::Handled`] to consume it, [`KeyEventResult::Ignored`] to let the
    /// normal intent → shortcut → Tab → activation → key-scroll chain run. Called each
    /// render (idempotent).
    ///
    /// ```no_run
    /// use std::rc::Rc;
    /// use pebbles_core::create_focus;
    /// use pebbles_core::keyboard::{KeyEventResult, LogicalKey, NamedKey};
    /// let node = create_focus();
    /// node.on_key_event(Rc::new(|e| match e.logical {
    ///     LogicalKey::Named(NamedKey::PageUp) if e.is_press() => KeyEventResult::Handled,
    ///     _ => KeyEventResult::Ignored,
    /// }));
    /// ```
    pub fn on_key_event(&self, handler: RawKeyHandler) {
        let key = self.key();
        with_mgr(|m| {
            m.raw.insert(key, handler);
        });
    }

    /// Register this node's keyboard-activation + focus-change handlers (called each
    /// render; idempotent). `autofocus` grabs focus if nothing is focused yet.
    pub fn register(&self, activation: Rc<dyn Fn()>, on_change: Option<Rc<dyn Fn(bool)>>, autofocus: bool) {
        let key = self.key();
        let should_autofocus = with_mgr(|m| {
            if !m.order.contains(&key) {
                m.order.push(key);
            }
            m.activation.insert(key, activation);
            // Membership in the innermost enclosing focus scope is captured here,
            // during render, while the scope's context is on the stack.
            m.scope_of.insert(key, current_scope());
            match on_change {
                Some(c) => {
                    m.on_change.insert(key, c);
                }
                None => {
                    m.on_change.remove(&key);
                }
            }
            // One-shot: mark autofocus consumed on the first render regardless, and only
            // actually grab focus if nothing in this node's focus scope holds it (Flutter:
            // autofocus applies within the enclosing FocusScope). A node in a freshly
            // opened dialog's scope therefore takes focus from the page behind it; in the
            // root scope it only grabs when nothing is focused. After this the node never
            // auto-grabs again, so an explicit blur sticks.
            let scope = current_scope();
            let held_in_scope = m.focus.peek().is_some_and(|f| scope == 0 || m.scope_of.get(&f) == Some(&scope));
            if autofocus && m.autofocused.insert(key) { !held_in_scope } else { false }
        });
        if should_autofocus {
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
        m.keys.remove(&key);
        m.raw.remove(&key);
        m.wants_tab.remove(&key);
        m.scope_of.remove(&key);
        // Let a genuine remount of this id autofocus again.
        m.autofocused.remove(&key);
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

/// Whether the focused node is a code editor that consumes Tab for indent/outdent (so the
/// shell sends [`KeyInput::Indent`]/[`KeyInput::Outdent`] rather than traversing focus).
pub fn focused_wants_tab() -> bool {
    let focused = focus_signal().peek();
    with_mgr(|m| focused.is_some_and(|k| m.wants_tab.contains(&k)))
}

/// Whether a text editor currently holds focus (so clipboard intents have a
/// target).
pub fn editor_is_focused() -> bool {
    with_mgr(|m| m.focus.peek().is_some_and(|k| m.edit.contains_key(&k)))
}

/// The focusable nodes registered in `window`, in traversal order. Read-only —
/// for tests, debugging and tooling (inspectors, focus lists).
pub fn registered_nodes(window: u32) -> Vec<(u32, ElementId)> {
    with_mgr(|m| m.order.iter().copied().filter(|&(w, _)| w == window).collect())
}

/// Number of live focus registrations across all windows (debug-only).
#[cfg(debug_assertions)]
pub fn census_registrations() -> usize {
    with_mgr(|m| m.order.len())
}

/// Route a keyboard edit intent to the focused editor. Returns whether it was
/// handled; `false` means the key should bubble to app-level shortcuts.
pub fn dispatch_key(key: KeyInput) -> bool {
    let focused = focus_signal().peek();
    // Editors take precedence (they consume all text intents).
    let editor = with_mgr(|m| focused.and_then(|k| m.edit.get(&k).cloned()));
    // An editor that declines the intent (bubbling registration) lets it fall through to
    // app shortcuts.
    if let Some(h) = editor {
        return h(key);
    }
    // Otherwise offer it to a non-editor key handler (chart traversal, etc.), which
    // reports whether it actually used the key so unhandled keys fall through to Tab.
    let keys = with_mgr(|m| focused.and_then(|k| m.keys.get(&k).cloned()));
    match keys {
        Some(h) => h(key),
        None => false,
    }
}

/// Offer a raw key event to the focused node's [`FocusNode::on_key_event`] handler.
/// [`KeyEventResult::Ignored`] when nothing is focused or the node has no handler —
/// the caller then runs the intent / shortcut chain (releases stop here).
pub fn dispatch_key_event(event: &KeyEvent) -> KeyEventResult {
    let focused = focus_signal().peek();
    let handler = with_mgr(|m| focused.and_then(|k| m.raw.get(&k).cloned()));
    match handler {
        Some(h) => h(event),
        None => KeyEventResult::Ignored,
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
/// previous. Traversal stays within the one window so Tab never jumps to another, and
/// within the focused node's **focus scope** (the trap: Tab cycles inside the
/// innermost enclosing [`create_focus_scope`] — dialogs and sheets contain focus).
/// With nothing focused, cycles the root scope (nodes outside any scope).
pub fn focus_move(window: u32, forward: bool) -> bool {
    let next = with_mgr(|m| {
        let nodes: Vec<FocusKey> = m.order.iter().copied().filter(|&(w, _)| w == window).collect();
        if nodes.is_empty() {
            return None;
        }
        // The scope we may not leave: the focused node's, or the root scope when
        // nothing is focused yet.
        let scope = m.focus.peek().and_then(|k| m.scope_of.get(&k).copied()).unwrap_or(0);
        let candidates: Vec<FocusKey> =
            nodes.iter().copied().filter(|k| m.scope_of.get(k).copied().unwrap_or(0) == scope).collect();
        // A scope with no members (unmounted mid-cycle) falls back to the root set.
        let pool = if candidates.is_empty() && scope != 0 { nodes } else { candidates };
        if pool.is_empty() {
            return None;
        }
        let len = pool.len();
        let cur = m.focus.peek().and_then(|k| pool.iter().position(|&x| x == k));
        let idx = match cur {
            Some(i) if forward => (i + 1) % len,
            Some(i) => (i + len - 1) % len,
            None => 0,
        };
        Some(pool[idx])
    });
    if next.is_some() {
        set_focus(next);
        true
    } else {
        false
    }
}
