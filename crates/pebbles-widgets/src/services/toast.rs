//! [`toast`] — transient, non-blocking notifications (Sonner-style), stacked in the
//! bottom-right corner and auto-dismissed after a duration. An app service like
//! [`dialog`](fn@crate::dialog): call `toast("Saved").show()` from anywhere; the
//! [`OverlayHost`](crate::overlay::OverlayHost) renders the current window's stack
//! topmost. Per-window (namespaced like the overlay/dialog signals).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets, MainAxisSize, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, IconKind};

use crate::components::icon;
use crate::overlay::window_size;
use crate::theme::theme;
use crate::widgets::{
    Container, GestureDetector, Opacity, Positioned, Transform, column, gap_h, gap_w, row, spacer, text,
};
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animation, component_props, create_root_signal, create_signal};

/// Identifies a shown toast (for [`dismiss_toast`]).
pub type ToastId = u64;

/// The tone of a toast — sets the leading icon + accent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ToastVariant {
    #[default]
    Default,
    Success,
    Warning,
    Destructive,
}

#[derive(Clone)]
struct ToastEntry {
    id: ToastId,
    title: String,
    description: Option<String>,
    variant: ToastVariant,
    action: Option<(String, Rc<dyn Fn()>)>,
    dismissible: bool,
    style: Option<crate::style::Style>,
    /// Original auto-dismiss duration (0 = sticky). Used to re-arm after a hover-pause.
    duration: f64,
    /// True once dismissal started: the card animates out and is removed after
    /// [`EXIT_SECS`] (deferred removal). Reactive via the stack signal.
    leaving: bool,
    /// Seconds left on the auto-dismiss timer — decremented on hover-enter, used to
    /// re-arm on hover-exit (C1 hover-pause). Shared handle (no per-toast signal).
    remaining: Rc<Cell<f64>>,
    /// The animation-clock time the auto-dismiss timer was last (re)armed.
    armed_at: Rc<Cell<f64>>,
}

/// Enter/exit tween length (seconds) — also the deferred-removal delay.
const MOTION_SECS: f64 = 0.18;

/// The most a stack shows at once; older toasts queue behind these.
const MAX_VISIBLE: usize = 3;
const WIDTH: f64 = 340.0;

thread_local! {
    static NEXT_ID: Cell<ToastId> = const { Cell::new(1) };
    static TOASTS: RefCell<HashMap<u32, Signal<Vec<ToastEntry>>>> = RefCell::new(HashMap::new());
}

fn stack_signal() -> Signal<Vec<ToastEntry>> {
    let window = current_window();
    TOASTS.with(|cell| *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(Vec::new())))
}

/// Forget a closed window's toast stack (clear + drop its signal — window ids are
/// never reused).
pub(crate) fn drop_window(window: u32) {
    if let Some(sig) = TOASTS.with(|m| m.borrow_mut().remove(&window)) {
        sig.set(Vec::new());
    }
}

/// Whether any toast is currently showing in the current window.
pub fn any_open() -> bool {
    !stack_signal().peek().is_empty()
}

/// Begin dismissing a toast: cancels its auto-dismiss timer, flags it `leaving` (so
/// the card animates out), and removes it for real once the exit tween ends
/// (one motion cycle). Idempotent — calling it again while a toast is already leaving
/// does nothing (Escape/scrim/action can all land during the exit window).
pub fn dismiss_toast(id: ToastId) {
    let state = stack_signal().peek().iter().find(|t| t.id == id).map(|t| t.leaving);
    if state != Some(false) {
        return; // absent, or already leaving
    }
    stack_signal().update(|v| {
        if let Some(t) = v.iter_mut().find(|t| t.id == id) {
            t.leaving = true;
        }
    });
    animation::clear_timeout(timer_key(id)); // stop the auto-dismiss countdown
    animation::set_timeout(removal_key(id), MOTION_SECS, move || remove_toast(id));
}

/// Remove a toast from the stack for real (after its exit tween) and clear its timers.
fn remove_toast(id: ToastId) {
    animation::clear_timeout(timer_key(id));
    animation::clear_timeout(removal_key(id));
    stack_signal().update(|v| v.retain(|t| t.id != id));
}

/// A namespaced timer key so a toast's auto-dismiss can't collide with other timers.
fn timer_key(id: ToastId) -> u64 {
    0x7040_0000_0000_0000 ^ id
}

/// The deferred-removal timer key (distinct namespace from the auto-dismiss timer).
fn removal_key(id: ToastId) -> u64 {
    0x7041_0000_0000_0000 ^ id
}

/// A toast to show. Build it, then [`show`](Toast::show).
pub struct Toast {
    title: String,
    description: Option<String>,
    variant: ToastVariant,
    duration: f64,
    action: Option<(String, Rc<dyn Fn()>)>,
    dismissible: bool,
    style: Option<crate::style::Style>,
}

/// Create a [`Toast`] with the given title.
pub fn toast(title: impl Into<String>) -> Toast {
    Toast {
        title: title.into(),
        description: None,
        variant: ToastVariant::Default,
        duration: 4.0,
        action: None,
        dismissible: true,
        style: None,
    }
}

impl Toast {
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    pub fn variant(mut self, variant: ToastVariant) -> Self {
        self.variant = variant;
        self
    }
    /// Seconds before auto-dismiss (default 4). `0` disables auto-dismiss.
    pub fn duration(mut self, secs: f64) -> Self {
        self.duration = secs;
        self
    }
    /// An action button (label + handler); tapping it also dismisses the toast.
    pub fn action(mut self, label: impl Into<String>, f: impl Fn() + 'static) -> Self {
        self.action = Some((label.into(), Rc::new(f)));
        self
    }
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
    /// Merge a [`Style`](crate::Style) onto the toast surface.
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
    /// Show the toast; returns its [`ToastId`]. Auto-dismisses after `duration` (unless 0).
    pub fn show(self) -> ToastId {
        let id = NEXT_ID.with(|n| {
            let v = n.get();
            n.set(v + 1);
            v
        });
        stack_signal().update(|v| {
            v.push(ToastEntry {
                id,
                title: self.title,
                description: self.description,
                variant: self.variant,
                action: self.action,
                dismissible: self.dismissible,
                style: self.style,
                duration: self.duration,
                leaving: false,
                remaining: Rc::new(Cell::new(self.duration)),
                armed_at: Rc::new(Cell::new(animation::now())),
            });
        });
        if self.duration > 0.0 {
            animation::set_timeout(timer_key(id), self.duration, move || dismiss_toast(id));
        }
        id
    }
}

fn variant_icon(v: ToastVariant) -> Option<(IconKind, Color)> {
    let c = theme().colors;
    match v {
        ToastVariant::Default => None,
        ToastVariant::Success => Some((IconKind::Check, palette_ok())),
        ToastVariant::Warning => Some((IconKind::Warning, palette_warn())),
        ToastVariant::Destructive => Some((IconKind::Warning, c.destructive)),
    }
}
fn palette_ok() -> Color {
    Color::from_rgba8(34, 197, 94, 255)
}
fn palette_warn() -> Color {
    Color::from_rgba8(234, 179, 8, 255)
}

/// The static visual (surface + content + Alert semantics), without motion/hover.
fn toast_card_inner(e: &ToastEntry) -> AnyWidget {
    let c = theme().colors;
    let mut left: Vec<AnyWidget> = Vec::new();
    if let Some((ic, tint)) = variant_icon(e.variant) {
        left.push(icon(ic).size(18.0).color(tint).into_widget());
        left.push(gap_w(10.0).into_widget());
    }
    let mut textcol: Vec<AnyWidget> =
        vec![text(e.title.clone()).size(14.0).weight(600.0).color(c.foreground).into_widget()];
    if let Some(d) = &e.description {
        textcol.push(gap_h(3.0).into_widget());
        textcol.push(text(d.clone()).size(12.5).color(c.muted_foreground).into_widget());
    }
    left.push(
        column(textcol)
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .into_widget(),
    );

    let mut r: Vec<AnyWidget> =
        vec![row(left).main_axis_size(MainAxisSize::Min).into_widget(), spacer().into_widget()];
    if let Some((label, f)) = &e.action {
        let id = e.id;
        let f = f.clone();
        r.push(
            GestureDetector::new(
                Container::new()
                    .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(6.0)))
                    .padding(EdgeInsets::symmetric(10.0, 6.0))
                    .child(text(label.clone()).size(12.5).weight(500.0).color(c.secondary_foreground)),
            )
            .cursor(pebbles_render::Cursor::Pointer)
            .on_tap(move || {
                f();
                dismiss_toast(id);
            })
            .into_widget(),
        );
    }
    if e.dismissible {
        let id = e.id;
        r.push(gap_w(6.0).into_widget());
        r.push(
            GestureDetector::new(icon(IconKind::Close).size(15.0).color(c.muted_foreground))
                .cursor(pebbles_render::Cursor::Pointer)
                .on_tap(move || dismiss_toast(id))
                .into_widget(),
        );
    }

    let base = crate::style::style()
        .width(WIDTH)
        .background(c.popover)
        .border(Border::new(c.border, 1.0))
        .radius_all(theme().radius + 2.0)
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 60), Offset::new(0.0, 8.0), 24.0, -6.0))
        .padding_xy(14.0, 12.0);
    let card = crate::style::styled(
        row(r).cross_axis_alignment(CrossAxisAlignment::Center),
        base.merge(e.style.clone().unwrap_or_default()),
    );
    // C7: announce each toast as an Alert (label = title) to assistive tech.
    crate::widgets::semantics(pebbles_render::SemanticsRole::Alert, e.title.clone(), card).into_widget()
}

/// Props for one animated toast card (a component so it owns its enter/leave tween).
#[derive(Clone)]
struct CardProps {
    entry: ToastEntry,
}

/// One toast card with C1 motion + hover-pause. Enter: mount at `t=0`, flip to `t=1`
/// on the next frame → fade + slide-up 8px. Leave: the entry's `leaving` flag (set by
/// [`dismiss_toast`]) drives `t→0`; the stack removes the entry after [`MOTION_SECS`].
/// Hover pauses the auto-dismiss (banks the remaining time) and re-arms on exit.
fn render_toast_card(p: &CardProps) -> AnyWidget {
    let e = &p.entry;
    // Trigger the enter transition one frame after mount (the flip-signal recipe).
    // `create_timeout` is called unconditionally (hooks are position-based); the
    // callback self-guards so it flips exactly once and never re-arms into a loop.
    let shown = create_signal(false);
    let s = shown;
    animation::create_timeout(0.0, move || {
        if !s.peek() {
            s.set(true);
        }
    });
    let target = if e.leaving || !shown.get() { 0.0 } else { 1.0 };
    let t = animation::animated(target, MOTION_SECS);

    let visual = toast_card_inner(e);
    let slid = Transform::translate(0.0, (1.0 - t) * 8.0, visual);
    let faded = Opacity::new(t as f32, slid);

    // Hover-pause: only meaningful for auto-dismissing, not-yet-leaving toasts.
    let id = e.id;
    let duration = e.duration;
    let leaving = e.leaving;
    let (rem_enter, at_enter) = (e.remaining.clone(), e.armed_at.clone());
    let (rem_exit, at_exit) = (e.remaining.clone(), e.armed_at.clone());
    GestureDetector::new(faded)
        .on_hover_enter(move || {
            if duration > 0.0 && !leaving {
                animation::clear_timeout(timer_key(id));
                let elapsed = (animation::now() - at_enter.get()).max(0.0);
                rem_enter.set((rem_enter.get() - elapsed).max(0.0));
            }
        })
        .on_hover_exit(move || {
            if duration > 0.0 && !leaving {
                at_exit.set(animation::now());
                let rem = rem_exit.get();
                animation::set_timeout(timer_key(id), rem, move || dismiss_toast(id));
            }
        })
        .into_widget()
}

/// Overlay children for the current window's toast stack (bottom-right, newest at the
/// bottom, capped at [`MAX_VISIBLE`]). Rendered by [`OverlayHost`].
pub(crate) fn overlay_children() -> Vec<AnyWidget> {
    let toasts = stack_signal().get();
    if toasts.is_empty() {
        return Vec::new();
    }
    // Show the most recent MAX_VISIBLE, newest at the bottom of the stack.
    let visible: Vec<&ToastEntry> = toasts.iter().rev().take(MAX_VISIBLE).rev().collect();
    let mut cards: Vec<AnyWidget> = Vec::new();
    for (i, e) in visible.iter().enumerate() {
        if i > 0 {
            cards.push(gap_h(10.0).into_widget());
        }
        cards.push(component_props(render_toast_card, CardProps { entry: (*e).clone() }).into_widget());
    }
    let stack = column(cards).cross_axis_alignment(CrossAxisAlignment::End).main_axis_size(MainAxisSize::Min);
    // Anchor bottom-right; window_size gives the current window's logical bounds.
    let (ww, _wh) = window_size();
    let panel = if ww > 0.0 {
        Positioned::new(stack).right(16.0).bottom(16.0)
    } else {
        Positioned::new(stack).left(16.0).top(16.0)
    };
    vec![panel.into_widget()]
}
