//! [`Dialog`] — a modal rendered in the app's overlay layer: a dimmed full-window
//! scrim over the app, with a centered surface holding your content. This is
//! shadcn's Dialog. It runs on the **single** app [`Ui`], so it composes safely with
//! the reactive runtime (unlike a separate OS window, which would need a per-`Ui`
//! reactive runtime the engine doesn't have yet).
//!
//! Imperative, Flutter-`showDialog` style: a handler calls
//! `dialog(content).title("…").width(440.0).on_close(..).open()`, returning a
//! [`DialogId`]. Close it from a button via [`close_dialog`], or — when dismissible
//! (the default) — with the Escape key or an outside click.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use pebbles_foundation::{Color, MainAxisAlignment, MainAxisSize, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow};

use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Positioned, center, gap_h, gap_w};
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, create_root_signal};

/// Identifies an open dialog.
pub type DialogId = u64;

/// The live modal: its content, id, panel width, and behavior.
#[derive(Clone)]
struct DialogEntry {
    content: AnyWidget,
    id: DialogId,
    width: f64,
    background: Option<Color>,
    dismissible: bool,
    on_close: Option<Rc<dyn Fn()>>,
}

thread_local! {
    static NEXT_ID: Cell<DialogId> = const { Cell::new(1) };
    /// One modal signal per window id (main = 0), created lazily on first access. Ids
    /// stay globally unique (a single counter), so `close_dialog(id)` is unambiguous.
    static MODAL: RefCell<HashMap<u32, Signal<Option<DialogEntry>>>> =
        RefCell::new(HashMap::new());
}

/// Create the main window's modal signal. Call once at startup (before the tree runs),
/// like the overlay/focus signals. Secondary windows create theirs lazily.
pub fn init() {
    let _ = modal_signal();
}

/// Forget a closed window's modal state (clear + drop its signal — window ids are
/// never reused).
pub(crate) fn drop_window(window: u32) {
    if let Some(sig) = MODAL.with(|m| m.borrow_mut().remove(&window)) {
        sig.set(None);
    }
}

/// The current window's modal signal (reactive), created on first access.
fn modal_signal() -> Signal<Option<DialogEntry>> {
    let window = current_window();
    MODAL.with(|cell| {
        *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(None))
    })
}

/// Whether a dialog is currently open in the current window.
pub fn is_open() -> bool {
    modal_signal().peek().is_some()
}

/// Close the open dialog (firing its `on_close`) when it matches `id`. Pass `0` to
/// close whatever is open.
pub fn close_dialog(id: DialogId) {
    if let Some(e) = modal_signal().peek()
        && (id == 0 || e.id == id)
    {
        modal_signal().set(None);
        if let Some(f) = &e.on_close {
            f();
        }
    }
}

/// Close the open dialog if it's dismissible (the Escape / outside-click path). The
/// shell calls this on Escape; the scrim calls it on an outside click.
pub fn dismiss_top() {
    if let Some(e) = modal_signal().peek()
        && e.dismissible
    {
        close_dialog(e.id);
    }
}

/// A modal dialog. Build it, then [`open`](Dialog::open) it.
pub struct Dialog {
    content: AnyWidget,
    title: String,
    width: f64,
    background: Option<Color>,
    dismissible: bool,
    on_close: Option<Rc<dyn Fn()>>,
}

/// Create a [`Dialog`] wrapping `content` (a centered ~480-wide surface by default).
pub fn dialog(content: impl IntoWidget) -> Dialog {
    Dialog {
        content: content.into_widget(),
        title: String::new(),
        width: 480.0,
        background: None,
        dismissible: true,
        on_close: None,
    }
}

impl Dialog {
    /// An accessible title (also handy as documentation; the surface itself draws no
    /// chrome — compose a header in your content).
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    /// The panel width (height follows the content).
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }
    /// The surface background (defaults to the theme popover color).
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    /// Whether Escape / an outside click closes it (default `true`).
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
    /// Called when the dialog closes (button, Escape, or outside click).
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
    /// Show the dialog, replacing any that's open. Returns its [`DialogId`].
    pub fn open(self) -> DialogId {
        let id = NEXT_ID.with(|n| {
            let v = n.get();
            n.set(v + 1);
            v
        });
        modal_signal().set(Some(DialogEntry {
            content: self.content,
            id,
            width: self.width,
            background: self.background,
            dismissible: self.dismissible,
            on_close: self.on_close,
        }));
        id
    }
}

// ---------------------------------------------------------------------------
// Alert Dialog — a preset over Dialog (shadcn AlertDialog)
// ---------------------------------------------------------------------------

/// A confirmation modal: title + description + Cancel/Confirm buttons. Unlike a plain
/// [`Dialog`] it is **non-dismissible by default** (an explicit choice is required).
pub struct AlertDialog {
    title: String,
    description: String,
    confirm: String,
    cancel: String,
    destructive: bool,
    dismissible: bool,
    on_confirm: Option<Rc<dyn Fn()>>,
    on_cancel: Option<Rc<dyn Fn()>>,
}

/// Create an [`AlertDialog`] with the given title.
pub fn alert_dialog(title: impl Into<String>) -> AlertDialog {
    AlertDialog {
        title: title.into(),
        description: String::new(),
        confirm: "Continue".to_string(),
        cancel: "Cancel".to_string(),
        destructive: false,
        dismissible: false,
        on_confirm: None,
        on_cancel: None,
    }
}

impl AlertDialog {
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = s.into();
        self
    }
    /// The confirm button label (default "Continue").
    pub fn confirm(mut self, s: impl Into<String>) -> Self {
        self.confirm = s.into();
        self
    }
    /// The cancel button label (default "Cancel").
    pub fn cancel(mut self, s: impl Into<String>) -> Self {
        self.cancel = s.into();
        self
    }
    /// Style the confirm button as destructive (red).
    pub fn destructive(mut self, destructive: bool) -> Self {
        self.destructive = destructive;
        self
    }
    /// Allow Escape / outside-click to dismiss (default `false` — shadcn semantics).
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
    pub fn on_confirm(mut self, f: impl Fn() + 'static) -> Self {
        self.on_confirm = Some(Rc::new(f));
        self
    }
    pub fn on_cancel(mut self, f: impl Fn() + 'static) -> Self {
        self.on_cancel = Some(Rc::new(f));
        self
    }
    /// Build the standard panel and open it as a modal. Returns its [`DialogId`].
    pub fn open(self) -> DialogId {
        use crate::components::{Button, ButtonVariant, button};
        use crate::widgets::{column, row, text};
        let c = theme().colors;

        let on_confirm = self.on_confirm.clone();
        let confirm_variant =
            if self.destructive { ButtonVariant::Destructive } else { ButtonVariant::Primary };
        // Both buttons close the top modal (one per window) then fire their callback.
        let confirm_btn = button(self.confirm.clone()).variant(confirm_variant).on_pressed(move || {
            close_dialog(0);
            if let Some(f) = &on_confirm {
                f();
            }
        });
        let on_cancel = self.on_cancel.clone();
        let cancel_btn: Button =
            button(self.cancel.clone()).variant(ButtonVariant::Outline).on_pressed(move || {
                close_dialog(0);
                if let Some(f) = &on_cancel {
                    f();
                }
            });

        let mut kids: Vec<AnyWidget> = vec![
            text(self.title.clone()).size(18.0).weight(600.0).color(c.foreground).into_widget(),
        ];
        if !self.description.is_empty() {
            kids.push(gap_h(8.0).into_widget());
            kids.push(
                text(self.description.clone()).size(14.0).color(c.muted_foreground).into_widget(),
            );
        }
        kids.push(gap_h(22.0).into_widget());
        kids.push(
            row(pebbles_core::children![cancel_btn, gap_w(10.0), confirm_btn])
                .main_axis_alignment(MainAxisAlignment::End)
                .into_widget(),
        );

        let content = column(kids)
            .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min);

        dialog(content).width(440.0).dismissible(self.dismissible).open()
    }
}

/// The overlay children for the open dialog (scrim + centered surface), or empty.
/// Rendered by [`OverlayHost`](crate::overlay::OverlayHost) above the popover layer.
pub(crate) fn overlay_children() -> Vec<AnyWidget> {
    let Some(entry) = modal_signal().get() else {
        return Vec::new();
    };
    let c = theme().colors;

    // Dimming scrim; an outside click dismisses (if dismissible).
    let scrim = Positioned::fill(
        GestureDetector::new(Container::new().color(Color::new([0.0, 0.0, 0.0, 0.45])))
            .on_tap(dismiss_top),
    )
    .into_widget();

    // Centered surface holding the content.
    let surface = Container::new()
        .width(entry.width)
        .decoration(
            BoxDecoration::new()
                .color(entry.background.unwrap_or(c.popover))
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(theme().radius + 4.0))
                .shadow(BoxShadow::new(
                    Color::from_rgba8(0, 0, 0, 90),
                    Offset::new(0.0, 18.0),
                    40.0,
                    -8.0,
                )),
        )
        .child(entry.content);
    let panel = Positioned::fill(center(surface)).into_widget();

    vec![scrim, panel]
}
