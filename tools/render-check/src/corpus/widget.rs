//! widget/ — every case through the public pebbles-widgets API (§7.2, V25). Category Screen,
//! slice Screens, Harness 480×320; judged by self-goldens from V12 (unjudged until then).

use pebbles_core::IntoWidget as _;
use pebbles_foundation::{Color, EdgeInsets, Offset};
use pebbles_icons::tabler;
use pebbles_render::{BorderRadius, BoxShadow};
use pebbles_testing::Harness;
use pebbles_widgets::theme::{DesignLanguage, set_design};
use pebbles_widgets::{
    ButtonVariant, button, calendar, center, checkbox, chip, column, container, dialog, list_tile, popover,
    radio, row, separator, sheet, slider, spinner, switch, table, tabs, text, text_field, toast, tooltip,
    wrap,
};

use crate::case::{Case, Category::Screen, Slice::Screens, harness, harness_arg};

const W: u32 = 480;
const H: u32 = 320;

pub fn cases() -> Vec<Case> {
    vec![
        harness("widget/button_variants_default", Screen, Screens, W, H, |h| {
            buttons(h, DesignLanguage::Default)
        }),
        harness("widget/button_variants_tailwind", Screen, Screens, W, H, |h| {
            buttons(h, DesignLanguage::Tailwind)
        }),
        harness("widget/button_variants_material", Screen, Screens, W, H, |h| {
            buttons(h, DesignLanguage::Material)
        }),
        harness("widget/text_field_states", Screen, Screens, W, H, text_field_states),
        harness("widget/toggles", Screen, Screens, W, H, toggles),
        harness("widget/tabs", Screen, Screens, W, H, tabs_case),
        harness("widget/chips", Screen, Screens, W, H, chips),
        harness("widget/spinner_settled", Screen, Screens, W, H, spinner_settled),
        harness("widget/list_tiles_divider", Screen, Screens, W, H, list_tiles_divider),
        harness("widget/dialog_over_scrim", Screen, Screens, W, H, dialog_over_scrim),
        harness("widget/popover_open", Screen, Screens, W, H, popover_open),
        harness("widget/tooltip_visible", Screen, Screens, W, H, tooltip_visible),
        harness("widget/toast_shown", Screen, Screens, W, H, toast_shown),
        harness("widget/sheet_open", Screen, Screens, W, H, sheet_open),
        harness("widget/data_table", Screen, Screens, W, H, data_table),
        harness("widget/calendar", Screen, Screens, W, H, calendar_case),
        harness("widget/card_grid", Screen, Screens, W, H, card_grid),
        harness_arg("widget/control_sizes_xs", Screen, Screens, SW, SH, control_sizes, 0),
        harness_arg("widget/control_sizes_sm", Screen, Screens, SW, SH, control_sizes, 1),
        harness_arg("widget/control_sizes_md", Screen, Screens, SW, SH, control_sizes, 2),
        harness_arg("widget/control_sizes_lg", Screen, Screens, SW, SH, control_sizes, 3),
        harness_arg("widget/control_sizes_custom16", Screen, Screens, SW, SH, control_sizes, 4),
    ]
}

const SW: u32 = 820;
const SH: u32 = 96;

/// A mixed row of every sized control at one [`ControlSize`] (Default design):
/// all of them must be the same height and sit level.
fn control_sizes(h: &mut Harness, which: usize) {
    use pebbles_widgets::{ControlSize, icon_button, pagination, select, toggle, toggle_group_labels};
    set_design(DesignLanguage::Default);
    let s = [ControlSize::Xs, ControlSize::Sm, ControlSize::Md, ControlSize::Lg, ControlSize::custom(16.0)]
        [which];
    page(
        h,
        row(vec![
            button("Save").size(s).into_widget(),
            button("Open").leading(tabler::PLUS).variant(ButtonVariant::Outline).size(s).into_widget(),
            icon_button(tabler::SETTINGS).variant(ButtonVariant::Outline).size(s).into_widget(),
            toggle(true, text("B")).size(s).into_widget(),
            toggle_group_labels(["L", "C", "R"]).value(0).size(s).into_widget(),
            text_field().placeholder("Search").width(110.0).size(s).into_widget(),
            select(["One", "Two"]).value(0).width(100.0).size(s).into_widget(),
            chip("Tag").size(s).into_widget(),
            pagination(3, 9).edges(false).size(s).into_widget(),
        ])
        .spacing(6.0)
        .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Center),
    );
}

/// Mount `child` 24 px in from the top-left corner.
fn page(h: &mut Harness, child: impl pebbles_core::IntoWidget) {
    h.mount_widget(container().padding(EdgeInsets::all(24.0)).child(child));
    h.settle();
}

fn buttons(h: &mut Harness, design: DesignLanguage) {
    set_design(design);
    let variants = [
        ButtonVariant::Secondary,
        ButtonVariant::Outline,
        ButtonVariant::Ghost,
        ButtonVariant::Destructive,
        ButtonVariant::Link,
    ];
    let mut children = vec![button("Primary").into_widget()];
    children.extend(variants.into_iter().map(|v| button(format!("{v:?}")).variant(v).into_widget()));
    children.push(button("Disabled").disabled(true).into_widget());
    page(h, wrap(children).spacing(8.0));
    h.press_tab();
    h.settle();
}

fn text_field_states(h: &mut Harness) {
    page(
        h,
        column(vec![
            text_field().placeholder("Placeholder").into_widget(),
            text_field().value("Filled value").into_widget(),
            text_field().value("Focused").into_widget(),
        ])
        .spacing(12.0),
    );
    for _ in 0..3 {
        h.press_tab();
    }
    h.settle();
}

fn toggles(h: &mut Harness) {
    page(
        h,
        column(vec![
            row(vec![checkbox(true).into_widget(), checkbox(false).into_widget()])
                .spacing(12.0)
                .into_widget(),
            row(vec![radio(true).into_widget(), radio(false).into_widget()]).spacing(12.0).into_widget(),
            row(vec![switch(true).into_widget(), switch(false).into_widget()]).spacing(12.0).into_widget(),
            slider(200.0).into_widget(),
        ])
        .spacing(16.0),
    );
}

fn tabs_case(h: &mut Harness) {
    page(h, tabs(1).tab("A", text("a"), || {}).tab("B", text("b"), || {}).tab("C", text("c"), || {}));
}

fn chips(h: &mut Harness) {
    page(h, row(vec![chip("Tag").into_widget(), chip("Icon").icon(tabler::STAR).into_widget()]).spacing(8.0));
}

fn spinner_settled(h: &mut Harness) {
    page(h, spinner(32.0));
}

fn list_tiles_divider(h: &mut Harness) {
    let mut children = Vec::new();
    for i in 0..6 {
        if i > 0 {
            children.push(separator().into_widget());
        }
        children.push(list_tile(format!("Item {i}")).subtitle("Secondary line").into_widget());
    }
    page(h, column(children));
}

fn dialog_over_scrim(h: &mut Harness) {
    page(h, column(vec![text("Page behind the scrim").into_widget(), button("Underneath").into_widget()]));
    dialog(button("OK")).title("Title").open();
    h.settle();
}

fn popover_open(h: &mut Harness) {
    page(h, popover(text("Popover content"), button("Trigger")));
    h.click(Offset::new(50.0, 40.0));
    h.settle();
}

fn tooltip_visible(h: &mut Harness) {
    // The tooltip chip lives in the passive overlay layer, so the page needs the overlay
    // host the shell gives every app; the trigger is centred so the chip has room above it.
    h.mount_widget(pebbles_widgets::View::new(
        Color::WHITE,
        pebbles_widgets::OverlayHost::wrap(center(tooltip("Hint", button("Hover"))).into_widget()),
    ));
    h.settle();
    h.hover(Offset::new(f64::from(W) / 2.0, f64::from(H) / 2.0));
    h.advance(1.0); // past the 0.5 s hover delay
}

fn toast_shown(h: &mut Harness) {
    page(h, text("Page"));
    toast("Saved").description("All good").duration(3600.0).show();
    h.settle();
}

fn sheet_open(h: &mut Harness) {
    page(h, text("Page"));
    sheet(text("Sheet content")).title("Sheet").open();
    h.settle();
}

fn data_table(h: &mut Harness) {
    let mut t = table(vec!["Name".into(), "Qty".into(), "Price".into()]);
    for (name, qty, price) in [
        ("Apples", "3", "$1.20"),
        ("Bread", "1", "$2.50"),
        ("Cheese", "2", "$7.80"),
        ("Dates", "12", "$4.10"),
        ("Eggs", "6", "$3.30"),
        ("Figs", "4", "$5.00"),
    ] {
        t = t.row([name, qty, price]);
    }
    page(h, t);
}

fn calendar_case(h: &mut Harness) {
    // A fixed past month: the calendar marks today from the system clock, which would change
    // the reference every day.
    page(h, calendar(|_, _, _| {}).month(2024, 2).selected(2024, 2, 14));
}

fn card_grid(h: &mut Harness) {
    let cards = (0..6).map(|i| {
        container()
            .width(130.0)
            .height(80.0)
            .color(Color::WHITE)
            .radius(BorderRadius::all(12.0))
            .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 60), Offset::new(0.0, 4.0), 12.0, 0.0))
            .padding(EdgeInsets::all(12.0))
            .child(text(format!("Card {i}")))
            .into_widget()
    });
    page(h, wrap(cards.collect::<Vec<_>>()).spacing(16.0));
}
