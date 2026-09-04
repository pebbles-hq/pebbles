//! A3: sticky section headers (pinned + push-off) and the collapsing hero
//! header. Driven headlessly through real scroll offsets.

use std::cell::RefCell;

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Point, Size, palette};
use pebbles_render::{RenderParagraph, RenderTransform, TextEnv};
use pebbles_testing::{frame};
use pebbles_widgets::{
    ScrollController, View, collapsing_header, list_tile, section_header, sticky_list, text,
    use_scroll_controller,
};

thread_local! {
    static CTL: RefCell<Option<ScrollController>> = const { RefCell::new(None) };
}

fn rows(labels: &[&str]) -> Vec<pebbles_core::AnyWidget> {
    labels
        .iter()
        .map(|l| list_tile(*l).into_widget())
        .collect()
}

fn sticky_root() -> impl IntoWidget {
    let controller = use_scroll_controller();
    CTL.with(|c| *c.borrow_mut() = Some(controller));
    sticky_list()
        .section(section_header("Alpha"), rows(&["a1", "a2", "a3"]))
        .section(section_header("Beta"), rows(&["b1", "b2"]))
        .section(section_header("Gamma"), rows(&["c1", "c2", "c3", "c4"]))
        .header_extent(40.0)
        .row_extent(48.0)
        .controller(controller)
}

#[test]
fn sticky_header_pins_the_active_section() {
    CTL.with(|c| *c.borrow_mut() = None);
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 220.0);
    ui.mount_root(View::new(palette::WHITE, component(sticky_root)).into_widget());
    frame(&mut ui, &mut env, win);

    let ctl = CTL.with(|c| c.borrow().expect("controller captured"));
    // Section tops: Alpha 0, Beta 184, Gamma 320 (40+3×48=184; +40+2×48=320).
    ctl.jump_to(200.0); // mid-Beta
    frame(&mut ui, &mut env, win);

    // The pinned header sits at the viewport top and is Beta's.
    let chain = ui.render_tree().hit_test(Offset::new(30.0, 10.0));
    let pinned = chain
        .iter()
        .rev()
        .find_map(|&id| ui.render_tree().object_ref(id).downcast_ref::<RenderParagraph>())
        .expect("a paragraph under the top edge");
    assert_eq!(pinned.text, "Beta", "mid-section-B pins B's header");

    // No push-off while the next header is far below: identity translation.
    let t = ui.render_tree().find::<RenderTransform>().unwrap();
    let tr = ui.render_tree().object_ref(t).downcast_ref::<RenderTransform>().unwrap();
    let p = tr.matrix * Point::new(0.0, 0.0);
    assert!(p.y.abs() < 1e-6, "no push-off mid-section: {p:?}");

    // Boundary −10px: Gamma's top (320) is 10px below the viewport top → the
    // pinned Beta header slides up by 30.
    ctl.jump_to(310.0);
    frame(&mut ui, &mut env, win);
    let t = ui.render_tree().find::<RenderTransform>().unwrap();
    let tr = ui.render_tree().object_ref(t).downcast_ref::<RenderTransform>().unwrap();
    let p = tr.matrix * Point::new(0.0, 0.0);
    assert!(
        (p.y + 30.0).abs() < 1e-6,
        "push-off translates the pinned header by the gap: {p:?}"
    );
}

fn collapse_root() -> impl IntoWidget {
    let controller = use_scroll_controller();
    CTL.with(|c| *c.borrow_mut() = Some(controller));
    let rows: Vec<pebbles_core::AnyWidget> = (1..=12)
        .map(|i| text(format!("content row {i}")).into_widget())
        .collect();
    collapsing_header(240.0, 64.0, |t| text(format!("t={t:.2}")).size(16.0))
        .content(rows)
        .controller(controller)
}

#[test]
fn collapsing_header_tracks_the_scroll_progress() {
    CTL.with(|c| *c.borrow_mut() = None);
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    let win = Size::new(300.0, 220.0);
    ui.mount_root(View::new(palette::WHITE, component(collapse_root)).into_widget());
    frame(&mut ui, &mut env, win);

    let ctl = CTL.with(|c| c.borrow().expect("controller captured"));
    let hero_t = |ui: &Ui| {
        ui.render_tree()
            .find_all::<RenderParagraph>()
            .into_iter()
            .map(|id| ui.render_tree().object_ref(id).downcast_ref::<RenderParagraph>().unwrap().text.clone())
            .find(|s| s.starts_with("t="))
            .expect("the hero paragraph")
    };

    ctl.jump_to(0.0); // t = 1 (fully expanded)
    frame(&mut ui, &mut env, win);
    assert_eq!(hero_t(&ui), "t=1.00", "expanded at the top");

    ctl.jump_to(120.0); // halfway: (240−120)/176 = 0.68
    frame(&mut ui, &mut env, win);
    let hero = hero_t(&ui);
    assert_eq!(hero, "t=0.68", "halfway collapses the hero");

    ctl.jump_to(240.0); // t = 0 (fully collapsed)
    frame(&mut ui, &mut env, win);
    assert_eq!(hero_t(&ui), "t=0.00", "collapsed at full scroll");
}
