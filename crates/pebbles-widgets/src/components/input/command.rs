//! [`Command`] — a searchable command list (shadcn's Command / ⌘K palette). A
//! search field filters across labelled groups; Up/Down/Enter/Escape drive the
//! selection through the shared [`ListNav`](super::list_nav) model, and picking a
//! row runs its action. [`command`] embeds it inline; [`command_palette`] centers
//! the same widget in a dismissible modal (the classic ⌘K overlay).
//!
//! Binding a global hotkey (Ctrl/⌘+K) stays app-side — call
//! `command_palette(groups).open()` from your key handler.

use std::rc::Rc;

use pebbles_foundation::{Alignment, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{BoxDecoration, IconData, IconKind};

use super::list_nav::list_nav;
use super::menu::{ActionRowProps, action_row};
use super::text_field::text_field;
use crate::dialog::{dialog, dismiss_top};
use crate::theme::theme;
use crate::widgets::{Container, SingleChildScrollView, column, gap_h, text};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{component_props, create_signal};

/// One selectable command. Build with [`command_item`].
#[derive(Clone)]
pub struct CommandItem {
    label: String,
    icon: Option<IconData>,
    shortcut: Option<String>,
    on_select: Option<Rc<dyn Fn()>>,
}

/// Create a [`CommandItem`] labelled `label`.
pub fn command_item(label: impl Into<String>) -> CommandItem {
    CommandItem { label: label.into(), icon: None, shortcut: None, on_select: None }
}

impl CommandItem {
    /// A leading icon.
    pub fn icon(mut self, icon: impl Into<IconData>) -> Self {
        self.icon = Some(icon.into());
        self
    }
    /// A trailing keyboard-shortcut hint (e.g. `"⌘N"`).
    pub fn shortcut(mut self, s: impl Into<String>) -> Self {
        self.shortcut = Some(s.into());
        self
    }
    /// The action to run when this item is chosen.
    pub fn on_select(mut self, f: impl Fn() + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }
}

/// A labelled group of [`CommandItem`]s. Build with [`command_group`].
#[derive(Clone)]
pub struct CommandGroup {
    label: String,
    items: Vec<CommandItem>,
}

/// Create a [`CommandGroup`] labelled `label` over `items`.
pub fn command_group<I>(label: impl Into<String>, items: I) -> CommandGroup
where
    I: IntoIterator<Item = CommandItem>,
{
    CommandGroup { label: label.into(), items: items.into_iter().collect() }
}

/// A searchable command list. Build with [`command`] (inline) or
/// [`command_palette`] (centered modal).
#[derive(Clone, Default)]
pub struct Command {
    groups: Vec<CommandGroup>,
    placeholder: String,
    empty: String,
    width: f64,
    modal: bool,
}

/// An inline command list over `groups`.
pub fn command<I>(groups: I) -> Command
where
    I: IntoIterator<Item = CommandGroup>,
{
    Command {
        groups: groups.into_iter().collect(),
        placeholder: "Type a command…".to_string(),
        empty: "No results.".to_string(),
        width: 480.0,
        ..Default::default()
    }
}

/// A command **palette**: the same list centered in a dismissible modal. Call
/// [`open`](Command::open) (usually from a global ⌘K key handler).
pub fn command_palette<I>(groups: I) -> Command
where
    I: IntoIterator<Item = CommandGroup>,
{
    Command { modal: true, ..command(groups) }
}

impl Command {
    /// The search placeholder.
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    /// The empty-state text shown when nothing matches.
    pub fn empty(mut self, s: impl Into<String>) -> Self {
        self.empty = s.into();
        self
    }
    /// The list width (default 480).
    pub fn width(mut self, w: f64) -> Self {
        self.width = w;
        self
    }
    /// Open the palette variant as a dismissible modal. No-op / meaningless for the
    /// inline [`command`] builder (use it in the tree instead).
    pub fn open(self) {
        let width = self.width;
        dialog(self.into_widget()).width(width).open();
    }
}

struct Props {
    groups: Vec<CommandGroup>,
    placeholder: String,
    empty: String,
    width: f64,
    modal: bool,
}

impl IntoWidget for Command {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_command,
            Props {
                groups: self.groups,
                placeholder: self.placeholder,
                empty: self.empty,
                width: self.width,
                modal: self.modal,
            },
        )
        .into_widget()
    }
}

fn render_command(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let width = p.width;
    let inner = width - 16.0;
    let query = create_signal(String::new());
    let q = query.get().to_lowercase();
    let nav = list_nav();

    // A modal palette closes itself on pick / Escape; an inline command just runs.
    let dismiss: Rc<dyn Fn()> = if p.modal { Rc::new(dismiss_top) } else { Rc::new(|| {}) };

    // Flatten the *visible* items (matching the query) in row order, collecting the
    // action for each so ListNav's row index maps straight back to an action.
    let mut actions: Vec<Option<Rc<dyn Fn()>>> = Vec::new();
    for g in &p.groups {
        for it in &g.items {
            if q.is_empty() || it.label.to_lowercase().contains(q.as_str()) {
                actions.push(it.on_select.clone());
            }
        }
    }
    nav.clamp(actions.len());
    let active = nav.active();

    let run = {
        let actions = actions.clone();
        let dismiss = dismiss.clone();
        move |row: usize| {
            if let Some(Some(a)) = actions.get(row) {
                a();
            }
            dismiss();
        }
    };
    let nav_handler = {
        let run = run.clone();
        let dismiss = dismiss.clone();
        nav.handler(actions.len(), move |row| run(row), move || dismiss())
    };

    let search = text_field()
        .leading(IconKind::Search)
        .placeholder(p.placeholder.clone())
        .width(inner)
        .autofocus()
        .on_nav(nav_handler)
        .on_changed(move |s| query.set(s.to_string()));

    // Build the grouped list, assigning each visible item a running row index so
    // its highlight tracks `active`.
    let mut body: Vec<AnyWidget> = Vec::new();
    let mut row = 0usize;
    let mut first_group = true;
    for g in &p.groups {
        let matches: Vec<&CommandItem> = g
            .items
            .iter()
            .filter(|it| q.is_empty() || it.label.to_lowercase().contains(q.as_str()))
            .collect();
        if matches.is_empty() {
            continue; // a group with no matches drops its label too
        }
        if !first_group {
            body.push(gap_h(4.0).into_widget());
        }
        first_group = false;
        body.push(
            Container::new()
                .padding(EdgeInsets::symmetric(8.0, 6.0))
                .alignment(Alignment::CENTER_LEFT)
                .child(text(g.label.clone()).size(11.5).semibold().color(c.muted_foreground))
                .into_widget(),
        );
        for it in matches {
            let this_row = row;
            row += 1;
            let run = run.clone();
            body.push(action_row(ActionRowProps {
                label: it.label.clone(),
                icon: it.icon,
                shortcut: it.shortcut.clone(),
                leading_check: None,
                reserve_gutter: true,
                destructive: false,
                disabled: false,
                highlighted: active == Some(this_row),
                width: inner,
                on_select: Rc::new(move || run(this_row)),
            }));
        }
    }

    let list: AnyWidget = if actions.is_empty() {
        Container::new()
            .width(inner)
            .padding(EdgeInsets::symmetric(8.0, 20.0))
            .alignment(Alignment::CENTER)
            .child(text(p.empty.clone()).size(13.0).color(c.muted_foreground))
            .into_widget()
    } else {
        let col =
            column(body).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min);
        if row > 7 {
            Container::new()
                .height(320.0)
                .child(SingleChildScrollView::vertical(col).scrollbar_thickness(6.0))
                .into_widget()
        } else {
            col.into_widget()
        }
    };

    // Inline command draws its own bordered surface; the palette gets the dialog's.
    let content = column(pebbles_core::children![
        search,
        Container::new().width(inner).padding(EdgeInsets::symmetric(0.0, 8.0)).child(
            Container::new().width(inner).height(1.0).decoration(BoxDecoration::new().color(c.border))
        ),
        list,
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min);

    if p.modal {
        content.into_widget()
    } else {
        Container::new()
            .width(width)
            .decoration(
                crate::style::style()
                    .background(c.popover)
                    .border(pebbles_render::Border::new(c.border, 1.0))
                    .radius_all(theme().radius)
                    .decoration()
                    .unwrap_or_else(BoxDecoration::new),
            )
            .padding(EdgeInsets::all(8.0))
            .child(content)
            .into_widget()
    }
}
