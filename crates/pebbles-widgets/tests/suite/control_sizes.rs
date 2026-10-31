//! The shared [`ControlSize`] scale: at every size (xs / sm / md / lg and custom
//! sizes down to 16 px) every sized component — text button, icon button, button
//! with an icon, toggle, segmented group, text field, select, chip, pagination —
//! has exactly the size's height, so a mixed row lines up. Tiny custom sizes lay
//! out without the label overflowing; pagination items are level.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{CrossAxisAlignment, Offset, Size, palette};
use pebbles_render::{IconKind, RenderParagraph, RenderPointerListener, TextEnv};
use pebbles_testing::draw_frame as frame;
use pebbles_widgets::{
    ButtonVariant, ControlMetrics, ControlSize, DesignLanguage, Theme, View, button, chip, column,
    icon_button, pagination, position_probe, row, select, text, text_field, toggle, toggle_group_labels,
};

thread_local! {
    static RECTS: RefCell<Vec<(String, Offset, Size)>> = const { RefCell::new(Vec::new()) };
    static SIZE: RefCell<ControlSize> = const { RefCell::new(ControlSize::Md) };
    static ONLY_PAGINATION: RefCell<bool> = const { RefCell::new(false) };
}

fn probe(name: &'static str, w: impl IntoWidget + 'static) -> pebbles_core::widget::AnyWidget {
    position_probe(
        move |o, s| {
            RECTS.with(|r| {
                let mut r = r.borrow_mut();
                if let Some(e) = r.iter_mut().find(|e| e.0 == name) {
                    e.1 = o;
                    e.2 = s;
                } else {
                    r.push((name.to_string(), o, s));
                }
            })
        },
        w,
    )
    .into_widget()
}

fn mixed_row() -> pebbles_core::widget::AnyWidget {
    let s = SIZE.with(|s| *s.borrow());
    if ONLY_PAGINATION.with(|o| *o.borrow()) {
        return probe("pagination", pagination(5, 20).size(s));
    }
    row(vec![
        probe("button", button("Save").size(s)),
        probe("button+icon", button("Open").leading(IconKind::Plus).size(s).variant(ButtonVariant::Outline)),
        probe("icon_button", icon_button(IconKind::Plus).size(s).variant(ButtonVariant::Outline)),
        probe("toggle", toggle(true, text("B")).size(s)),
        probe("segmented", toggle_group_labels(["L", "C", "R"]).value(0).size(s)),
        probe("text_field", text_field().placeholder("Search").width(110.0).size(s)),
        probe("select", select(["One", "Two"]).value(0).width(110.0).size(s)),
        probe("chip", chip("Tag").size(s)),
        probe("pagination", pagination(3, 9).size(s)),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .spacing(6.0)
    .into_widget()
}

fn measure(theme: Theme, size: ControlSize) -> (Ui, Vec<(String, Offset, Size)>) {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    theme.make_current();
    RECTS.with(|r| r.borrow_mut().clear());
    SIZE.with(|s| *s.borrow_mut() = size);
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| column(vec![mixed_row()]).cross_axis_alignment(CrossAxisAlignment::Start)),
        )
        .into_widget(),
    );
    frame(&mut ui, &mut env, Size::new(1400.0, 120.0));
    let rects = RECTS.with(|r| r.borrow().clone());
    let want = if ONLY_PAGINATION.with(|o| *o.borrow()) { 1 } else { 9 };
    assert_eq!(rects.len(), want, "every control reported a rect");
    (ui, rects)
}

fn assert_uniform(theme: Theme, size: ControlSize) {
    let h = theme.size(size).height;
    let (_, rects) = measure(theme, size);
    let top = rects[0].1.y;
    for (name, o, s) in &rects {
        assert!((s.height - h).abs() < 0.01, "{size:?}: {name} is {} tall, expected {h}", s.height);
        assert!((o.y - top).abs() < 0.01, "{size:?}: {name} top {} != {top} (row not level)", o.y);
    }
    let icon = rects.iter().find(|r| r.0 == "icon_button").unwrap().2;
    assert!((icon.width - h).abs() < 0.01, "{size:?}: icon button is square ({icon:?})");
}

#[test]
fn every_sized_control_shares_the_height_at_each_default_size() {
    for size in [ControlSize::Xs, ControlSize::Sm, ControlSize::Md, ControlSize::Lg] {
        assert_uniform(Theme::light(), size);
    }
}

#[test]
fn default_scale_matches_the_standard_desktop_metrics() {
    let t = Theme::light();
    assert_eq!(t.font_size, 13.0);
    let md = t.size(ControlSize::Md);
    assert_eq!((md.height, md.font, md.icon), (28.0, 13.0, 16.0));
    let heights: Vec<f64> = [ControlSize::Xs, ControlSize::Sm, ControlSize::Md, ControlSize::Lg]
        .map(|s| t.size(s).height)
        .to_vec();
    assert_eq!(heights, vec![20.0, 24.0, 28.0, 32.0]);
    assert_eq!(t.control_height, md.height, "control_height token is the md height");
}

#[test]
fn material_and_tailwind_keep_their_own_uniform_scales() {
    for lang in [DesignLanguage::Material, DesignLanguage::Tailwind] {
        let t = Theme::light().design(lang);
        assert!(t.size(ControlSize::Md).height > 28.0, "{lang:?} md stays roomier");
        assert_uniform(t, ControlSize::Md);
        assert_uniform(t, ControlSize::Sm);
    }
}

#[test]
fn custom_sizes_work_down_to_tiny_controls_without_overflow() {
    for h in [16.0, 18.0, 44.0] {
        assert_uniform(Theme::light(), ControlSize::custom(h));
    }
    // Explicit metrics are honoured as given.
    let exact = ControlSize::Custom(ControlMetrics { height: 22.0, font: 11.0, padding_x: 5.0, icon: 12.0 });
    assert_uniform(Theme::light(), exact);

    // At 16 px every label still fits inside its control.
    let (ui, _) = measure(Theme::light(), ControlSize::custom(16.0));
    for id in ui.render_tree().find_all::<RenderParagraph>() {
        let s = ui.render_tree().size_of(id);
        assert!(s.height <= 16.0 + 0.01, "a label overflows a 16 px control: {s:?}");
    }
}

#[test]
fn pagination_items_are_level_and_square() {
    for size in [ControlSize::Sm, ControlSize::Md, ControlSize::custom(18.0)] {
        let h = Theme::light().size(size).height;
        ONLY_PAGINATION.with(|o| *o.borrow_mut() = true);
        let (ui, rects) = measure(Theme::light(), size);
        ONLY_PAGINATION.with(|o| *o.borrow_mut() = false);
        assert!((rects[0].2.height - h).abs() < 0.01, "{size:?}: the pagination row is {:?}", rects[0].2);
        // Every interactive control in the row (all buttons) is exactly `h` tall.
        let listeners = ui.render_tree().find_all::<RenderPointerListener>();
        assert!(listeners.len() >= 9, "pagination controls found: {}", listeners.len());
        for id in listeners {
            let s = ui.render_tree().size_of(id);
            assert!((s.height - h).abs() < 0.01, "{size:?}: a control is {} tall, expected {h}", s.height);
        }
    }
}
