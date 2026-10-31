//! [`Chip`] — a compact tag/entity pill with an optional leading icon and an
//! optional **delete** affordance. Flutter's `Chip` (deletable flavor): the
//! standard token widget for filters, tags, selected options and contact chips.
//! Built on the Badge surface with an `IconButton` ✕ that fires `on_deleted`.

use std::rc::Rc;

use pebbles_render::{IconData, IconKind};

use crate::components::icon;
use crate::components::input::icon_button;
use crate::style::{Style, style, styled};
use crate::theme::{mix, theme};
use crate::widgets::{gap_w, row, text};
use pebbles_core::context::Callback;
use pebbles_core::focus::create_focus;
use pebbles_core::reactive::create_signal;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animated, component_props};

/// A small pill with a label, an optional icon, and an optional ✕ delete button.
#[derive(Clone, Default)]
pub struct Chip {
    label: String,
    icon_kind: Option<IconData>,
    deletable: bool,
    disabled: bool,
    selected: bool,
    show_check: bool,
    on_deleted: Option<Callback>,
    on_pressed: Option<Callback>,
    style: Option<Style>,
    size: crate::theme::ControlSize,
}

/// Create a [`Chip`] with a text label (Flutter's deletable/input chip).
pub fn chip(label: impl Into<String>) -> Chip {
    Chip { label: label.into(), ..Default::default() }
}

/// A single-select chip: give it a `selected` state and an `on_pressed` that selects
/// it (the owner enforces one-at-a-time). Flutter's `ChoiceChip`.
pub fn choice_chip(label: impl Into<String>) -> Chip {
    chip(label)
}

/// A multi-select toggle chip that shows a leading check when selected. Flutter's
/// `FilterChip` — wire `on_pressed` to flip its `selected` in your set.
pub fn filter_chip(label: impl Into<String>) -> Chip {
    chip(label).show_check(true)
}

/// A tappable action chip (no selected state). Flutter's `ActionChip` — attach
/// `on_pressed`.
pub fn action_chip(label: impl Into<String>) -> Chip {
    chip(label)
}

impl Chip {
    /// A leading icon.
    pub fn icon(mut self, kind: impl Into<IconData>) -> Self {
        self.icon_kind = Some(kind.into());
        self
    }
    /// Show the ✕ delete affordance (fires `on_deleted` when pressed).
    pub fn deletable(mut self, deletable: bool) -> Self {
        self.deletable = deletable;
        self
    }
    /// Disable the chip (muted presentation, no delete affordance).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// Selected state — a filled (primary) presentation, plus a leading check when
    /// [`show_check`](Self::show_check) is set. Drives `ChoiceChip`/`FilterChip`.
    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
    /// Show a leading check when selected (the `FilterChip` look). Default `false`.
    pub fn show_check(mut self, show: bool) -> Self {
        self.show_check = show;
        self
    }
    /// Called when the chip body is tapped (selection or an action). Independent of
    /// the ✕ delete affordance.
    pub fn on_pressed(mut self, cb: impl Fn() + 'static) -> Self {
        self.on_pressed = Some(pebbles_core::action(cb));
        self
    }
    /// Called when the ✕ is pressed. The chip itself is NOT removed from the tree
    /// — the owner drops it from its list (controlled removal, like every value
    /// in the catalog).
    pub fn on_deleted(mut self, cb: impl Fn() + 'static) -> Self {
        self.on_deleted = Some(pebbles_core::action(cb));
        self
    }
    /// The control size (shared [`ControlSize`](crate::theme::ControlSize) scale): a
    /// chip is exactly as tall as a button of the same size.
    pub fn size(mut self, size: crate::theme::ControlSize) -> Self {
        self.size = size;
        self
    }
    /// Merge a [`Style`] onto the pill's base presentation (user fields win).
    pub fn style(mut self, s: Style) -> Self {
        self.style = Some(s);
        self
    }
}

impl IntoWidget for Chip {
    fn into_widget(self) -> AnyWidget {
        component_props(render_chip, self).into_widget()
    }
}

fn render_chip(p: &Chip) -> AnyWidget {
    let th = theme();
    let c = th.colors;
    // Tap-able chips (choice / filter / action) take hover/press/focus feedback and
    // keyboard activation; display / disabled chips stay static.
    let interactive = p.on_pressed.is_some() && !p.disabled;
    let hovered = create_signal(false);
    let pressed = create_signal(false);
    let node = create_focus();
    let focused = interactive && node.is_focused();

    let (base_bg, fg, border) = if p.disabled {
        (Some(c.muted), c.muted_foreground, false)
    } else if p.selected {
        (Some(c.primary), c.primary_foreground, false)
    } else {
        (Some(c.secondary), c.secondary_foreground, true)
    };
    // Blend the base background toward the foreground on hover/press (interactive only).
    let bg = base_bg.map(|b| {
        if interactive {
            let hv = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12);
            let pr = animated(if pressed.get() { 1.0 } else { 0.0 }, 0.07);
            let cur = mix(b, mix(b, c.foreground, 0.08), hv as f32);
            mix(cur, mix(b, c.foreground, 0.16), pr as f32)
        } else {
            b
        }
    });

    let m = p.size.metrics();
    let mut base = style()
        .radius_all(th.pill())
        .padding(pebbles_foundation::EdgeInsets::symmetric(m.padding_x, 0.0))
        .height(m.height)
        .align(pebbles_foundation::Alignment::CENTER_LEFT);
    let isz = (m.icon - 2.0).max(8.0);
    if let Some(bg) = bg {
        base = base.background(bg);
    }
    // Focus ring replaces the resting border while the chip holds keyboard focus.
    if focused {
        base = base.border(pebbles_render::Border::new(c.ring, 2.0));
    } else if border {
        base = base.border(pebbles_render::Border::new(c.border, th.border_width));
    }

    let mut row_items: Vec<AnyWidget> = Vec::new();
    // A leading check for a selected FilterChip.
    if p.selected && p.show_check && !p.disabled {
        row_items.push(icon(IconKind::Check).size(isz).color(fg).into_widget());
        row_items.push(gap_w(6.0).into_widget());
    }
    if let Some(kind) = p.icon_kind {
        row_items.push(icon(kind).size(isz).color(fg).into_widget());
        row_items.push(gap_w(6.0).into_widget());
    }
    row_items.push(text(p.label.clone()).size(m.font).weight(500.0).color(fg).into_widget());
    let mut body: AnyWidget =
        row(row_items).main_axis_size(pebbles_foundation::MainAxisSize::Min).into_widget();
    if p.deletable
        && !p.disabled
        && let Some(on_deleted) = p.on_deleted.clone()
    {
        // The ✕ is itself a focusable IconButton, so it is Tab-reachable and
        // Enter/Space-activatable for free; a label announces it to screen readers.
        let close = icon_button(IconKind::Close)
            .icon_size((isz - 2.0).max(8.0))
            .padding(0.0)
            .label(format!("Remove {}", p.label))
            .on_pressed(on_deleted);
        let close_styled = styled(close, style().padding(th.pad_all(2.0)).radius_all(th.pill()));
        body = row(vec![body, gap_w(6.0).into_widget(), close_styled.into_widget()])
            .main_axis_size(pebbles_foundation::MainAxisSize::Min)
            .into_widget();
    }
    let pill = styled(body, base.merge(p.style.clone().unwrap_or_default()));

    // Tappable (choice / filter / action chips): wrap in a gesture + keyboard activation.
    match p.on_pressed.clone() {
        Some(on_pressed) if !p.disabled => {
            let activation: Rc<dyn Fn()> = match &p.on_pressed {
                Some(Callback::Plain(f)) => f.clone(),
                _ => Rc::new(|| {}),
            };
            node.register(activation, None, false);
            let gesture = crate::widgets::GestureDetector::new(pill)
                .on_tap(on_pressed)
                .cursor(pebbles_render::Cursor::Pointer)
                .on_hover_enter(move || hovered.set(true))
                .on_hover_exit(move || {
                    hovered.set(false);
                    pressed.set(false);
                })
                .on_pointer_down(move || {
                    pressed.set(true);
                    node.request_focus();
                })
                .on_pointer_up(move || pressed.set(false));
            let mut sem = crate::widgets::semantics(
                crate::widgets::SemanticsRole::Button,
                p.label.clone(),
                gesture.into_widget(),
            );
            // Choice / filter chips carry a selected state; action chips don't.
            if p.show_check || p.selected {
                sem = sem.checked(p.selected);
            }
            sem.into_widget()
        }
        // Disabled-but-tappable chips announce as a disabled Button; plain display
        // chips get a trivial Label so their text is still surfaced.
        _ => {
            if p.on_pressed.is_some() {
                crate::widgets::semantics(
                    crate::widgets::SemanticsRole::Button,
                    p.label.clone(),
                    pill.into_widget(),
                )
                .disabled(true)
                .into_widget()
            } else {
                crate::widgets::semantics(
                    crate::widgets::SemanticsRole::Label,
                    p.label.clone(),
                    pill.into_widget(),
                )
                .into_widget()
            }
        }
    }
}
