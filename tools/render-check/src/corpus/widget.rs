//! widget/ — every case through the public pebbles-widgets API (§7.2, V25). Category Screen,
//! slice Screens, Harness 480×320; judged by self-goldens from V12 (unjudged until then).

use pebbles_core::IntoWidget as _;
use pebbles_foundation::{Color, EdgeInsets, Offset};
use pebbles_icons::tabler;
use pebbles_render::{BorderRadius, BoxShadow};
use pebbles_testing::Harness;
use pebbles_widgets::theme::{DesignLanguage, set_design};
use pebbles_widgets::{
    ButtonVariant, button, calendar, checkbox, chip, column, container, dialog, list_tile, popover, radio,
    row, separator, sheet, slider, spinner, switch, table, tabs, text, text_field, toast, tooltip, wrap,
};

use crate::case::{Case, Category::Screen, Slice::Screens, harness};

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
    ]
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
    page(h, tooltip("Hint", button("Hover")));
    h.hover(Offset::new(50.0, 40.0));
    h.settle();
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
