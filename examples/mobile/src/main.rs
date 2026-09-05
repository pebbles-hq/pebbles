//! A **full, working social app** at phone size (390×844) — post, like, comment,
//! bookmark, follow, notifications, editable profile, live dark mode — all driven by
//! in-memory state (no server; the illusion is entirely `store.rs`).
//!
//! Layout:
//!   store.rs        → the state manager (users / posts / notifications + actions)
//!   screens/        → one file per tab (feed, notifications, profile)
//!   components/      → post card, and the compose + comments bottom sheets
//!   main.rs         → the Scaffold shell: top bar + bottom nav + the FAB

mod components;
mod screens;
mod store;

use pebbles::prelude::*;

use components::compose::open_composer;

fn app() -> impl IntoWidget {
    let tab = create_signal(0_usize);
    let title = ["Pebbles", "Notifications", "Profile"][tab.get().min(2)];

    let mut shell = scaffold(safe_area(body(tab.get()))).top(top_panel(title)).bottom(
        bottom_nav()
            .item(tab_item(lucide::HOUSE, "Home", 0, tab, None))
            .item(tab_item(lucide::BELL, "Alerts", 1, tab, Some(store::unread())))
            .item(tab_item(lucide::USER, "Profile", 2, tab, None)),
    );
    // Compose a new post — the canonical bottom-right action, on the feed only.
    if tab.get() == 0 {
        shell = shell.fab(fab(lucide::PLUS).on_pressed(open_composer));
    }
    shell
}

fn body(tab: usize) -> AnyWidget {
    match tab {
        0 => screens::feed().into_widget(),
        1 => screens::notifications().into_widget(),
        _ => screens::profile().into_widget(),
    }
}

/// A bottom-nav tab: icon (+ an unread dot) over a label, colored by selection.
fn tab_item(
    ic: IconData,
    label: &str,
    index: usize,
    tab: Signal<usize>,
    badge: Option<usize>,
) -> impl IntoWidget {
    let c = theme().colors;
    let selected = tab.get() == index;
    let color = if selected { c.primary } else { c.muted_foreground };

    let glyph: AnyWidget = match badge {
        Some(n) if n > 0 => stack(children![
            icon(ic).size(22.0).color(color),
            positioned(
                container()
                    .decoration(BoxDecoration::new().color(palette::rose::S500).shape(BoxShape::Circle))
                    .width(8.0)
                    .height(8.0),
            )
            .right(0.0)
            .top(0.0),
        ])
        .into_widget(),
        _ => icon(ic).size(22.0).color(color).into_widget(),
    };

    pressable(
        container().padding(EdgeInsets::symmetric(16.0, 8.0)).child(
            column(children![
                glyph,
                gap_h(3.0),
                text(label.to_string()).size(11.0).weight(if selected { 600.0 } else { 500.0 }).color(color),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_size(MainAxisSize::Min),
        ),
    )
    .on_tap(move || tab.set(index))
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(app))
        .title("Pebbles — Social")
        .size(390, 844) // phone-sized; desktop-run for now
        .run()
}
