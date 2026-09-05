//! The root app component: a `Scaffold` with a `SideNav` and a `RouteView` body.
//! The current route is a **global signal** (`state::route`) read here and written
//! by the nav items — no prop-drilling.

use pebbles::prelude::*;

use crate::screens;
use crate::state::{NAV, label_for, navigate, route};

/// A small uppercase section header row for the sidebar.
fn nav_section(label: &str) -> impl IntoWidget {
    let c = theme().colors;
    column(children![
        gap_h(12.0),
        padding(
            EdgeInsets::symmetric(8.0, 2.0),
            text(label).size(11.0).semibold().color(c.muted_foreground),
        ),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
    .main_axis_size(MainAxisSize::Min)
}

/// `GALLERY_TOUR=<ms>`: hop to the next screen every `<ms>`, forever — the
/// burn-in / demo tour (exercises every screen's mount, render, unmount).
fn install_tour() {
    // ONCE per process — app() re-renders on every navigation, and re-installing
    // here would replace the pending hop with a fresh index-0 chain (the tour
    // would then visit screen #0 forever; caught by the hop log below).
    thread_local! {
        static INSTALLED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    if INSTALLED.with(|f| f.replace(true)) {
        return;
    }
    let Some(ms) = std::env::var("GALLERY_TOUR").ok().and_then(|v| v.parse::<u64>().ok()) else {
        return;
    };
    fn hop(all: std::rc::Rc<Vec<String>>, i: usize, key: u64, secs: f64) {
        pebbles::core::animation::set_timeout(key, secs, move || {
            let route = all[i % all.len()].clone();
            // Log each hop so burn-in output PROVES which screens were covered.
            eprintln!("gallery tour → {route}");
            navigate(&route);
            hop(all.clone(), i + 1, key, secs);
        });
    }
    // GALLERY_TOUR_ROUTES="overview,typography" pins the tour to just those
    // screens (screen-focused burn-in); default = every NAV route.
    let all: Vec<String> = match std::env::var("GALLERY_TOUR_ROUTES") {
        Ok(list) if !list.trim().is_empty() => list.split(',').map(|s| s.trim().to_string()).collect(),
        _ => NAV.iter().flat_map(|g| g.routes.iter().map(|(r, _, _)| (*r).to_string())).collect(),
    };
    // A fixed caller-owned timer key (set_timeout ids are a caller namespace).
    let key = u64::from_le_bytes(*b"gal-tour");
    hop(std::rc::Rc::new(all), 0, key, (ms as f64 / 1000.0).max(0.05));
}

pub fn app() -> impl IntoWidget {
    install_tour();
    let c = theme().colors;
    let current = route().get();

    // ----- side navigation -----
    let brand = padding(
        EdgeInsets::symmetric(6.0, 10.0),
        row(children![
            icon(lucide::GEM).size(20.0).color(c.primary),
            gap_w(8.0),
            text("Pebbles").size(17.0).bold().color(c.foreground),
        ])
        .main_axis_size(MainAxisSize::Min),
    );
    let mut side = side_nav()
        .width(232.0)
        .header(brand)
        .footer(padding(EdgeInsets::all(6.0), muted("v0.0.1 · Solid-style on Vello")));
    for group in NAV {
        side = side.item(nav_section(group.label));
        for (r, ic, label) in group.routes {
            let route_id = *r;
            let selected = current.as_str() == route_id;
            side = side
                .item(nav_item(*label).icon(*ic).selected(selected).on_select(move || navigate(route_id)));
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
        .route("command", || component(screens::command::command_screen))
        .route("toggles", || component(screens::toggles::toggles))
        .route("radio-group", || component(screens::radiogroup::radio_groups))
        .route("slider", || component(screens::sliders::sliders))
        .route("progress", || component(screens::progress::progress_screen))
        .route("button-group", || component(screens::button_group::button_groups))
        .route("dialog", || component(screens::dialog::dialogs_screen))
        .route("windows", || component(screens::windows::windows))
        .route("rows", || component(screens::rows::rows))
        .route("columns", || component(screens::columns::columns))
        .route("stacks", || component(screens::stacks::stacks))
        .route("wraps", || component(screens::wraps::wraps))
        .route("boxes", || component(screens::boxes::boxes))
        .route("scrolling", || component(screens::scrolling::scrolling))
        .route("animated-container", || component(screens::animated_container::animated_containers))
        // Motion & transitions — one screen per widget.
        .route("animated-opacity", || component(screens::motion::animated_opacity_screen))
        .route("animated-scale", || component(screens::motion::animated_scale_screen))
        .route("animated-rotation", || component(screens::motion::animated_rotation_screen))
        .route("animated-slide", || component(screens::motion::animated_slide_screen))
        .route("animated-align", || component(screens::motion::animated_align_screen))
        .route("animated-padding", || component(screens::motion::animated_padding_screen))
        .route("animated-positioned", || component(screens::motion::animated_positioned_screen))
        .route("fade-transition", || component(screens::motion::fade_transition_screen))
        .route("scale-transition", || component(screens::motion::scale_transition_screen))
        .route("rotation-transition", || component(screens::motion::rotation_transition_screen))
        .route("slide-transition", || component(screens::motion::slide_transition_screen))
        .route("size-transition", || component(screens::motion::size_transition_screen))
        .route("positioned-transition", || component(screens::motion::positioned_transition_screen))
        .route("decorated-box-transition", || component(screens::motion::decorated_box_transition_screen))
        .route("animated-switcher", || component(screens::motion::animated_switcher_screen))
        .route("animated-cross-fade", || component(screens::motion::animated_cross_fade_screen))
        .route("dismissible", || component(screens::motion::dismissible_screen))
        .route("animated-list", || component(screens::motion::animated_list_screen))
        .route("animated-grid", || component(screens::motion::animated_grid_screen))
        .route("hero", || component(screens::motion::hero_screen))
        // Drag & drop / pointer control — one screen per widget.
        .route("draggable", || component(screens::dnd::draggable_screen))
        .route("drag-target", || component(screens::dnd::drag_target_screen))
        .route("long-press-draggable", || component(screens::dnd::long_press_draggable_screen))
        .route("ignore-pointer", || component(screens::dnd::ignore_pointer_screen))
        .route("absorb-pointer", || component(screens::dnd::absorb_pointer_screen))
        .route("interactive-viewer", || component(screens::dnd::interactive_viewer_screen))
        .route("reorderable-list-view", || component(screens::dnd::reorderable_list_view_screen))
        // Layout — one screen per long-tail layout widget.
        .route("indexed-stack", || component(screens::layout_extra::indexed_stack_screen))
        .route("offstage", || component(screens::layout_extra::offstage_screen))
        .route("visibility", || component(screens::layout_extra::visibility_screen))
        .route("baseline", || component(screens::layout_extra::baseline_screen))
        .route("rotated-box", || component(screens::layout_extra::rotated_box_screen))
        .route("unconstrained-box", || component(screens::layout_extra::unconstrained_box_screen))
        .route("sized-overflow-box", || component(screens::layout_extra::sized_overflow_box_screen))
        .route("fractional-translation", || component(screens::layout_extra::fractional_translation_screen))
        .route("table-layout", || component(screens::layout_extra::table_screen))
        .route("custom-single-child", || component(screens::layout_extra::custom_single_child_layout_screen))
        .route("custom-multi-child", || component(screens::layout_extra::custom_multi_child_layout_screen))
        .route("flow", || component(screens::layout_extra::flow_screen))
        .route("layout-builder", || component(screens::layout_extra::layout_builder_screen))
        // Painting & effects — one screen per widget.
        .route("clip-rect", || component(screens::painting::clip_rect_screen))
        .route("clip-oval", || component(screens::painting::clip_oval_screen))
        .route("clip-path", || component(screens::painting::clip_path_screen))
        .route("color-filtered", || component(screens::painting::color_filtered_screen))
        .route("shader-mask", || component(screens::painting::shader_mask_screen))
        // Input & forms — additions (the base input set has its own screens).
        .route("choice-chip", || component(screens::inputs_extra::choice_chip_screen))
        .route("filter-chip", || component(screens::inputs_extra::filter_chip_screen))
        .route("action-chip", || component(screens::inputs_extra::action_chip_screen))
        .route("stepper", || component(screens::inputs_extra::stepper_screen))
        .route("selectable-text", || component(screens::inputs_extra::selectable_text_screen))
        // Material staples — one screen per widget.
        .route("fab", || component(screens::material::fab_screen))
        .route("grid-tile", || component(screens::material::grid_tile_screen))
        // Async & accessibility long-tail.
        .route("stream-builder", || component(screens::async_a11y::stream_builder_screen))
        .route("semantics-combinators", || component(screens::async_a11y::semantics_combinators_screen))
        // Scaffold slots + mobile-specific.
        .route("media-query", || component(screens::mobile::media_query_screen))
        .route("safe-area", || component(screens::mobile::safe_area_screen))
        .route("orientation-builder", || component(screens::mobile::orientation_builder_screen))
        .route("scaffold-slots", || component(screens::mobile::scaffold_slots_screen))
        // Long-tail.
        .route("placeholder", || component(screens::longtail::placeholder_screen))
        .route("banner", || component(screens::longtail::banner_screen))
        .route("canvas", || component(screens::canvas::canvas_screen))
        .route("resizable", || component(screens::resizable::resizables))
        .route("badge", || component(screens::badge::badges))
        .route("chips", || component(screens::chips::chips))
        .route("alert", || component(screens::alert::alerts))
        .route("skeleton", || component(screens::skeleton::skeletons))
        .route("kbd", || component(screens::kbd::kbd_screen))
        .route("empty", || component(screens::empty::empty_screen))
        .route("separator", || component(screens::separator::separators))
        .route("avatar", || component(screens::avatar::avatars))
        .route("card", || component(screens::card::cards))
        .route("collapsible", || component(screens::collapsible::collapsibles))
        .route("styling", || component(screens::styling::styling))
        .route("colors", || component(screens::colors::colors))
        .route("context", || component(screens::context::context))
        .route("tabs", || component(screens::tabs::tabs_screen))
        .route("accordion", || component(screens::accordion::accordions))
        .route("breadcrumb", || component(screens::breadcrumb::breadcrumbs))
        .route("menubar", || component(screens::menubar::menubars))
        .route("pagination", || component(screens::pagination::paginations))
        .route("toolbar", || component(screens::toolbars::toolbars))
        .route("chrome", || component(screens::chrome::chrome_screen))
        .route("routing", || component(screens::routing::routing_screen))
        .route("tooltips", || component(screens::tooltips::tooltips))
        .route("popovers", || component(screens::popovers::popovers))
        .route("sheets", || component(screens::sheets::sheets))
        .route("toasts", || component(screens::toasts::toasts))
        .route("context-menu", || component(screens::context_menu::context_menu))
        .route("list", || component(screens::list::lists))
        .route("data-table", || component(screens::data_table::data_tables))
        .route("file-explorer", || component(screens::file_explorer::file_explorer_screen))
        .route("split-view", || component(screens::split_view::split_views))
        .route("list-view", || component(screens::list_view::list_view))
        .route("grid-view", || component(screens::grid_view::grid_view))
        .route("typography", || component(screens::typography::typography))
        .route("fonts", || component(screens::fonts::fonts))
        .route("icons", || component(screens::icons::icons))
        .route("images", || component(screens::images::images))
        .fallback(|| component(screens::overview::overview));

    // A live light/dark toggle — flips the global theme signal; every component that
    // read `theme()` (i.e. the whole tree) re-renders. Icon shows the target mode.
    let dark = theme().dark;
    let theme_toggle =
        icon_button(if dark { lucide::SUN } else { lucide::MOON }).on_pressed(|| toggle_theme());

    let top = top_panel(label_for(&current))
        .leading(icon(IconKind::Menu).size(18.0).color(c.muted_foreground))
        .action(theme_toggle)
        .action(badge("v0.0.1").variant(BadgeVariant::Secondary));

    scaffold(body).top(top).side(side)
}
