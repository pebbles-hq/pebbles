//! [`sheet`] — an edge-anchored modal panel (shadcn Sheet / Drawer): a full-height
//! (left/right) or full-width (top/bottom) surface over a dimmed scrim. An app service
//! like [`dialog`](crate::dialog); per-window, one at a time. `Side::Bottom` is the
//! "drawer" pattern.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets};
use pebbles_render::{Border, BorderRadius, BoxDecoration};

use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Positioned, SizedBox, column, text};
use crate::overlay::window_size;
use pebbles_core::context::action;
use pebbles_core::reactive::current_window;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, create_root_signal};

/// Which edge a [`Sheet`] anchors to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

/// Identifies an open sheet.
pub type SheetId = u64;

#[derive(Clone)]
struct SheetEntry {
    content: AnyWidget,
    id: SheetId,
    side: Side,
    size: f64,
    title: String,
    background: Option<Color>,
    dismissible: bool,
    on_close: Option<Rc<dyn Fn()>>,
}

thread_local! {
    static NEXT_ID: Cell<SheetId> = const { Cell::new(1) };
    static SHEET: RefCell<HashMap<u32, Signal<Option<SheetEntry>>>> = RefCell::new(HashMap::new());
}

/// Create the main window's sheet signal (call once at startup, like dialog/overlay).
pub fn init() {
    let _ = sheet_signal();
}

fn sheet_signal() -> Signal<Option<SheetEntry>> {
    let window = current_window();
    SHEET.with(|cell| *cell.borrow_mut().entry(window).or_insert_with(|| create_root_signal(None)))
}

/// Whether a sheet is open in the current window.
pub fn is_open() -> bool {
    sheet_signal().peek().is_some()
}

/// Close the sheet matching `id` (or any open one when `id == 0`), firing `on_close`.
pub fn close_sheet(id: SheetId) {
    if let Some(e) = sheet_signal().peek()
        && (id == 0 || e.id == id)
    {
        sheet_signal().set(None);
        if let Some(f) = &e.on_close {
            f();
        }
    }
}

/// Close the open sheet if it's dismissible (Escape / scrim click).
pub fn dismiss_top() {
    if let Some(e) = sheet_signal().peek()
        && e.dismissible
    {
        close_sheet(e.id);
    }
}

/// A sheet to open. Build it, then [`open`](Sheet::open).
pub struct Sheet {
    content: AnyWidget,
    side: Side,
    size: f64,
    title: String,
    background: Option<Color>,
    dismissible: bool,
    on_close: Option<Rc<dyn Fn()>>,
}

/// Create a [`Sheet`] wrapping `content` (a right-side, 360px panel by default).
pub fn sheet(content: impl IntoWidget) -> Sheet {
    Sheet {
        content: content.into_widget(),
        side: Side::Right,
        size: 360.0,
        title: String::new(),
        background: None,
        dismissible: true,
        on_close: None,
    }
}

impl Sheet {
    /// Which edge the sheet slides from (default `Right`). `Bottom` = a drawer.
    pub fn side(mut self, side: Side) -> Self {
        self.side = side;
        self
    }
    /// The panel's width (left/right) or height (top/bottom), in logical px.
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }
    /// A header title rendered above the content.
    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }
    pub fn on_close(mut self, f: impl Fn() + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }
    /// Open the sheet (replacing any open one). Returns its [`SheetId`].
    pub fn open(self) -> SheetId {
        let id = NEXT_ID.with(|n| {
            let v = n.get();
            n.set(v + 1);
            v
        });
        sheet_signal().set(Some(SheetEntry {
            content: self.content,
            id,
            side: self.side,
            size: self.size,
            title: self.title,
            background: self.background,
            dismissible: self.dismissible,
            on_close: self.on_close,
        }));
        id
    }
}

/// Overlay children for the open sheet (scrim + edge-anchored panel), or empty.
/// Rendered by [`OverlayHost`](crate::overlay::OverlayHost).
pub(crate) fn overlay_children() -> Vec<AnyWidget> {
    let Some(e) = sheet_signal().get() else {
        return Vec::new();
    };
    let c = theme().colors;

    let scrim = Positioned::fill(
        GestureDetector::new(Container::new().color(Color::new([0.0, 0.0, 0.0, 0.4])))
            .on_tap(action(dismiss_top)),
    )
    .into_widget();

    // The panel content: optional title header, then the caller's content.
    let mut kids: Vec<AnyWidget> = Vec::new();
    if !e.title.is_empty() {
        kids.push(text(e.title.clone()).size(17.0).weight(600.0).color(c.foreground).into_widget());
        kids.push(SizedBox::spacer(0.0, 14.0).into_widget());
    }
    kids.push(e.content);
    let body = column(kids).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min();

    let horizontal = matches!(e.side, Side::Left | Side::Right);
    let (ww, wh) = window_size();
    let mut surface = Container::new()
        .decoration(
            BoxDecoration::new()
                .color(e.background.unwrap_or(c.background))
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(0.0)),
        )
        .padding(EdgeInsets::all(22.0))
        .child(body);
    if horizontal {
        surface = surface.width(e.size);
        if wh > 0.0 {
            surface = surface.height(wh);
        }
    } else {
        surface = surface.height(e.size);
        if ww > 0.0 {
            surface = surface.width(ww);
        }
    }

    let mut panel = Positioned::new(surface);
    panel = match e.side {
        Side::Left => panel.left(0.0).top(0.0),
        Side::Right => panel.right(0.0).top(0.0),
        Side::Top => panel.left(0.0).top(0.0),
        Side::Bottom => panel.left(0.0).bottom(0.0),
    };

    vec![scrim, panel.into_widget()]
}
