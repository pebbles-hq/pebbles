//! Searchable dropdowns built on the popover + a filtered command list — shadcn's
//! **Combobox** (single choice) and a **MultiSelect** (many). Both pop a search
//! box over the option list; typing filters it, with a "no results" empty state.
//! [`Combobox`] closes on pick; [`MultiSelect`] stays open and toggles.

use std::rc::Rc;

use pebbles_foundation::{Alignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Border, Cursor, IconKind, PointerEvent};

use super::list_nav::list_nav;
use super::menu::{ActionRowProps, action_row};
use super::popover::{anchor_below, popover_surface};
use super::text_field::text_field;
use crate::components::icon;
use crate::overlay::{hide_overlay, show_overlay_guarded};
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, SingleChildScrollView, column, gap_h, row, spacer, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, action_event, children, component_props, create_signal};

type ChangedCb = Rc<dyn Fn(usize, &str)>;
type SelectionCb = Rc<dyn Fn(&[usize])>;

const TRIGGER_H: f64 = 38.0;

/// A trigger that looks like the [`Select`](super::Select) trigger: bordered box,
/// value-or-placeholder text, trailing chevron.
fn trigger_box(label: String, filled: bool, width: f64, user: Option<crate::style::Style>) -> AnyWidget {
    let c = theme().colors;
    let fg = if filled { c.foreground } else { c.muted_foreground };
    let deco = crate::style::style()
        .background(c.background)
        .border(Border::new(c.input, 1.0))
        .radius_all(theme().radius)
        .merge(user.unwrap_or_default())
        .decoration()
        .unwrap_or_default();
    Container::new()
        .width(width)
        .height(TRIGGER_H)
        .decoration(deco)
        .padding(EdgeInsets::symmetric(12.0, 0.0))
        .alignment(Alignment::CENTER_LEFT)
        .child(row(children![
            text(label).size(14.0).color(fg),
            spacer(),
            icon(IconKind::ChevronDown).size(16.0).color(c.muted_foreground),
        ]))
        .into_widget()
}

/// The shared searchable menu: a search field over a filtered, checkable option
/// list. `is_selected(i)` marks the check; `on_pick(i)` handles a choice.
fn search_menu(
    options: Rc<Vec<String>>,
    width: f64,
    search_ph: String,
    empty: String,
    is_selected: Rc<dyn Fn(usize) -> bool>,
    on_pick: Rc<dyn Fn(usize)>,
) -> AnyWidget {
    component_props(render_search_menu, MenuProps { options, width, search_ph, empty, is_selected, on_pick })
        .into_widget()
}

struct MenuProps {
    options: Rc<Vec<String>>,
    width: f64,
    search_ph: String,
    empty: String,
    is_selected: Rc<dyn Fn(usize) -> bool>,
    on_pick: Rc<dyn Fn(usize)>,
}

fn render_search_menu(p: &MenuProps) -> AnyWidget {
    let c = theme().colors;
    let width = p.width;
    let inner = width - 8.0;
    let query = create_signal(String::new());
    let q = query.get().to_lowercase();
    let nav = list_nav();

    let matches: Vec<(usize, String)> = p
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| q.is_empty() || o.to_lowercase().contains(q.as_str()))
        .map(|(i, o)| (i, o.clone()))
        .collect();

    // Keep the keyboard cursor in range after filtering, then map a picked
    // *visible-row* index back to the original option index.
    nav.clamp(matches.len());
    let active = nav.active();
    let pick_by_row: Rc<dyn Fn(usize)> = {
        let rows: Vec<usize> = matches.iter().map(|(i, _)| *i).collect();
        let on_pick = p.on_pick.clone();
        Rc::new(move |row| {
            if let Some(&orig) = rows.get(row) {
                on_pick(orig);
            }
        })
    };
    let nav_handler = {
        let pick_by_row = pick_by_row.clone();
        nav.handler(matches.len(), move |row| pick_by_row(row), hide_overlay)
    };

    let search = text_field()
        .leading(IconKind::Search)
        .placeholder(p.search_ph.clone())
        .width(inner)
        .autofocus()
        .on_nav(nav_handler)
        .on_changed(move |s| query.set(s.to_string()));

    let list: AnyWidget = if matches.is_empty() {
        Container::new()
            .width(inner)
            .padding(EdgeInsets::symmetric(8.0, 16.0))
            .alignment(Alignment::CENTER)
            .child(text(p.empty.clone()).size(13.0).color(c.muted_foreground))
            .into_widget()
    } else {
        let scrolls = matches.len() > 6;
        let items: Vec<AnyWidget> = matches
            .into_iter()
            .enumerate()
            .map(|(row, (i, label))| {
                let sel = (p.is_selected)(i);
                let pick = p.on_pick.clone();
                action_row(ActionRowProps {
                    label,
                    icon: None,
                    shortcut: None,
                    leading_check: Some(sel),
                    reserve_gutter: true,
                    destructive: false,
                    disabled: false,
                    highlighted: active == Some(row),
                    width: inner,
                    on_select: Rc::new(move || pick(i)),
                })
            })
            .collect();
        let col = column(items).main_axis_size(MainAxisSize::Min);
        if scrolls {
            Container::new()
                .height(240.0)
                .child(SingleChildScrollView::vertical(col).scrollbar_thickness(6.0))
                .into_widget()
        } else {
            col.into_widget()
        }
    };

    popover_surface(
        width,
        4.0,
        column(children![search, gap_h(6.0), list]).main_axis_size(MainAxisSize::Min).into_widget(),
    )
}

// ---------------------------------------------------------------------------
// Combobox — searchable single-select.
// ---------------------------------------------------------------------------

/// A searchable single-select. Build with [`combobox`].
#[derive(Clone, Default)]
pub struct Combobox {
    options: Vec<String>,
    initial: Option<usize>,
    placeholder: String,
    search_placeholder: String,
    empty: String,
    width: f64,
    on_changed: Option<ChangedCb>,
    style: Option<crate::style::Style>,
}

/// Create a [`Combobox`] over `options`.
pub fn combobox<I, S>(options: I) -> Combobox
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    Combobox {
        options: options.into_iter().map(Into::into).collect(),
        placeholder: "Select…".to_string(),
        search_placeholder: "Search…".to_string(),
        empty: "No results.".to_string(),
        width: 240.0,
        ..Default::default()
    }
}

impl Combobox {
    pub fn value(mut self, index: usize) -> Self {
        self.initial = Some(index);
        self
    }
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn search_placeholder(mut self, s: impl Into<String>) -> Self {
        self.search_placeholder = s.into();
        self
    }
    pub fn empty(mut self, s: impl Into<String>) -> Self {
        self.empty = s.into();
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
    /// Merge a [`Style`](crate::Style) onto the trigger box.
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
}

struct ComboProps {
    options: Vec<String>,
    initial: Option<usize>,
    placeholder: String,
    search_placeholder: String,
    empty: String,
    width: f64,
    on_changed: Option<ChangedCb>,
    style: Option<crate::style::Style>,
}

impl IntoWidget for Combobox {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_combobox,
            ComboProps {
                options: self.options,
                initial: self.initial,
                placeholder: self.placeholder,
                search_placeholder: self.search_placeholder,
                empty: self.empty,
                width: self.width,
                on_changed: self.on_changed,
                style: self.style,
            },
        )
        .into_widget()
    }
}

fn render_combobox(p: &ComboProps) -> AnyWidget {
    let width = p.width;
    let selected = create_signal(p.initial);
    let options = Rc::new(p.options.clone());
    let on_changed = p.on_changed.clone();
    let search_ph = p.search_placeholder.clone();
    let empty = p.empty.clone();

    let label = match selected.get() {
        Some(i) => options.get(i).cloned().unwrap_or_else(|| p.placeholder.clone()),
        None => p.placeholder.clone(),
    };
    let trigger = trigger_box(label, selected.get().is_some(), width, p.style.clone());

    let menu_options = options.clone();
    GestureDetector::new(trigger)
        .cursor(Cursor::Pointer)
        .on_tap(action_event(move |e: PointerEvent| {
            let trigger_left = e.global.x - e.position.x;
            let trigger_top = e.global.y - e.position.y;
            let est = (menu_options.len().min(6) as f64) * 34.0 + 60.0;
            let (left, top) = anchor_below(trigger_left, trigger_top, TRIGGER_H, width, est);

            let opts = menu_options.clone();
            let is_selected: Rc<dyn Fn(usize) -> bool> = Rc::new(move |i| selected.peek() == Some(i));
            let oc = on_changed.clone();
            let picked_opts = opts.clone();
            let on_pick: Rc<dyn Fn(usize)> = Rc::new(move |i| {
                selected.set(Some(i));
                if let Some(cb) = &oc {
                    cb(i, picked_opts.get(i).map(String::as_str).unwrap_or(""));
                }
                hide_overlay();
            });
            show_overlay_guarded(
                search_menu(opts, width, search_ph.clone(), empty.clone(), is_selected, on_pick),
                left,
                top,
                width,
                est,
                move || selected.alive(),
            );
        }))
        .into_widget()
}

// ---------------------------------------------------------------------------
// MultiSelect — searchable multi-select (stays open, toggles).
// ---------------------------------------------------------------------------

/// A searchable multi-select. Build with [`multi_select`].
#[derive(Clone, Default)]
pub struct MultiSelect {
    options: Vec<String>,
    initial: Vec<usize>,
    placeholder: String,
    search_placeholder: String,
    empty: String,
    width: f64,
    on_changed: Option<SelectionCb>,
    style: Option<crate::style::Style>,
}

/// Create a [`MultiSelect`] over `options`.
pub fn multi_select<I, S>(options: I) -> MultiSelect
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    MultiSelect {
        options: options.into_iter().map(Into::into).collect(),
        placeholder: "Select…".to_string(),
        search_placeholder: "Search…".to_string(),
        empty: "No results.".to_string(),
        width: 240.0,
        ..Default::default()
    }
}

impl MultiSelect {
    pub fn values(mut self, indices: impl IntoIterator<Item = usize>) -> Self {
        self.initial = indices.into_iter().collect();
        self
    }
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    pub fn search_placeholder(mut self, s: impl Into<String>) -> Self {
        self.search_placeholder = s.into();
        self
    }
    pub fn empty(mut self, s: impl Into<String>) -> Self {
        self.empty = s.into();
        self
    }
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
        self
    }
    pub fn on_changed(mut self, f: impl Fn(&[usize]) + 'static) -> Self {
        self.on_changed = Some(Rc::new(f));
        self
    }
    /// Merge a [`Style`](crate::Style) onto the trigger box.
    pub fn style(mut self, s: crate::style::Style) -> Self {
        self.style = Some(s);
        self
    }
}

struct MultiProps {
    options: Vec<String>,
    initial: Vec<usize>,
    placeholder: String,
    search_placeholder: String,
    empty: String,
    width: f64,
    on_changed: Option<SelectionCb>,
    style: Option<crate::style::Style>,
}

impl IntoWidget for MultiSelect {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_multi,
            MultiProps {
                options: self.options,
                initial: self.initial,
                placeholder: self.placeholder,
                search_placeholder: self.search_placeholder,
                empty: self.empty,
                width: self.width,
                on_changed: self.on_changed,
                style: self.style,
            },
        )
        .into_widget()
    }
}

fn render_multi(p: &MultiProps) -> AnyWidget {
    let width = p.width;
    let selected: Signal<Vec<usize>> = create_signal(p.initial.clone());
    let options = Rc::new(p.options.clone());
    let on_changed = p.on_changed.clone();
    let search_ph = p.search_placeholder.clone();
    let empty = p.empty.clone();

    // Trigger label: placeholder / joined labels / "N selected".
    let sel = selected.get();
    let label = if sel.is_empty() {
        p.placeholder.clone()
    } else if sel.len() <= 2 {
        sel.iter().filter_map(|&i| options.get(i)).cloned().collect::<Vec<_>>().join(", ")
    } else {
        format!("{} selected", sel.len())
    };
    let trigger = trigger_box(label, !sel.is_empty(), width, p.style.clone());

    let menu_options = options.clone();
    GestureDetector::new(trigger)
        .cursor(Cursor::Pointer)
        .on_tap(action_event(move |e: PointerEvent| {
            let trigger_left = e.global.x - e.position.x;
            let trigger_top = e.global.y - e.position.y;
            let est = (menu_options.len().min(6) as f64) * 34.0 + 60.0;
            let (left, top) = anchor_below(trigger_left, trigger_top, TRIGGER_H, width, est);

            let opts = menu_options.clone();
            let is_selected: Rc<dyn Fn(usize) -> bool> = Rc::new(move |i| selected.get().contains(&i));
            let oc = on_changed.clone();
            let on_pick: Rc<dyn Fn(usize)> = Rc::new(move |i| {
                selected.update(|v| {
                    if let Some(pos) = v.iter().position(|&x| x == i) {
                        v.remove(pos);
                    } else {
                        v.push(i);
                    }
                });
                if let Some(cb) = &oc {
                    cb(&selected.peek());
                }
                // Stays open — no hide_overlay.
            });
            show_overlay_guarded(
                search_menu(opts, width, search_ph.clone(), empty.clone(), is_selected, on_pick),
                left,
                top,
                width,
                est,
                move || selected.alive(),
            );
        }))
        .into_widget()
}
