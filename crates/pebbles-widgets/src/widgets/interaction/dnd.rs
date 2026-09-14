//! [`Draggable`] / [`DragTarget`] / [`long_press_draggable`] — general drag-and-drop
//! (Flutter's `Draggable` / `LongPressDraggable` / `DragTarget`).
//!
//! A [`draggable`] carries a typed payload and, while dragged, renders a `feedback`
//! widget that follows the pointer in a window overlay. A [`drag_target`] publishes
//! its on-screen rect and, on release over it, receives the payload (downcast to the
//! target's type) via `on_accept`. Targets highlight while a compatible payload hovers.
//!
//! ```ignore
//! draggable(Fruit::Apple, chip("Apple")).feedback(chip("Apple"));
//! drag_target(|hovering| basket(hovering))
//!     .on_accept::<Fruit>(move |f| picked.update(|v| v.push(*f)));
//! ```
//!
//! Payloads are matched by type: `drag_target(..).on_accept::<T>(..)` only accepts —
//! and only highlights for — draggables whose data is a `T` (override the predicate
//! with [`DragTarget::on_will_accept`]).

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use pebbles_foundation::{Offset, Rect};

use crate::widgets::{GestureDetector, SizedBox, ignore_pointer, positioned, stack};
use crate::{hide_overlay, show_overlay};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{
    Element, Signal, action_event, component_props, create_cleanup, create_root_signal, create_signal,
    use_bounds,
};

/// A type-erased drag payload (downcast to the target's type on accept).
type DragData = Rc<dyn Any>;
/// A drop-acceptance predicate over a payload.
type AcceptFn = Rc<dyn Fn(&DragData) -> bool>;
/// A drop handler invoked with the accepted payload.
type HandlerFn = Rc<dyn Fn(&DragData)>;
/// A drag-target content builder — `true` while a compatible payload hovers.
type BuilderFn = Rc<dyn Fn(bool) -> AnyWidget>;

/// The live drag payload + geometry, shared through a root signal so the feedback
/// overlay and every drag target re-render as the pointer moves.
#[derive(Clone)]
struct DragState {
    data: DragData,
    /// Current pointer position, window coordinates.
    pointer: Offset,
    /// Pointer offset within the source at grab time, so feedback stays under the cursor.
    anchor: Offset,
    feedback: AnyWidget,
    width: f64,
    height: f64,
}

/// A registered drop target: its current rect + the (type-erased) accept predicate
/// and handler, so a drag release can find and notify it without a shared context.
struct TargetReg {
    id: u64,
    rect: Rect,
    accepts: AcceptFn,
    on_accept: HandlerFn,
}

thread_local! {
    static SESSION: RefCell<Option<Signal<Option<DragState>>>> = const { RefCell::new(None) };
    static TARGETS: RefCell<Vec<TargetReg>> = const { RefCell::new(Vec::new()) };
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

/// The app-wide drag session signal (created once, lives for the app's lifetime).
fn session() -> Signal<Option<DragState>> {
    SESSION.with(|s| {
        if s.borrow().is_none() {
            *s.borrow_mut() = Some(create_root_signal::<Option<DragState>>(None));
        }
        s.borrow().expect("session initialized above")
    })
}

fn next_id() -> u64 {
    NEXT_ID.with(|n| {
        let id = n.get();
        n.set(id + 1);
        id
    })
}

fn contains(rect: Rect, p: Offset) -> bool {
    rect.contains(p.to_point())
}

// ===========================================================================
// Draggable
// ===========================================================================

/// A widget that can be picked up and dragged onto a [`DragTarget`]. Built by
/// [`draggable`] / [`long_press_draggable`].
#[derive(Clone)]
pub struct Draggable {
    data: DragData,
    child: AnyWidget,
    feedback: Option<AnyWidget>,
    child_when_dragging: Option<AnyWidget>,
    long_press: bool,
}

/// A draggable carrying `data` (any `'static` type — the drop target downcasts it).
/// The drag begins as soon as the pointer moves.
pub fn draggable<T: 'static>(data: T, child: impl IntoWidget) -> Draggable {
    Draggable {
        data: Rc::new(data),
        child: child.into_widget(),
        feedback: None,
        child_when_dragging: None,
        long_press: false,
    }
}

/// A [`draggable`] whose drag begins only after a long press (Flutter's
/// `LongPressDraggable`) — useful when the child also scrolls or taps.
pub fn long_press_draggable<T: 'static>(data: T, child: impl IntoWidget) -> Draggable {
    Draggable { long_press: true, ..draggable(data, child) }
}

impl Draggable {
    /// The widget that follows the pointer while dragging (default: the child).
    pub fn feedback(mut self, feedback: impl IntoWidget) -> Self {
        self.feedback = Some(feedback.into_widget());
        self
    }
    /// What to show in the child's original place while it's being dragged
    /// (default: the child, unchanged).
    pub fn child_when_dragging(mut self, child: impl IntoWidget) -> Self {
        self.child_when_dragging = Some(child.into_widget());
        self
    }
}

impl IntoWidget for Draggable {
    fn into_widget(self) -> AnyWidget {
        component_props(render_draggable, self).into_widget()
    }
}

fn render_draggable(d: &Draggable) -> Element {
    let bounds = use_bounds(); // this draggable's window rect (for feedback size)
    let dragging = create_signal(false);

    let shown = if dragging.get() {
        d.child_when_dragging.clone().unwrap_or_else(|| d.child.clone())
    } else {
        d.child.clone()
    };

    let data = d.data.clone();
    let feedback = d.feedback.clone().unwrap_or_else(|| d.child.clone());
    let (w, h) = (bounds.width(), bounds.height());

    let start = {
        let data = data.clone();
        let feedback = feedback.clone();
        move |e: pebbles_render::PointerEvent| {
            let sess = session();
            sess.set(Some(DragState {
                data: data.clone(),
                pointer: e.global,
                anchor: e.position,
                feedback: feedback.clone(),
                width: w,
                height: h,
            }));
            dragging.set(true);
            show_overlay(
                component_props(render_feedback, FeedbackProps { sess }).into_widget(),
                0.0,
                0.0,
                1.0e6,
                1.0e6,
            );
        }
    };
    let update = move |e: pebbles_render::PointerEvent| {
        session().update(|s| {
            if let Some(st) = s {
                st.pointer = e.global;
            }
        });
    };
    let end = move || {
        let sess = session();
        if let Some(st) = sess.peek() {
            let pointer = st.pointer;
            // Deliver to the topmost (smallest) accepting target under the pointer.
            let hit = TARGETS.with(|t| {
                t.borrow()
                    .iter()
                    .filter(|reg| contains(reg.rect, pointer) && (reg.accepts)(&st.data))
                    .min_by(|a, b| {
                        (a.rect.width() * a.rect.height()).total_cmp(&(b.rect.width() * b.rect.height()))
                    })
                    .map(|reg| reg.on_accept.clone())
            });
            if let Some(on_accept) = hit {
                on_accept(&st.data);
            }
        }
        sess.set(None);
        dragging.set(false);
        hide_overlay();
    };

    let gd = GestureDetector::new(shown);
    if d.long_press {
        gd.on_long_press_start(action_event(start))
            .on_long_press_move(action_event(update))
            .on_long_press_end(end)
            .into_widget()
    } else {
        gd.on_pan_start(action_event(start)).on_pan_update(action_event(update)).on_pan_end(end).into_widget()
    }
}

/// Props for the feedback overlay (a one-field wrapper so the render fn takes a
/// borrow of a struct, not a bare `Copy` signal).
#[derive(Clone)]
struct FeedbackProps {
    sess: Signal<Option<DragState>>,
}

/// The window overlay that renders the dragged feedback under the pointer.
fn render_feedback(p: &FeedbackProps) -> Element {
    match p.sess.get() {
        Some(st) => {
            let left = st.pointer.x - st.anchor.x;
            let top = st.pointer.y - st.anchor.y;
            // `ignore_pointer` so the feedback never interferes with hit testing.
            let fb = ignore_pointer(SizedBox::exact(st.width, st.height, st.feedback.clone()));
            stack(vec![positioned(fb).left(left).top(top).into_widget()]).into_widget()
        }
        None => stack(Vec::<AnyWidget>::new()).into_widget(),
    }
}

// ===========================================================================
// DragTarget
// ===========================================================================

/// A drop zone that receives a [`Draggable`]'s payload on release. Built by
/// [`drag_target`]; its `builder` is passed whether a compatible payload is
/// currently hovering, so it can highlight.
#[derive(Clone)]
pub struct DragTarget {
    builder: BuilderFn,
    will_accept: Option<AcceptFn>,
    type_check: AcceptFn,
    on_accept: HandlerFn,
}

/// A drag target whose `builder(hovering)` renders its content — `hovering` is `true`
/// while a payload it will accept is over it. Attach [`DragTarget::on_accept`].
pub fn drag_target<W: IntoWidget>(builder: impl Fn(bool) -> W + 'static) -> DragTarget {
    DragTarget {
        builder: Rc::new(move |h| builder(h).into_widget()),
        will_accept: None,
        type_check: Rc::new(|_| true),
        on_accept: Rc::new(|_| {}),
    }
}

impl DragTarget {
    /// Called with the payload (downcast to `T`) when a `T` is dropped here. Setting
    /// this also makes the target accept — and highlight for — only `T` payloads,
    /// unless you override with [`DragTarget::on_will_accept`].
    pub fn on_accept<T: 'static>(mut self, f: impl Fn(&T) + 'static) -> Self {
        self.on_accept = Rc::new(move |d: &DragData| {
            if let Some(v) = d.downcast_ref::<T>() {
                f(v);
            }
        });
        self.type_check = Rc::new(|d: &DragData| d.downcast_ref::<T>().is_some());
        self
    }

    /// A custom predicate deciding whether a `T` payload is acceptable (default: any
    /// `T`). Overrides the type-only check from [`DragTarget::on_accept`].
    pub fn on_will_accept<T: 'static>(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.will_accept = Some(Rc::new(move |d: &DragData| d.downcast_ref::<T>().map(&f).unwrap_or(false)));
        self
    }
}

impl IntoWidget for DragTarget {
    fn into_widget(self) -> AnyWidget {
        component_props(render_drag_target, self).into_widget()
    }
}

fn render_drag_target(t: &DragTarget) -> Element {
    let id_sig = create_signal(0u64);
    if id_sig.peek() == 0 {
        id_sig.set(next_id());
        let id = id_sig.peek();
        create_cleanup(move || {
            TARGETS.with(|t| t.borrow_mut().retain(|r| r.id != id));
        });
    }
    let id = id_sig.peek();
    let rect = use_bounds();
    let accepts = t.will_accept.clone().unwrap_or_else(|| t.type_check.clone());

    // Keep this target's registration current (rect + closures) for drop delivery.
    TARGETS.with(|regs| {
        let mut regs = regs.borrow_mut();
        let reg = TargetReg { id, rect, accepts: accepts.clone(), on_accept: t.on_accept.clone() };
        match regs.iter_mut().find(|r| r.id == id) {
            Some(existing) => *existing = reg,
            None => regs.push(reg),
        }
    });

    // Highlight while an acceptable payload hovers over us.
    let hovering = match session().get() {
        Some(st) => contains(rect, st.pointer) && accepts(&st.data),
        None => false,
    };
    (t.builder)(hovering)
}
