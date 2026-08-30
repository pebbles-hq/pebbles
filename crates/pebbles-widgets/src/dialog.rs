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
use std::rc::Rc;

use pebbles_foundation::{Color, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow};

use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Positioned, center};
use pebbles_core::context::action;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, create_signal};

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
    static MODAL: RefCell<Option<Signal<Option<DialogEntry>>>> = const { RefCell::new(None) };
}

/// Create the global modal signal. Call once at startup (before the tree runs), like
/// the overlay/focus signals.
pub fn init() {
    let _ = modal_signal();
}

fn modal_signal() -> Signal<Option<DialogEntry>> {
    MODAL.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_signal(None));
        }
        cell.unwrap()
    })
}

/// Whether a dialog is currently open.
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
    /// The panel width (height follows the content). `size(w, _)` is an alias.
    pub fn width(mut self, width: f64) -> Self {
        self.width = width;
        self
    }
    /// Alias for [`width`](Dialog::width); the height is content-driven.
    pub fn size(mut self, width: u32, _height: u32) -> Self {
        self.width = width as f64;
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
            .on_tap(action(dismiss_top)),
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
