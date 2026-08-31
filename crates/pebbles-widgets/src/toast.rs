//! [`toast`] — transient, non-blocking notifications (Sonner-style), stacked in the
//! bottom-right corner and auto-dismissed after a duration. An app service like
//! [`dialog`](crate::dialog): call `toast("Saved").show()` from anywhere; the
//! [`OverlayHost`](crate::overlay::OverlayHost) renders the current window's stack
//! topmost. Per-window (namespaced like the overlay/dialog signals).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets, MainAxisSize, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, IconKind};

use crate::components::icon;
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Positioned, SizedBox, column, row, spacer, text};
use crate::overlay::window_size;
use pebbles_core::context::action;
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, animation, create_root_signal};

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
}

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

/// Whether any toast is currently showing in the current window.
pub fn any_open() -> bool {
    !stack_signal().peek().is_empty()
}

/// Dismiss a toast by id (also cancels its pending auto-dismiss timer).
pub fn dismiss_toast(id: ToastId) {
    animation::clear_timeout(timer_key(id));
    stack_signal().update(|v| v.retain(|t| t.id != id));
}

/// A namespaced timer key so a toast's auto-dismiss can't collide with other timers.
fn timer_key(id: ToastId) -> u64 {
    0x7040_0000_0000_0000 ^ id
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

fn toast_card(e: &ToastEntry) -> AnyWidget {
    let c = theme().colors;
    let mut left: Vec<AnyWidget> = Vec::new();
    if let Some((ic, tint)) = variant_icon(e.variant) {
        left.push(icon(ic).size(18.0).color(tint).into_widget());
        left.push(SizedBox::spacer(10.0, 0.0).into_widget());
    }
    let mut textcol: Vec<AnyWidget> =
        vec![text(e.title.clone()).size(14.0).weight(600.0).color(c.foreground).into_widget()];
    if let Some(d) = &e.description {
        textcol.push(SizedBox::spacer(0.0, 3.0).into_widget());
        textcol.push(text(d.clone()).size(12.5).color(c.muted_foreground).into_widget());
    }
    left.push(
        column(textcol).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min).into_widget(),
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
            .on_tap(action(move || {
                f();
                dismiss_toast(id);
            }))
            .into_widget(),
        );
    }
    if e.dismissible {
        let id = e.id;
        r.push(SizedBox::spacer(6.0, 0.0).into_widget());
        r.push(
            GestureDetector::new(icon(IconKind::Close).size(15.0).color(c.muted_foreground))
                .cursor(pebbles_render::Cursor::Pointer)
                .on_tap(action(move || dismiss_toast(id)))
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
    crate::style::styled(
        row(r).cross_axis_alignment(CrossAxisAlignment::Center),
        base.merge(e.style.clone().unwrap_or_default()),
    )
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
            cards.push(SizedBox::spacer(0.0, 10.0).into_widget());
        }
        cards.push(toast_card(e));
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
