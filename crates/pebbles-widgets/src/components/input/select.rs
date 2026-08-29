//! [`Select`] — a dropdown picker. Clicking the trigger pops a menu into the
//! global overlay layer (so it escapes clipping and scrolling); picking an option
//! sets the value and dismisses. `on_changed` reports the chosen index + label.

use std::rc::Rc;

use pebbles_foundation::{Alignment, Color, EdgeInsets, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, IconKind, PointerEvent};

use crate::components::icon;
use crate::overlay::{hide_overlay, show_overlay, window_size};
use crate::theme::{mix, theme};
use crate::widgets::{
    Container, GestureDetector, SingleChildScrollView, SizedBox, column, row, spacer, text,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, action, action_event, animated, children, component_props, create_signal};

/// A dropdown select. Build with [`select`].
pub struct Select {
    options: Vec<String>,
    initial: Option<usize>,
    placeholder: String,
    width: f64,
    on_changed: Option<Rc<dyn Fn(usize, &str)>>,
}

/// Create a [`Select`] over `options`.
pub fn select<I, S>(options: I) -> Select
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Select {
        options: options.into_iter().map(Into::into).collect(),
        initial: None,
        placeholder: "Select…".to_string(),
        width: 220.0,
        on_changed: None,
    }
}

impl Select {
    pub fn value(mut self, index: usize) -> Self {
        self.initial = Some(index);
        self
    }
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
        self
    }
    pub fn on_changed(mut self, f: impl Fn(usize, &str) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
}

struct Props {
    options: Vec<String>,
    initial: Option<usize>,
    placeholder: String,
    width: f64,
    on_changed: Option<Rc<dyn Fn(usize, &str)>>,
}

impl IntoWidget for Select {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_select,
            Props {
                options: self.options,
                initial: self.initial,
                placeholder: self.placeholder,
                width: self.width,
                on_changed: self.on_changed,
            },
        )
        .into_widget()
    }
}

/// Build the dropdown menu widget shown in the overlay. When `max_height` is set
/// (long option lists) the items scroll inside the popover.
fn build_menu(
    options: &[String],
    width: f64,
    selected: Signal<Option<usize>>,
    on_changed: Option<Rc<dyn Fn(usize, &str)>>,
    max_height: Option<f64>,
) -> AnyWidget {
    let c = theme().colors;
    let mut items: Vec<AnyWidget> = Vec::new();
    for (i, opt) in options.iter().enumerate() {
        let label = opt.clone();
        let oc = on_changed.clone();
        let is_sel = selected.peek() == Some(i);
        let pick: Rc<dyn Fn()> = Rc::new(move || {
            selected.set(Some(i));
            if let Some(cb) = &oc {
                cb(i, &label);
            }
            hide_overlay();
        });
        items.push(menu_item(opt.clone(), is_sel, width - 8.0, pick));
    }

    let list = column(items);
    let body: AnyWidget = match max_height {
        Some(h) => Container::new()
            .height(h)
            .child(SingleChildScrollView::vertical(list).scrollbar_thickness(6.0))
            .into_widget(),
        None => list.into_widget(),
    };

    Container::new()
        .width(width)
        .decoration(
            BoxDecoration::new()
                .color(c.popover)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(theme().radius))
                .shadow(BoxShadow::new(
                    Color::from_rgba8(0, 0, 0, 45),
                    Offset::new(0.0, 8.0),
                    22.0,
                    -4.0,
                )),
        )
        .padding(EdgeInsets::all(4.0))
        .child(body)
        .into_widget()
}

// ---------------------------------------------------------------------------
// Menu item — a hover-highlighting row.
// ---------------------------------------------------------------------------

struct MenuItemProps {
    label: String,
    selected: bool,
    width: f64,
    pick: Rc<dyn Fn()>,
}

fn menu_item(label: String, selected: bool, width: f64, pick: Rc<dyn Fn()>) -> AnyWidget {
    component_props(render_menu_item, MenuItemProps { label, selected, width, pick }).into_widget()
}

fn render_menu_item(p: &MenuItemProps) -> GestureDetector {
    let c = theme().colors;
    let hovered = create_signal(false);
    let t = animated(if hovered.get() { 1.0 } else { 0.0 }, 0.1);
    let bg = mix(c.popover, c.accent, t as f32);
    let fg = mix(c.popover_foreground, c.accent_foreground, t as f32);

    // shadcn layout: a fixed left indicator area (the check), then the label.
    let check: AnyWidget = if p.selected {
        icon(IconKind::Check).size(15.0).color(fg).into_widget()
    } else {
        SizedBox::spacer(0.0, 0.0).into_widget()
    };
    let body = Container::new()
        .width(p.width)
        .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(4.0)))
        .padding(EdgeInsets::symmetric(8.0, 7.0))
        .child(row(children![
            Container::new().width(22.0).alignment(Alignment::CENTER_LEFT).child(check),
            text(p.label.clone()).size(14.0).color(fg),
        ]));

    let pick = p.pick.clone();
    GestureDetector::new(body)
        .cursor(Cursor::Pointer)
        .on_hover_enter(action(move || hovered.set(true)))
        .on_hover_exit(action(move || hovered.set(false)))
        .on_tap(action(move || pick()))
}

fn render_select(p: &Props) -> GestureDetector {
    let c = theme().colors;
    let selected = create_signal(p.initial);
    let width = p.width;
    let options = p.options.clone();
    let on_changed = p.on_changed.clone();

    let label = match selected.get() {
        Some(i) => options.get(i).cloned().unwrap_or_default(),
        None => p.placeholder.clone(),
    };
    let label_color = if selected.get().is_some() { c.foreground } else { c.muted_foreground };

    let trigger = Container::new()
        .width(width)
        .height(38.0)
        .decoration(
            BoxDecoration::new()
                .color(c.background)
                .border(Border::new(c.input, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .padding(EdgeInsets::symmetric(12.0, 0.0))
        .alignment(Alignment::CENTER_LEFT)
        .child(row(children![
            text(label).size(14.0).color(label_color),
            spacer(),
            icon(IconKind::ChevronDown).size(16.0).color(c.muted_foreground),
        ]));

    GestureDetector::new(trigger).cursor(Cursor::Pointer).on_tap(action_event(
        move |e: PointerEvent| {
            // The trigger's window-space rect = click global − click local.
            let trigger_left = e.global.x - e.position.x;
            let trigger_top = e.global.y - e.position.y;
            let trigger_h = 38.0;
            let (ww, wh) = window_size();

            // Menu height: rows + padding, capped (then it scrolls internally).
            let natural = options.len() as f64 * 34.0 + 8.0;
            let menu_h = natural.min(300.0);
            let scrolls = natural > menu_h + 0.5;

            // Flip up when there isn't room below and there is room above.
            let below = trigger_top + trigger_h + 6.0;
            let above = trigger_top - menu_h - 6.0;
            let top = if wh > 0.0 && below + menu_h > wh - 8.0 && above >= 8.0 {
                above
            } else {
                below
            };
            // Shift horizontally to stay on-screen.
            let left = if ww > 0.0 {
                trigger_left.min(ww - width - 8.0).max(8.0)
            } else {
                trigger_left
            };

            let menu = build_menu(
                &options,
                width,
                selected,
                on_changed.clone(),
                scrolls.then_some(menu_h),
            );
            show_overlay(menu, left, top);
        },
    ))
}
