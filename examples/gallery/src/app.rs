//! The root app component: a `Scaffold` with a `SideNav` and a `RouteView` body.
//! The current route is a **global signal** ([`state::route`]) read here and written
//! by the nav items — no prop-drilling.

use pebbles::prelude::*;

use crate::screens;
use crate::state::{NAV, label_for, navigate, route};

/// A small uppercase section header row for the sidebar.
fn nav_section(label: &str) -> impl IntoWidget {
    let c = theme().colors;
    column(children![
        SizedBox::spacer(0.0, 12.0),
        Padding::new(
            EdgeInsets::symmetric(8.0, 2.0),
            text(label).size(11.0).semibold().color(c.muted_foreground),
        ),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_min()
}

pub fn app() -> impl IntoWidget {
    let c = theme().colors;
    let current = route().get();

    // ----- side navigation -----
    let brand = Padding::new(
        EdgeInsets::symmetric(6.0, 10.0),
        row(children![
            icon(lucide::GEM).size(20.0).color(c.primary),
            SizedBox::spacer(8.0, 0.0),
            text("Pebbles").size(17.0).bold().color(c.foreground),
        ])
        .main_axis_min(),
    );
    let mut side = side_nav()
        .width(232.0)
        .header(brand)
        .footer(Padding::new(EdgeInsets::all(6.0), muted("v0.0.1 · Solid-style on Vello")));
    for group in NAV {
        side = side.item(nav_section(group.label));
        for (r, ic, label) in group.routes {
            let route_id = *r;
            let selected = current.as_str() == route_id;
            side = side.item(
                nav_item(*label)
                    .icon(*ic)
                    .selected(selected)
                    .on_select(move || navigate(route_id)),
            );
        }
    }

    // ----- routed content (each route builds its screen component) -----
    let body = route_view(current.clone())
        .route("overview", || component(screens::overview::overview))
        .route("buttons", || component(screens::buttons::buttons))
        .route("text-fields", || component(screens::text_fields::text_fields))
        .route("date-picker", || component(screens::date_picker::date_picker))
        .route("select", || component(screens::selects::selects))
        .route("combobox", || component(screens::combobox::combobox_screen))
        .route("toggles", || component(screens::toggles::toggles))
        .route("radio-group", || component(screens::radiogroup::radio_groups))
        .route("slider", || component(screens::sliders::sliders))
        .route("progress", || component(screens::progress::progress_screen))
        .route("button-group", || component(screens::button_group::button_groups))
        .route("dialog", || component(screens::dialog::dialogs_screen))
        .route("windows", || component(screens::windows::windows))
        .route("layout", || component(screens::layout::layout))
        .route("resizable", || component(screens::resizable::resizables))
        .route("surfaces", || component(screens::surfaces::surfaces))
        .route("separator", || component(screens::separator::separators))
        .route("avatar", || component(screens::avatar::avatars))
        .route("card", || component(screens::card::cards))
        .route("collapsible", || component(screens::collapsible::collapsibles))
        .route("styling", || component(screens::styling::styling))
        .route("colors", || component(screens::colors::colors))
        .route("navigation", || component(screens::navigation::navigation))
        .route("data", || component(screens::data::data))
        .route("typography", || component(screens::typography::typography))
        .route("icons", || component(screens::icons::icons))
        .route("images", || component(screens::images::images))
        .fallback(|| component(screens::overview::overview));

    let top = top_panel(label_for(&current))
        .leading(icon(IconKind::Menu).size(18.0).color(c.muted_foreground))
        .action(badge("v0.0.1").variant(BadgeVariant::Secondary));

    scaffold(body).top(top).side(side)
}
