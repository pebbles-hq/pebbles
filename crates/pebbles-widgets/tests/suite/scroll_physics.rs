//! A4: drag-to-scroll, fling, rubber-band overscroll and the physics knobs.
//! Driven headlessly through a real `Ui` with a fixed drag clock.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::{RenderScroll, ScrollPhysics, TextEnv};
use pebbles_widgets::{SingleChildScrollView, View, column, gap_h, text};

/// A tall (scrollable) column inside a drag-scroll viewport with the given physics.
fn tall(overscroll: bool) -> SingleChildScrollView {
    let mut kids: Vec<pebbles_core::AnyWidget> = Vec::new();
    for i in 0..30 {
        kids.push(text(format!("row {i}")).into_widget());
        kids.push(gap_h(40.0).into_widget());
    }
    SingleChildScrollView::vertical(column(kids).main_axis_size(pebbles_foundation::MainAxisSize::Min))
        .drag_scroll(true)
        .physics(ScrollPhysics { overscroll, ..Default::default() })
}

fn scroll_of(ui: &Ui) -> (f64, f64) {
    let id = ui.render_tree().find::<RenderScroll>().unwrap();
    let s = ui.render_tree().object_ref(id).downcast_ref::<RenderScroll>().unwrap();
    (s.offset, s.target)
}

#[test]
fn content_drag_moves_the_offset_one_to_one() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(palette::WHITE, component(|| tall(false))).into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert!(ui.render_tree().find::<RenderScroll>().is_some_and(|_| true));

    // Claim the drag at the middle of the viewport and pull up 60px (offset grows).
    assert!(ui.begin_content_drag(Offset::new(150.0, 100.0)), "drag-scroll claims the drag");
    assert!(ui.content_drag_active());
    assert!(ui.update_content_drag(Offset::new(150.0, 40.0)), "offset follows the pointer");
    let (off, _) = scroll_of(&ui);
    assert!((off - 60.0).abs() < 1e-6, "1:1 tracking: {off}");

    // Release with no velocity: offset stays (spring settles at target).
    assert!(ui.end_content_drag(Offset::new(150.0, 40.0)));
    let (_, target) = scroll_of(&ui);
    assert!((target - 60.0).abs() < 1e-6, "release keeps the dragged position");
}

#[test]
fn rubber_band_overscroll_resists_and_springs_back() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.set_test_clock(Some(0.0));
    ui.mount_root(
        View::new(palette::WHITE, component(|| tall(true))).into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));

    // Drag down 90px from the top: the finger moves 90px, but the offset only
    // rubber-bands to −30 (excess/3).
    assert!(ui.begin_content_drag(Offset::new(150.0, 40.0)));
    assert!(ui.update_content_drag(Offset::new(150.0, 130.0)));
    let (off, _) = scroll_of(&ui);
    assert!((off + 30.0).abs() < 0.5, "excess/3 rubber band: {off}");

    // Release: the offset springs back to 0.
    assert!(ui.end_content_drag(Offset::new(150.0, 130.0)));
    assert!(ui.content_drag_active() == false);
    let mut settled = false;
    for _ in 0..300 {
        if !ui.tick_scrolls(1.0 / 60.0) {
            settled = true;
            break;
        }
    }
    let (off, target) = scroll_of(&ui);
    assert!(settled, "the spring-back settles");
    assert!(off.abs() < 0.2 && target.abs() < 0.2, "rests at 0: off={off} target={target}");
}

#[test]
fn release_with_velocity_flings_then_decays() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.set_test_clock(Some(0.0));
    ui.mount_root(
        View::new(palette::WHITE, component(|| tall(true))).into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));

    // Drag up 200px over 0.2s → velocity ≈ 1000 px/s at release.
    assert!(ui.begin_content_drag(Offset::new(150.0, 180.0)));
    assert!(ui.update_content_drag(Offset::new(150.0, 80.0)));
    ui.set_test_clock(Some(0.2));
    assert!(ui.update_content_drag(Offset::new(150.0, -20.0)));
    let before = scroll_of(&ui).0;
    assert!(ui.end_content_drag(Offset::new(150.0, -20.0)));

    // The fling decays over the following ticks; the target advances past the
    // finger's release point, then the driver idles.
    let mut settled = false;
    for _ in 0..600 {
        if !ui.tick_scrolls(1.0 / 60.0) {
            settled = true;
            break;
        }
    }
    assert!(settled, "the fling settles");
    let (_, target) = scroll_of(&ui);
    assert!(target > before + 100.0, "the fling traveled past the finger: {target}");
}

#[test]
fn wheel_stays_hard_clamped_at_the_edges() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(palette::WHITE, component(|| tall(true))).into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));
    // Wheel past the top: the offset never goes negative.
    ui.dispatch_scroll(Offset::new(150.0, 100.0), -200.0);
    ui.dispatch_scroll(Offset::new(150.0, 100.0), -200.0);
    let (off, target) = scroll_of(&ui);
    assert!(off >= -1e-6 && target >= -1e-6, "wheel is hard-clamped: off={off}");
}

#[test]
fn child_pan_target_wins_over_drag_scroll() {
    // A pan-hungry child inside a drag-scroll view: the drag goes to the child.
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                SingleChildScrollView::vertical(column(vec![
                    pebbles_widgets::GestureDetector::new(text("slider-ish"))
                        .on_pan_start(|| {})
                        .on_pan_update(|| {})
                        .on_pan_end(|| {})
                        .into_widget(),
                ])
                .main_axis_size(pebbles_foundation::MainAxisSize::Min))
                .drag_scroll(true)
                .physics(ScrollPhysics { overscroll: true, ..Default::default() })
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert!(
        !ui.begin_content_drag(Offset::new(150.0, 10.0)),
        "a pan-hungry child under the pointer claims the drag"
    );
}

// ---------------------------------------------------------------------------
// A5: pull-to-refresh
// ---------------------------------------------------------------------------

thread_local! {
    static REFRESHES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    static DONE: std::cell::RefCell<Option<pebbles_widgets::RefreshDone>> = const { std::cell::RefCell::new(None) };
}

fn feed() -> impl IntoWidget {
    let kids: Vec<pebbles_core::AnyWidget> = (0..20)
        .map(|i| text(format!("feed row {i}")).into_widget())
        .collect();
    pebbles_widgets::refresh_indicator(column(kids).main_axis_size(pebbles_foundation::MainAxisSize::Min))
        .threshold(64.0)
        .on_refresh(|done| {
            REFRESHES.with(|r| r.set(r.get() + 1));
            DONE.with(|d| *d.borrow_mut() = Some(done));
        })
}

#[test]
fn pull_to_refresh_arms_fires_once_and_finishes() {
    REFRESHES.with(|r| r.set(0));
    DONE.with(|d| *d.borrow_mut() = None);
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(feed)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert!(ui.render_tree().find::<pebbles_render::RenderSpinner>().is_none(), "no spinner at rest");

    // Pull down 192px (banded → −64 = the threshold) and release: exactly one
    // on_refresh fires and the spinner row appears.
    assert!(ui.begin_content_drag(Offset::new(150.0, 30.0)));
    assert!(ui.update_content_drag(Offset::new(150.0, 30.0 + 192.0)));
    ui.end_content_drag(Offset::new(150.0, 30.0 + 192.0));
    assert_eq!(REFRESHES.with(|r| r.get()), 1, "armed release fires on_refresh once");
    ui.rebuild_if_dirty();
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert!(ui.render_tree().find::<pebbles_render::RenderSpinner>().is_some(), "spinner holds while refreshing");

    // A second pull while refreshing is ignored (v1 contract).
    assert!(ui.begin_content_drag(Offset::new(150.0, 30.0)));
    assert!(ui.update_content_drag(Offset::new(150.0, 30.0 + 192.0)));
    ui.end_content_drag(Offset::new(150.0, 30.0 + 192.0));
    assert_eq!(REFRESHES.with(|r| r.get()), 1, "second pull while refreshing is ignored");

    // finish() collapses the row.
    DONE.with(|d| d.borrow().as_ref().expect("done handle delivered").finish());
    ui.rebuild_if_dirty();
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert!(ui.render_tree().find::<pebbles_render::RenderSpinner>().is_none(), "finish collapses the spinner");
    DONE.with(|d| *d.borrow_mut() = None);
}

#[test]
fn pull_without_reaching_the_threshold_does_not_fire() {
    REFRESHES.with(|r| r.set(0));
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(feed)).into_widget());
    ui.layout(&mut env, Size::new(300.0, 200.0));

    // A small 60px pull (banded −20 < threshold) never arms.
    assert!(ui.begin_content_drag(Offset::new(150.0, 30.0)));
    assert!(ui.update_content_drag(Offset::new(150.0, 90.0)));
    ui.end_content_drag(Offset::new(150.0, 90.0));
    assert_eq!(REFRESHES.with(|r| r.get()), 0, "a sub-threshold pull never fires");
    ui.rebuild_if_dirty();
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert!(ui.render_tree().find::<pebbles_render::RenderSpinner>().is_none(), "no spinner either");
}

/// A route flip: page 0 is the scrollable page, page 1 is "elsewhere". The blue
/// strip at the top is the nav button (mirrors the navigation.rs harness).
fn scroll_nav_root() -> impl IntoWidget {
    use pebbles_core::{action, create_signal};
    use pebbles_widgets::{Container, GestureDetector};
    let route = create_signal(0i32);
    let content = if route.get() == 0 {
        // Bounded viewport (30..330 in the window) so the content overflows it.
        Container::new().height(300.0).child(tall(false)).into_widget()
    } else {
        text("elsewhere").into_widget()
    };
    column(vec![
        GestureDetector::new(Container::new().width(140.0).height(30.0).color(palette::BLUE))
            .on_tap(action(move || route.update(|r| *r = 1 - *r)))
            .into_widget(),
        content,
    ])
}

/// REGRESSION — the "scroll, then navigate" app-killer. A wheel fling leaves a
/// spring animating; unmounting the scroll view (navigation) frees its RenderId;
/// the next frame's `tick_scrolls` used to index the freed node and panic
/// ("invalid SlotMap key used"), taking the whole app down. Same class for a
/// content drag crossing an unmount. Both must be silently dropped.
#[test]
fn springs_and_drags_survive_unmount_mid_flight() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.set_test_clock(Some(0.0));
    ui.mount_root(View::new(palette::WHITE, component(scroll_nav_root)).into_widget());
    let window = Size::new(300.0, 400.0);
    ui.layout(&mut env, window);
    let nav = Offset::new(110.0, 15.0); // hits the 140-wide strip whether start-aligned or centered
    let page = Offset::new(150.0, 200.0);
    // A full tap, the way the shell delivers one.
    fn tap(ui: &mut Ui, p: Offset) {
        ui.dispatch_pointer_down(p);
        ui.dispatch_tap(p);
        ui.dispatch_pointer_up(p);
    }

    // Wheel fling: the spring goes live…
    assert!(ui.dispatch_scroll(page, 120.0));
    // …then navigate away before it settles: the scroll view unmounts.
    tap(&mut ui, nav);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, window);
    assert!(ui.render_tree().find::<RenderScroll>().is_none(), "page 1 has no scroll view");
    assert!(!ui.tick_scrolls(0.016), "a dead spring is dropped, not ticked");

    // Back to the page, then a live content drag across the same unmount.
    tap(&mut ui, nav);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, window);
    assert!(ui.begin_content_drag(page));
    tap(&mut ui, nav);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, window);
    assert!(ui.render_tree().find::<RenderScroll>().is_none(), "unmounted again");
    assert!(!ui.update_content_drag(Offset::new(150.0, 160.0)), "stale drag is dropped");
    assert!(!ui.end_content_drag(Offset::new(150.0, 160.0)));
}
