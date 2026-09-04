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

use pebbles_foundation::{Axis, Offset, Size};
use pebbles_render::{
    BoxConstraints, PointerButton, PointerEvent, RenderId, RenderList, RenderPointerListener, RenderScroll,
    RenderTree, Scene, TextEnv,
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
    /// The drag-scroll viewport a content drag is currently driving, if any.
    content_drag: Option<RenderId>,
    /// Test override for [`Ui::clock_now`] (drag/fling velocity estimation);
    /// `None` uses the wall clock.
    clock_override: Option<f64>,
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

mod build;
mod dispatch;

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

    /// Number of live elements in this window's tree (debug observability).
    #[cfg(debug_assertions)]
    pub fn element_count(&self) -> usize {
        self.elements.len()
    }

    /// Number of live render nodes in this window's tree (debug observability).
    #[cfg(debug_assertions)]
    pub fn render_node_count(&self) -> usize {
        self.render.node_count()
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

    /// Paint the tree into `scene`. `text` gives paint-time shaping access
    /// (P5.2 lazy text materialization). Returns `true` when a lazy measurement
    /// changed estimated geometry and a **corrective relayout** is needed — the
    /// affected nodes are already marked dirty; the caller schedules one more
    /// frame (headless drivers: loop `layout` + `paint` until it returns false).
    pub fn paint(&mut self, text: &mut TextEnv, scene: &mut Scene) -> bool {
        let pending = self.render.paint(text, scene);
        let corrective = !pending.is_empty();
        for id in pending {
            self.render.mark_needs_layout(id);
        }
        corrective
    }
}
