//! The element tree and the [`Ui`] engine that drives it.
//!
//! An **element** is the retained instantiation of a widget. Where widgets are
//! rebuilt constantly, elements persist and are *reconciled*: when a new widget of
//! the same concrete type + key lands in the same slot, the element (and any
//! `State` or render object it owns) is reused and merely updated.
//!
//! [`Ui`] owns the element arena and the [`RenderTree`], and exposes the four
//! operations the shell needs each frame: reconcile dirty subtrees, hit-test +
//! dispatch input, lay out, and paint.

use std::any::Any;
use std::rc::Rc;

use pebbles_foundation::{Offset, Size};
use pebbles_render::{
    BoxConstraints, PointerButton, PointerEvent, RenderId, RenderList, RenderPointerListener,
    RenderScroll, RenderTree, Scene, TextEnv,
};

use crate::scroll::ScrollTo;
use slotmap::{Key, SlotMap, new_key_type};

use crate::context::Callback;
use crate::widget::{AnyWidget, Widget};

new_key_type! {
    /// A stable, generational handle to a node in the element tree.
    pub struct ElementId;
}

/// What kind of element a node is, and the per-kind retained data.
enum ElementKind {
    /// A SolidJS-style function component: re-runs its `fn` to build a child; its
    /// local signals are owned by this element.
    Function,
    /// A render-object element: owns a node in the render tree (its `render_id`).
    Render,
    /// A parent-data element: owns no render object; attaches parent data to its
    /// single child's render object (e.g. `Expanded`, `Positioned`).
    ParentData,
}

/// One node in the element tree.
struct ElementNode {
    /// Parent link. Retained for ancestor walks (inherited widgets, focus) — not
    /// all consumers exist yet.
    #[allow(dead_code)]
    parent: Option<ElementId>,
    /// Current widget configuration.
    widget: AnyWidget,
    kind: ElementKind,
    /// Child elements, in order.
    children: Vec<ElementId>,
    /// The backing render node, for `Render` elements only.
    render_id: Option<RenderId>,
    /// Distance from the root; used to reconcile parents before children.
    depth: u32,
}

/// Discriminates a widget's category without moving it.
enum Category {
    Function,
    Render,
    ParentData,
}

fn category(widget: &dyn Widget) -> Category {
    if widget.as_component().is_some() {
        Category::Function
    } else if widget.as_parent_data().is_some() {
        Category::ParentData
    } else {
        // Anything else must be a render widget; this is enforced by the macros.
        debug_assert!(widget.as_render().is_some(), "widget has no known category");
        Category::Render
    }
}

thread_local! {
    /// Hands out a distinct id to each `Ui` (window). The first — the main window —
    /// is `0`, matching the single-window runtime exactly.
    static NEXT_UI_ID: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

fn next_ui_id() -> u32 {
    NEXT_UI_ID.with(|c| {
        let v = c.get();
        c.set(v + 1);
        v
    })
}

/// The UI engine: element arena + render tree + dirty set. Each `Ui` is one window;
/// its `ui_id` namespaces its components in the shared reactive runtime so multiple
/// windows never alias each other's element ids.
#[derive(Default)]
pub struct Ui {
    ui_id: u32,
    elements: SlotMap<ElementId, ElementNode>,
    render: RenderTree,
    root: Option<ElementId>,
    dirty: Vec<ElementId>,
    /// The render node currently hovered plus its stored exit handlers. Keyed by
    /// render id — stable across rebuilds because a reconciled listener updates its
    /// render object in place rather than recreating it — so hover-out fires reliably.
    hovered: Option<HoverTarget>,
    /// The scroll viewport whose scrollbar is being dragged, if any.
    scrollbar_drag: Option<RenderId>,
    /// Imperative scroll views whose spring is currently animating.
    scroll_anim: std::collections::HashSet<RenderId>,
}

/// A resolved event handler ready to run: a plain closure or an event-carrying
/// closure.
enum Invoke {
    Plain(Rc<dyn Fn()>),
    Event(Rc<dyn Fn(PointerEvent)>),
}

struct HoverTarget {
    /// The hovered element's stable `source` id (NOT the render id, which a re-render
    /// can reassign — e.g. showing a tooltip re-renders the overlay host). Keying by
    /// source lets hover-exit still fire after such a re-render.
    source: u64,
    exits: Vec<Invoke>,
}

impl Ui {
    pub fn new() -> Self {
        Ui { ui_id: next_ui_id(), ..Self::default() }
    }

    /// This window's id in the shared runtime (`0` = main window).
    pub fn window_id(&self) -> u32 {
        self.ui_id
    }

    /// Make this window the "current" one in the shared runtime, so window-scoped
    /// globals (per-window overlay + dialog signals) resolve to *this* window. The
    /// shell calls this before dispatching input to a window, since event handlers —
    /// unlike render — don't otherwise set the current window.
    pub fn make_current(&self) {
        crate::reactive::set_current_window(self.ui_id);
    }

    /// Read-only access to the render tree (the shell lays out / paints it).
    pub fn render_tree(&self) -> &RenderTree {
        &self.render
    }

    // ----- lifecycle -------------------------------------------------------

    /// Inflate `widget` as the root of the tree. The root widget must ultimately
    /// own a render object (the shell wraps user content in a `View`).
    pub fn mount_root(&mut self, widget: AnyWidget) {
        crate::reactive::set_current_window(self.ui_id);
        let root = self.inflate(None, widget);
        self.root = Some(root);
        self.render.root = self.elements[root].render_id;
        self.sync_render(root);
        self.apply_parent_data(root);
    }

    /// Reconcile every dirty subtree. Returns `true` if anything was rebuilt (the
    /// shell then re-runs layout + paint).
    pub fn rebuild_if_dirty(&mut self) -> bool {
        crate::reactive::set_current_window(self.ui_id);
        // Run scheduled effects, then fold THIS window's reactive re-renders into the
        // dirty set (other windows drain their own).
        crate::reactive::flush_effects();
        for id in crate::reactive::take_pending_components(self.ui_id) {
            self.mark_dirty(id);
        }
        if self.dirty.is_empty() {
            return false;
        }
        let mut dirty = std::mem::take(&mut self.dirty);
        // Reconcile shallow elements first so a parent's rebuild subsumes children.
        dirty.sort_by_key(|&id| self.elements.get(id).map(|n| n.depth).unwrap_or(0));
        dirty.dedup();
        for id in dirty {
            if self.elements.contains_key(id) {
                self.rebuild_element(id);
            }
        }
        if let Some(root) = self.root {
            self.sync_render(root);
            self.apply_parent_data(root);
        }
        true
    }

    // ----- input -----------------------------------------------------------

    /// Resolve every callback in the `pick`ed slot of a listener into runnable form.
    fn invokes_of(
        listener: &RenderPointerListener,
        pick: impl Fn(&RenderPointerListener) -> &[pebbles_render::TapCallback],
    ) -> Vec<Invoke> {
        pick(listener)
            .iter()
            .filter_map(|any| any.downcast_ref::<Callback>())
            .map(|cb| match cb {
                Callback::Plain(f) => Invoke::Plain(f.clone()),
                Callback::Event(f) => Invoke::Event(f.clone()),
            })
            .collect()
    }

    fn run_invoke(&mut self, invoke: Invoke, event: PointerEvent) {
        match invoke {
            Invoke::Plain(f) => f(),
            Invoke::Event(f) => f(event),
        }
    }

    /// Fire every callback in the topmost listener that has any for the picked event,
    /// passing a [`PointerEvent`] with the position in that widget's local space.
    fn fire_pointer(
        &mut self,
        point: Offset,
        button: PointerButton,
        pick: impl Fn(&RenderPointerListener) -> &[pebbles_render::TapCallback],
    ) -> bool {
        let hits = self.render.hit_test(point); // ordered root -> leaf
        for &rid in hits.iter().rev() {
            let invokes = self
                .render
                .object_ref(rid)
                .downcast_ref::<RenderPointerListener>()
                .map(|l| Self::invokes_of(l, &pick))
                .unwrap_or_default();
            if !invokes.is_empty() {
                let local = point - self.render.absolute_offset(rid);
                let event = PointerEvent { position: local, global: point, button };
                for invoke in invokes {
                    self.run_invoke(invoke, event);
                }
                return true;
            }
        }
        false
    }

    /// A primary-button tap at `point`. Returns `true` if handled.
    pub fn dispatch_tap(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_tap)
    }

    /// A primary-button double tap at `point`.
    pub fn dispatch_double_tap(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_double_tap)
    }

    /// A secondary-button (right-click) tap at `point`.
    pub fn dispatch_secondary_tap(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Secondary, |l| &l.on_secondary_tap)
    }

    /// Secondary button pressed down at `point`.
    pub fn dispatch_secondary_tap_down(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Secondary, |l| &l.on_secondary_tap_down)
    }

    /// Secondary button released at `point`.
    pub fn dispatch_secondary_tap_up(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Secondary, |l| &l.on_secondary_tap_up)
    }

    /// A long-press at `point` (button held past the long-press interval).
    pub fn dispatch_long_press(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_long_press)
    }

    /// Primary button pressed down at `point`.
    pub fn dispatch_pointer_down(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_pointer_down)
    }

    /// Primary button released at `point`.
    pub fn dispatch_pointer_up(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_pointer_up)
    }

    /// The source id of the topmost primary-tap-family listener under `point` (tap
    /// or double-tap) — used to arm a press so a release elsewhere becomes a cancel,
    /// and so a double-tap-only widget (no `on_tap`) still receives its event.
    pub fn tap_target_at(&self, point: Offset) -> Option<u64> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            let l = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            if l.on_tap.is_empty() && l.on_double_tap.is_empty() {
                None
            } else {
                self.render.source_of(rid)
            }
        })
    }

    /// Fire a picked event slot on the listener with the given source id, computing
    /// the [`PointerEvent`] in that widget's local space. Used for gestures that stay
    /// bound to their original target after the pointer moves (cancel, long-press).
    fn fire_source(
        &mut self,
        source: u64,
        point: Offset,
        button: PointerButton,
        pick: impl Fn(&RenderPointerListener) -> &[pebbles_render::TapCallback],
    ) -> bool {
        let Some(rid) = self.render.find_by_source(source) else { return false };
        let invokes = self
            .render
            .object_ref(rid)
            .downcast_ref::<RenderPointerListener>()
            .map(|l| Self::invokes_of(l, &pick))
            .unwrap_or_default();
        if invokes.is_empty() {
            return false;
        }
        let local = point - self.render.absolute_offset(rid);
        let event = PointerEvent { position: local, global: point, button };
        for invoke in invokes {
            self.run_invoke(invoke, event);
        }
        true
    }

    /// Fire `on_tap_cancel` on the armed target.
    pub fn dispatch_tap_cancel(&mut self, source: u64) -> bool {
        self.fire_source(source, Offset::ZERO, PointerButton::Primary, |l| &l.on_tap_cancel)
    }

    /// The source id of the topmost long-press listener under `point`.
    pub fn long_press_target_at(&self, point: Offset) -> Option<u64> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            let l = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            let wants = !l.on_long_press.is_empty()
                || !l.on_long_press_down.is_empty()
                || !l.on_long_press_start.is_empty()
                || !l.on_long_press_move.is_empty()
                || !l.on_long_press_up.is_empty()
                || !l.on_long_press_end.is_empty()
                || !l.on_long_press_cancel.is_empty();
            if wants { self.render.source_of(rid) } else { None }
        })
    }

    /// Long press: pointer contacted (may begin a long press).
    pub fn dispatch_long_press_down(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_down)
    }
    /// Long press recognized: fires `on_long_press` + `on_long_press_start`.
    pub fn dispatch_long_press_begin(&mut self, source: u64, point: Offset) -> bool {
        let a = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press);
        let b = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_start);
        a | b
    }
    /// Pointer moved during a long press.
    pub fn dispatch_long_press_move(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_move)
    }
    /// Long press ended: fires `on_long_press_up` + `on_long_press_end`.
    pub fn dispatch_long_press_end(&mut self, source: u64, point: Offset) -> bool {
        let a = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_up);
        let b = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_end);
        a | b
    }
    /// A pending long press was cancelled.
    pub fn dispatch_long_press_cancel(&mut self, source: u64) -> bool {
        self.fire_source(source, Offset::ZERO, PointerButton::Primary, |l| &l.on_long_press_cancel)
    }

    /// The source id of the topmost drag (pan) listener under `point`.
    pub fn pan_target_at(&self, point: Offset) -> Option<u64> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            let l = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            if l.wants_pan() { self.render.source_of(rid) } else { None }
        })
    }

    /// A drag began on the target (primary press).
    pub fn dispatch_pan_start(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_pan_start)
    }
    /// The pointer moved during an active drag.
    pub fn dispatch_pan_update(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_pan_update)
    }
    /// The drag ended (primary released).
    pub fn dispatch_pan_end(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_pan_end)
    }

    /// Tertiary (middle) button pressed at `point`.
    pub fn dispatch_tertiary_down(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Middle, |l| &l.on_tertiary_tap_down)
    }
    /// Tertiary (middle) button released at `point`.
    pub fn dispatch_tertiary_up(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Middle, |l| &l.on_tertiary_tap_up)
    }

    /// Update hover state for the pointer at `point`, firing enter/exit callbacks as
    /// the topmost hover-listener changes. Identity is by element id (stable across
    /// rebuilds); exit actions are stored on enter so they fire reliably even after
    /// the widget restyles itself. A pointer staying over the same widget is a no-op.
    pub fn dispatch_hover(&mut self, point: Offset) -> bool {
        let hits = self.render.hit_test(point);
        let found: Option<(u64, Vec<Invoke>, Vec<Invoke>)> = hits.iter().rev().find_map(|&rid| {
            let listener = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            if !listener.wants_hover() {
                return None;
            }
            let source = self.render.source_of(rid)?;
            let enters = Self::invokes_of(listener, |l| &l.on_enter);
            let exits = Self::invokes_of(listener, |l| &l.on_exit);
            Some((source, enters, exits))
        });

        let new_key = found.as_ref().map(|(s, _, _)| *s);
        let old_key = self.hovered.as_ref().map(|h| h.source);
        if new_key == old_key {
            return false; // still over the same widget
        }

        let hover_event =
            PointerEvent { position: point, global: point, button: PointerButton::Primary };
        let mut fired = false;
        if let Some(old) = self.hovered.take() {
            // Only fire the previously-hovered widget's exit handlers if its element
            // still exists. If it unmounted while hovered (e.g. a click swapped the
            // panel out from under the cursor), its handler closures capture now-freed
            // signals — invoking them would use-after-free. Keying by the stable source
            // (not the render id) means a mere re-render that reassigned render ids
            // still counts as "exists", so exit fires correctly.
            if self.render.find_by_source(old.source).is_some() {
                for invoke in old.exits {
                    self.run_invoke(invoke, hover_event);
                    fired = true;
                }
            }
        }
        if let Some((source, enters, exits)) = found {
            for invoke in enters {
                self.run_invoke(invoke, hover_event);
                fired = true;
            }
            self.hovered = Some(HoverTarget { source, exits });
        }
        fired
    }

    /// The cursor icon the topmost hover-listener under `point` requests, if any.
    pub fn cursor_at(&self, point: Offset) -> Option<pebbles_render::Cursor> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?.cursor
        })
    }

    /// Activate the focused widget (Enter/Space). Returns whether handled.
    pub fn dispatch_activate(&mut self) -> bool {
        crate::focus::activate_focused()
    }

    /// Route a keyboard edit intent to the focused text editor. Returns whether an
    /// editor consumed it.
    pub fn dispatch_key(&mut self, key: crate::keyboard::KeyInput) -> bool {
        crate::focus::dispatch_key(key)
    }

    /// Whether the focused node is a text editor (shell key-routing precedence).
    pub fn focused_is_editor(&self) -> bool {
        crate::focus::focused_is_editor()
    }

    /// Move keyboard focus to the next (`forward`) or previous focusable (Tab).
    pub fn focus_move(&mut self, forward: bool) -> bool {
        crate::focus::focus_move(self.ui_id, forward)
    }

    /// Scroll the topmost scrollable under `point` by `delta` (logical px). Returns
    /// `true` if an offset actually changed (caller should relayout + redraw).
    pub fn dispatch_scroll(&mut self, point: Offset, delta: f64) -> bool {
        let hits = self.render.hit_test(point);
        for &rid in hits.iter().rev() {
            // Imperative scroll view: nudge its spring target directly.
            if let Some(s) = self.render.object_ref(rid).downcast_ref::<RenderScroll>() {
                if s.at_edge(delta) {
                    continue; // bubble to an ancestor scroll view
                }
                let moved = self
                    .render
                    .object_mut(rid)
                    .downcast_mut::<RenderScroll>()
                    .is_some_and(|s| s.scroll_by(delta));
                if moved {
                    self.scroll_anim.insert(rid);
                    self.render.mark_needs_layout(rid);
                }
                return moved;
            }
            // Controlled (virtualized) list: route to its offset signal.
            if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
                if list.at_edge(delta) {
                    continue;
                }
                return crate::scroll::dispatch(list.id, ScrollTo::By(delta));
            }
        }
        false
    }

    /// Advance every animating scroll spring by `dt`. Returns whether any are still
    /// moving (the shell keeps requesting frames while true).
    pub fn tick_scrolls(&mut self, dt: f64) -> bool {
        if self.scroll_anim.is_empty() {
            return false;
        }
        for rid in self.scroll_anim.iter().copied().collect::<Vec<_>>() {
            let still = self
                .render
                .try_object_mut(rid)
                .and_then(|o| o.downcast_mut::<RenderScroll>())
                .map(|s| s.tick(dt))
                .unwrap_or(false);
            self.render.mark_needs_layout(rid);
            if !still {
                self.scroll_anim.remove(&rid);
            }
        }
        !self.scroll_anim.is_empty()
    }

    /// The viewport extent of the innermost scroll view under `point`.
    fn viewport_under(&self, point: Offset) -> Option<f64> {
        self.render.hit_test(point).iter().rev().find_map(|&rid| {
            if let Some(s) = self.render.object_ref(rid).downcast_ref::<RenderScroll>() {
                return Some(s.viewport_extent);
            }
            self.render.object_ref(rid).downcast_ref::<RenderList>().map(|l| l.viewport())
        })
    }

    /// Keyboard page scroll (`sign` = +1 down / -1 up) on the scroll view under
    /// `point`. Scrolls ~85% of a viewport.
    pub fn scroll_page(&mut self, point: Offset, sign: f64) -> bool {
        match self.viewport_under(point) {
            Some(vp) => self.dispatch_scroll(point, sign * vp * 0.85),
            None => false,
        }
    }

    /// Keyboard line scroll (`sign` = +1 down / -1 up).
    pub fn scroll_line(&mut self, point: Offset, sign: f64) -> bool {
        self.dispatch_scroll(point, sign * 48.0)
    }

    /// Keyboard Home/End — jump to the start or end of the scroll view under `point`.
    pub fn scroll_to_end(&mut self, point: Offset, end: bool) -> bool {
        let hits = self.render.hit_test(point);
        for &rid in hits.iter().rev() {
            if self.render.object_ref(rid).is::<RenderScroll>() {
                if let Some(s) = self.render.object_mut(rid).downcast_mut::<RenderScroll>() {
                    let to = if end { s.max_offset } else { 0.0 };
                    s.scroll_to(to);
                    self.scroll_anim.insert(rid);
                    self.render.mark_needs_layout(rid);
                }
                return true;
            }
            if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
                let frac = if end { 1.0 } else { 0.0 };
                return crate::scroll::dispatch(list.id, ScrollTo::ToFraction(frac));
            }
        }
        false
    }

    /// Begin a scrollbar drag if `point` lands on a viewport's scrollbar strip.
    /// Returns whether one was grabbed (the shell then routes moves to it).
    pub fn begin_scrollbar_drag(&mut self, point: Offset) -> bool {
        let hits = self.render.hit_test(point);
        for &rid in hits.iter().rev() {
            let local = point - self.render.absolute_offset(rid);
            let size = self.render.size_of(rid);
            if let Some(s) = self.render.object_ref(rid).downcast_ref::<RenderScroll>() {
                if s.scrollbar_hit(local, size) {
                    self.scrollbar_drag = Some(rid);
                    self.update_scrollbar_drag(point);
                    return true;
                }
                return false;
            }
            if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
                if list.scrollbar_hit(local, size) {
                    self.scrollbar_drag = Some(rid);
                    self.update_scrollbar_drag(point);
                    return true;
                }
                return false;
            }
        }
        false
    }

    /// Update the active scrollbar drag to `point`. Returns whether it scrolled.
    pub fn update_scrollbar_drag(&mut self, point: Offset) -> bool {
        let Some(rid) = self.scrollbar_drag else { return false };
        // The scroll view may have unmounted mid-drag (e.g. its overlay closed on a
        // wheel/resize). Drop the stale drag instead of indexing a freed node.
        if !self.render.contains(rid) {
            self.scrollbar_drag = None;
            return false;
        }
        let local = point - self.render.absolute_offset(rid);
        let size = self.render.size_of(rid);
        // Imperative scroll view.
        if let Some(s) = self.render.object_mut(rid).downcast_mut::<RenderScroll>() {
            let changed = s.set_offset_from_point(local, size);
            if changed {
                self.render.mark_needs_layout(rid);
            }
            return changed;
        }
        // Controlled list: map to a fraction and route to its offset signal.
        if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
            let frac = list.fraction_at(local);
            return crate::scroll::dispatch(list.id, ScrollTo::ToFraction(frac));
        }
        false
    }

    /// End any active scrollbar drag.
    pub fn end_scrollbar_drag(&mut self) {
        self.scrollbar_drag = None;
    }

    /// Whether a scrollbar drag is currently active.
    pub fn scrollbar_dragging(&self) -> bool {
        self.scrollbar_drag.is_some()
    }

    fn mark_dirty(&mut self, id: ElementId) {
        if !self.dirty.contains(&id) {
            self.dirty.push(id);
        }
    }

    // ----- layout / paint --------------------------------------------------

    /// Lay the tree out to fill a `window`-sized area.
    pub fn layout(&mut self, text: &mut TextEnv, window: Size) {
        self.render.layout(text, BoxConstraints::tight(window));
    }

    /// Paint the tree into `scene`.
    pub fn paint(&self, scene: &mut Scene) {
        self.render.paint(scene);
    }

    // ----- inflation -------------------------------------------------------

    fn inflate(&mut self, parent: Option<ElementId>, mut widget: AnyWidget) -> ElementId {
        let depth = parent.map(|p| self.elements[p].depth + 1).unwrap_or(0);
        match category(&*widget) {
            Category::Function => {
                let (_, render) = widget.as_component().unwrap();
                let id = self.elements.insert(ElementNode {
                    parent,
                    widget,
                    kind: ElementKind::Function,
                    children: Vec::new(),
                    render_id: None,
                    depth,
                });
                // Run the component with reactive tracking on this element.
                let child_widget = {
                    let guard = crate::reactive::begin_component(id);
                    let out = render();
                    crate::reactive::end_component(guard);
                    out
                };
                let child = self.inflate(Some(id), child_widget);
                self.elements[id].children.push(child);
                id
            }
            Category::Render => {
                let render_object = widget.as_render().unwrap().create_render_object();
                let render_id = self.render.insert(render_object);
                let child_widgets = widget.as_render_mut().unwrap().take_children();
                let id = self.elements.insert(ElementNode {
                    parent,
                    widget,
                    kind: ElementKind::Render,
                    children: Vec::new(),
                    render_id: Some(render_id),
                    depth,
                });
                // Tag the render node with its element id for stable gesture identity.
                self.render.set_source(render_id, id.data().as_ffi());
                let mut children = Vec::with_capacity(child_widgets.len());
                for cw in child_widgets {
                    children.push(self.inflate(Some(id), cw));
                }
                self.elements[id].children = children;
                id
            }
            Category::ParentData => {
                let child_widget = widget.as_parent_data_mut().unwrap().take_child();
                let id = self.elements.insert(ElementNode {
                    parent,
                    widget,
                    kind: ElementKind::ParentData,
                    children: Vec::new(),
                    render_id: None,
                    depth,
                });
                if let Some(cw) = child_widget {
                    let child = self.inflate(Some(id), cw);
                    self.elements[id].children.push(child);
                }
                id
            }
        }
    }

    // ----- reconciliation --------------------------------------------------

    /// The core reconcile primitive: given the (optional) existing element and the
    /// (optional) new widget for one slot, return the resulting element id.
    fn update_child(
        &mut self,
        parent: ElementId,
        old: Option<ElementId>,
        new_widget: Option<AnyWidget>,
    ) -> Option<ElementId> {
        match (old, new_widget) {
            (None, None) => None,
            (Some(old), None) => {
                self.unmount(old);
                None
            }
            (None, Some(w)) => Some(self.inflate(Some(parent), w)),
            (Some(old), Some(w)) => {
                if self.can_update(old, &*w) {
                    self.update(old, w);
                    Some(old)
                } else {
                    self.unmount(old);
                    Some(self.inflate(Some(parent), w))
                }
            }
        }
    }

    /// Two widgets are compatible (element reused) iff same concrete type + key —
    /// and, for function components, the same component function.
    fn can_update(&self, old: ElementId, new_widget: &dyn Widget) -> bool {
        let existing = &self.elements[old].widget;
        let same_type = (existing.as_any() as &dyn Any).type_id()
            == (new_widget.as_any() as &dyn Any).type_id();
        if !same_type || existing.key() != new_widget.key() {
            return false;
        }
        match (existing.as_component(), new_widget.as_component()) {
            (Some((a, _)), Some((b, _))) => a == b,
            _ => true,
        }
    }

    fn update(&mut self, id: ElementId, mut new_widget: AnyWidget) {
        match category(&*new_widget) {
            Category::Function => {
                let (_, render) = new_widget.as_component().unwrap();
                self.elements[id].widget = new_widget;
                let child_widget = {
                    let guard = crate::reactive::begin_component(id);
                    let out = render();
                    crate::reactive::end_component(guard);
                    out
                };
                let old_child = self.elements[id].children.first().copied();
                let new_child = self.update_child(id, old_child, Some(child_widget));
                self.elements[id].children = new_child.into_iter().collect();
            }
            Category::Render => {
                let child_widgets = new_widget.as_render_mut().unwrap().take_children();
                if let Some(rid) = self.elements[id].render_id {
                    new_widget.as_render().unwrap().update_render_object(self.render.object_mut(rid));
                }
                self.elements[id].widget = new_widget;
                self.reconcile_children(id, child_widgets);
            }
            Category::ParentData => {
                let child_widget = new_widget.as_parent_data_mut().unwrap().take_child();
                self.elements[id].widget = new_widget;
                let old_child = self.elements[id].children.first().copied();
                let new_child = self.update_child(id, old_child, child_widget);
                self.elements[id].children = new_child.into_iter().collect();
            }
        }
    }

    /// Reconcile a render element's child list against `new_widgets`, matched by
    /// index (keyed reordering is a later refinement).
    fn reconcile_children(&mut self, parent: ElementId, new_widgets: Vec<AnyWidget>) {
        let old_children = self.elements[parent].children.clone();
        let mut incoming: Vec<Option<AnyWidget>> = new_widgets.into_iter().map(Some).collect();
        let count = old_children.len().max(incoming.len());
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let old = old_children.get(i).copied();
            let new_widget = incoming.get_mut(i).and_then(|slot| slot.take());
            if let Some(el) = self.update_child(parent, old, new_widget) {
                result.push(el);
            }
        }
        self.elements[parent].children = result;
    }

    fn rebuild_element(&mut self, id: ElementId) {
        // Only function components are ever marked dirty (via their reactive signals);
        // render/parent-data elements are reconciled top-down by their parent's rebuild.
        debug_assert!(
            matches!(self.elements[id].kind, ElementKind::Function),
            "a dirty element must be a function component"
        );
        let (_, render) = self.elements[id]
            .widget
            .as_component()
            .expect("a dirty element must be a function component");
        let child_widget = {
            let guard = crate::reactive::begin_component(id);
            let out = render();
            crate::reactive::end_component(guard);
            out
        };
        let old_child = self.elements[id].children.first().copied();
        let new_child = self.update_child(id, old_child, Some(child_widget));
        self.elements[id].children = new_child.into_iter().collect();
    }

    /// Tear down this window's whole tree: unmount every element — running component
    /// cleanups (animation loops, timeouts, focus/scroll registrations) — and free its
    /// signals from the shared runtime. The shell calls this when the OS window
    /// closes. Without it the closed window's components stay alive in the shared
    /// runtime: any `create_loop` in them pins the frame loop at full rate forever,
    /// and every open/close cycle leaks the whole tree.
    pub fn dispose(&mut self) {
        self.make_current();
        if let Some(root) = self.root.take() {
            self.unmount(root);
        }
        // Drop any re-render requests already queued for this window — nothing will
        // ever drain them again.
        let _ = crate::reactive::take_pending_components(self.ui_id);
        self.dirty.clear();
        self.hovered = None;
        self.scrollbar_drag = None;
        self.scroll_anim.clear();
    }

    fn unmount(&mut self, id: ElementId) {
        let children = std::mem::take(&mut self.elements[id].children);
        for child in children {
            self.unmount(child);
        }
        if let ElementKind::Function = &self.elements[id].kind {
            crate::reactive::dispose_component(id);
            crate::focus::unregister(id);
        }
        if let Some(rid) = self.elements[id].render_id {
            self.render.remove_node(rid);
        }
        self.dirty.retain(|&d| d != id);
        self.elements.remove(id);
    }

    // ----- render-tree projection -----------------------------------------

    /// Re-project the element tree onto the render tree: every render element's
    /// render node gets, as its children, the nearest render descendants of its
    /// child elements — skipping component/stateful elements that own no render
    /// object. O(n) per build; simple and correct.
    fn sync_render(&mut self, el: ElementId) {
        if let Some(rid) = self.elements[el].render_id {
            let kids = self.render_children_of(el);
            self.render.set_children(rid, kids);
        }
        for child in self.elements[el].children.clone() {
            self.sync_render(child);
        }
    }

    fn render_children_of(&self, el: ElementId) -> Vec<RenderId> {
        let mut out = Vec::new();
        for &child in &self.elements[el].children {
            if let Some(rid) = self.elements[child].render_id {
                out.push(rid);
            } else {
                out.extend(self.render_children_of(child));
            }
        }
        out
    }

    /// Walk the element tree; for each `ParentData` element, attach its parent data
    /// to its nearest render descendant — the render node that is a direct child of
    /// the enclosing flex/stack, where that container reads it during layout.
    fn apply_parent_data(&mut self, el: ElementId) {
        if let Some(pd) = self.elements[el].widget.as_parent_data() {
            let data = pd.parent_data();
            if let Some(rid) = self.render_children_of(el).first().copied() {
                // A `Semantics` widget rides the same parent-data channel but routes to
                // the node's accessibility slot instead of its layout parent-data.
                match data.downcast::<pebbles_render::SemanticsProps>() {
                    Ok(props) => self.render.set_semantics(rid, *props),
                    Err(data) => self.render.set_parent_data(rid, data),
                }
            }
        }
        for child in self.elements[el].children.clone() {
            self.apply_parent_data(child);
        }
    }
}

