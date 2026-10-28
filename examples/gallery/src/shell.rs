//! The gallery shell: a top bar (theme switcher, text direction) over a side list of every
//! page and the selected page.

use std::cell::RefCell;

use pebbles::prelude::*;

use crate::registry::{PAGES, Page};

thread_local! {
    static CURRENT: RefCell<Option<Signal<usize>>> = const { RefCell::new(None) };
}

/// The selected page index (app-scope signal).
pub fn current() -> Signal<usize> {
    CURRENT.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(0)))
}

/// Select the page with `id` (no-op for an unknown id).
pub fn select(id: &str) {
    if let Some(i) = PAGES.iter().position(|p| p.id == id) {
        current().set(i);
    }
}

const DESIGNS: [(&str, DesignLanguage); 3] = [
    ("Default", DesignLanguage::Default),
    ("Material", DesignLanguage::Material),
    ("Tailwind", DesignLanguage::Tailwind),
];
const MODES: [(&str, ThemeMode); 3] =
    [("System", ThemeMode::System), ("Light", ThemeMode::Light), ("Dark", ThemeMode::Dark)];

fn top_bar(page: &'static Page) -> impl IntoWidget {
    let t = theme();
    let design_ix = DESIGNS.iter().position(|(_, d)| *d == t.design).unwrap_or(0);
    let mode = theme_mode();
    let mode_ix = MODES.iter().position(|(_, m)| *m == mode).unwrap_or(0);
    let rtl = text_direction() == TextDirection::Rtl;
    container()
        .padding(EdgeInsets::symmetric(8.0, 16.0))
        .decoration(BoxDecoration::new().color(t.colors.background).border(Border::only(
            BorderSide::NONE,
            BorderSide::NONE,
            BorderSide::new(t.colors.border, 1.0),
            BorderSide::NONE,
        )))
        .child(row(children![
            text(format!("{} / {}", page.group, page.title)).size(14.0).semibold(),
            spacer(),
            toggle_group_labels(DESIGNS.map(|(l, _)| l)).size(ToggleSize::Sm).value(design_ix).on_changed(
                |sel: &[usize]| {
                    if let Some(&i) = sel.first() {
                        set_design(DESIGNS[i].1);
                    }
                }
            ),
            gap_w(12.0),
            toggle_group_labels(MODES.map(|(l, _)| l)).size(ToggleSize::Sm).value(mode_ix).on_changed(
                |sel: &[usize]| {
                    if let Some(&i) = sel.first() {
                        set_theme_mode(MODES[i].1);
                    }
                }
            ),
            gap_w(12.0),
            toggle_group_labels(["LTR", "RTL"]).size(ToggleSize::Sm).value(usize::from(rtl)).on_changed(
                |sel: &[usize]| {
                    let rtl = sel.first() == Some(&1);
                    set_text_direction(if rtl { TextDirection::Rtl } else { TextDirection::Ltr });
                }
            ),
        ]))
}

fn side_list(selected: usize) -> impl IntoWidget {
    let mut side = side_nav().width(220.0);
    let mut group = "";
    for (i, p) in PAGES.iter().enumerate() {
        if p.group != group {
            group = p.group;
            side = side.item(
                container()
                    .padding(EdgeInsets::only(10.0, 12.0, 10.0, 4.0))
                    .child(text(group).size(11.0).semibold().color(theme().colors.muted_foreground)),
            );
        }
        side = side.item(nav_item(p.title).selected(i == selected).on_select(move || current().set(i)));
    }
    side
}

/// The whole gallery app.
pub fn app() -> Element {
    let i = current().get().min(PAGES.len() - 1);
    let page = &PAGES[i];
    scaffold(keyed(page.id, component(page.build)))
        .top(top_bar(page))
        .side(side_list(i))
        .background(theme().colors.background)
        .into_widget()
}
