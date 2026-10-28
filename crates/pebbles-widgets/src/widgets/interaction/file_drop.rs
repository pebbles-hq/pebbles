//! [`FileDropTarget`] — receive files dragged in from the OS file manager
//! (the `desktop_drop` package's `DropTarget` for Flutter desktop).
//!
//! ```ignore
//! file_drop_target(|hovering| zone(hovering))
//!     .on_drag_entered(|paths| println!("{} file(s) over us", paths.len()))
//!     .on_drag_exited(|| println!("left"))
//!     .on_drop(move |paths| files.set(paths));
//! ```
//!
//! The shell feeds the platform's file-drag events into the free functions of this
//! module ([`hover`], [`move_to`], [`drop_file`], [`flush`], [`cancel`]); tests can
//! call them directly to simulate a drag from the file manager.
//!
//! **Which target gets the files.** The smallest target whose on-screen rect holds
//! the pointer. Desktop platforms often do not report pointer motion during an OS
//! drag (winit's X11 backend sends no `CursorMoved` while an XDND drag is over the
//! window, so the last known position is stale), so when no target holds the
//! pointer and the window has exactly one target, that target is used — a single
//! drop zone behaves like a window-wide one.
//!
//! **Platform note.** winit 0.30 implements file drag-and-drop on X11 (and XWayland),
//! Windows and macOS, but not on native Wayland: under a Wayland session the drop
//! events never arrive unless the app runs through XWayland (`WAYLAND_DISPLAY=`
//! unset, or `WINIT_UNIX_BACKEND=x11`).

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use pebbles_foundation::{Offset, Rect};

use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{
    Element, Signal, component_props, create_cleanup, create_root_signal, create_signal, use_bounds,
};

/// Called with the hovering paths when a file drag enters a target.
type PathsFn = Rc<dyn Fn(&[PathBuf])>;
/// Called when a file drag leaves a target (or is cancelled).
type ExitFn = Rc<dyn Fn()>;
/// Called with every dropped path, once per drop.
type DropFn = Rc<dyn Fn(Vec<PathBuf>)>;
/// Content builder — `true` while files hover this target.
type BuilderFn = Rc<dyn Fn(bool) -> AnyWidget>;

/// The in-progress OS file drag.
#[derive(Clone, Debug, PartialEq)]
struct FileDrag {
    paths: Vec<PathBuf>,
    pointer: Offset,
    /// The target currently under the drag (it has had `on_drag_entered`).
    target: Option<u64>,
}

struct TargetReg {
    id: u64,
    rect: Rect,
    on_enter: Option<PathsFn>,
    on_exit: Option<ExitFn>,
    on_drop: Option<DropFn>,
}

thread_local! {
    static SESSION: RefCell<Option<Signal<Option<FileDrag>>>> = const { RefCell::new(None) };
    static TARGETS: RefCell<Vec<TargetReg>> = const { RefCell::new(Vec::new()) };
    static DROPPED: RefCell<Vec<PathBuf>> = const { RefCell::new(Vec::new()) };
    static DROP_AT: Cell<Offset> = const { Cell::new(Offset::ZERO) };
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

fn session() -> Signal<Option<FileDrag>> {
    SESSION.with(|s| {
        if s.borrow().is_none() {
            *s.borrow_mut() = Some(create_root_signal::<Option<FileDrag>>(None));
        }
        s.borrow().expect("session initialized above")
    })
}

/// The target a drag at `pointer` lands on (see the module docs).
fn pick(pointer: Offset) -> Option<u64> {
    TARGETS.with(|t| {
        let t = t.borrow();
        t.iter()
            .filter(|r| r.rect.contains(pointer.to_point()))
            .min_by(|a, b| (a.rect.width() * a.rect.height()).total_cmp(&(b.rect.width() * b.rect.height())))
            .map(|r| r.id)
            .or_else(|| (t.len() == 1).then(|| t[0].id))
    })
}

fn handlers(id: u64) -> (Option<PathsFn>, Option<ExitFn>, Option<DropFn>) {
    TARGETS.with(|t| {
        t.borrow()
            .iter()
            .find(|r| r.id == id)
            .map(|r| (r.on_enter.clone(), r.on_exit.clone(), r.on_drop.clone()))
            .unwrap_or((None, None, None))
    })
}

/// Move the drag to `new`, firing exit on the old target and enter on the new one.
fn retarget(st: &mut FileDrag, new: Option<u64>) {
    if st.target == new {
        return;
    }
    if let Some(old) = st.target
        && let (_, Some(exit), _) = handlers(old)
    {
        exit();
    }
    st.target = new;
    if let Some(id) = new
        && let (Some(enter), _, _) = handlers(id)
    {
        enter(&st.paths);
    }
}

/// A file from the OS is being dragged over the window at `pointer` (winit
/// `WindowEvent::HoveredFile`, one event per file). Returns whether anything changed.
pub fn hover(path: PathBuf, pointer: Offset) -> bool {
    let s = session();
    let mut st = s.peek().unwrap_or(FileDrag { paths: Vec::new(), pointer, target: None });
    let fresh = !st.paths.contains(&path);
    if fresh {
        st.paths.push(path);
    }
    st.pointer = pointer;
    let old = st.target;
    retarget(&mut st, pick(pointer));
    let changed = fresh || old != st.target;
    s.set(Some(st));
    changed
}

/// The pointer moved during a file drag. Returns whether the hovered target changed.
pub fn move_to(pointer: Offset) -> bool {
    let s = session();
    let Some(mut st) = s.peek() else { return false };
    st.pointer = pointer;
    let old = st.target;
    retarget(&mut st, pick(pointer));
    let changed = old != st.target;
    s.set(Some(st));
    changed
}

/// A file was dropped at `pointer` (winit `WindowEvent::DroppedFile`, one event per
/// file). Queued until [`flush`] so a multi-file drop reaches `on_drop` once.
pub fn drop_file(path: PathBuf, pointer: Offset) {
    DROPPED.with(|d| d.borrow_mut().push(path));
    DROP_AT.with(|p| p.set(pointer));
}

/// Deliver the queued drop (the shell calls this once the platform's batch of
/// `DroppedFile` events is done). Returns whether a drop was delivered.
pub fn flush() -> bool {
    let paths = DROPPED.with(|d| std::mem::take(&mut *d.borrow_mut()));
    if paths.is_empty() {
        return false;
    }
    let s = session();
    let target = pick(DROP_AT.with(Cell::get));
    s.set(None);
    if let Some(id) = target
        && let (_, _, Some(on_drop)) = handlers(id)
    {
        on_drop(paths);
    }
    true
}

/// The file drag left the window or was cancelled (winit
/// `WindowEvent::HoveredFileCancelled`). Returns whether a drag was in progress.
pub fn cancel() -> bool {
    let s = session();
    let Some(mut st) = s.peek() else { return false };
    retarget(&mut st, None);
    s.set(None);
    true
}

/// The paths of the file drag currently over the window (empty when none).
pub fn hovered_paths() -> Vec<PathBuf> {
    session().peek().map(|d| d.paths).unwrap_or_default()
}

/// A drop zone for files dragged in from the OS. Built by [`file_drop_target`].
#[derive(Clone)]
pub struct FileDropTarget {
    builder: BuilderFn,
    on_enter: Option<PathsFn>,
    on_exit: Option<ExitFn>,
    on_drop: Option<DropFn>,
}

/// A file drop zone whose `builder(hovering)` renders its content — `hovering` is
/// `true` while OS files are dragged over it. Attach [`FileDropTarget::on_drop`].
pub fn file_drop_target<W: IntoWidget>(builder: impl Fn(bool) -> W + 'static) -> FileDropTarget {
    FileDropTarget {
        builder: Rc::new(move |h| builder(h).into_widget()),
        on_enter: None,
        on_exit: None,
        on_drop: None,
    }
}

impl FileDropTarget {
    /// Called with the dragged paths when a file drag enters this target.
    pub fn on_drag_entered(mut self, f: impl Fn(&[PathBuf]) + 'static) -> Self {
        self.on_enter = Some(Rc::new(f));
        self
    }
    /// Called when the file drag leaves this target or is cancelled.
    pub fn on_drag_exited(mut self, f: impl Fn() + 'static) -> Self {
        self.on_exit = Some(Rc::new(f));
        self
    }
    /// Called once per drop with every dropped path.
    pub fn on_drop(mut self, f: impl Fn(Vec<PathBuf>) + 'static) -> Self {
        self.on_drop = Some(Rc::new(f));
        self
    }
}

impl IntoWidget for FileDropTarget {
    fn into_widget(self) -> AnyWidget {
        component_props(render_file_drop_target, self).into_widget()
    }
}

fn render_file_drop_target(t: &FileDropTarget) -> Element {
    let id_sig = create_signal(0u64);
    if id_sig.peek() == 0 {
        let id = NEXT_ID.with(|n| {
            let id = n.get();
            n.set(id + 1);
            id
        });
        id_sig.set(id);
        create_cleanup(move || TARGETS.with(|t| t.borrow_mut().retain(|r| r.id != id)));
    }
    let id = id_sig.peek();
    let rect = use_bounds();
    TARGETS.with(|regs| {
        let mut regs = regs.borrow_mut();
        let reg = TargetReg {
            id,
            rect,
            on_enter: t.on_enter.clone(),
            on_exit: t.on_exit.clone(),
            on_drop: t.on_drop.clone(),
        };
        match regs.iter_mut().find(|r| r.id == id) {
            Some(existing) => *existing = reg,
            None => regs.push(reg),
        }
    });
    let hovering = session().get().is_some_and(|d| d.target == Some(id));
    (t.builder)(hovering)
}
