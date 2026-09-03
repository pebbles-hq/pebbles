//! Regression: navigating between screens (unmounting a page whose widgets are
//! mid-animation / were hovered) must never crash. Reproduces "the app crashes
//! when I navigate to the toggles screen" by flipping a route signal while the
//! toggle thumbs animate, running a full frame after each event like the shell.

use pebbles_core::{IntoWidget, KeyInput, Ui, action, component, create_signal};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{Container, GestureDetector, SingleChildScrollView, View, checkbox, column, gap_h, radio, switch, text, text_field};

fn toggles_page() -> impl IntoWidget {
    let a = create_signal(true);
    let b = create_signal(false);
    let c = create_signal(true);
    let mut kids: Vec<pebbles_core::AnyWidget> = vec![
        // A focusable editor at the top — focusing it then navigating away unmounts
        // the focused editor mid-session.
        text_field().placeholder("type here").width(200.0).into_widget(),
        switch(a.get()).on_changed(action(move || a.update(|v| *v = !*v))).into_widget(),
        checkbox(b.get()).on_changed(action(move || b.update(|v| *v = !*v))).into_widget(),
        radio(c.get()).on_selected(action(move || c.update(|v| *v = !*v))).into_widget(),
    ];
    // Pad it out so the content overflows and the scroll view actually scrolls,
    // exactly like a real (tall) screen wrapped by `screen()`.
    for _ in 0..40 {
        kids.push(gap_h(30.0).into_widget());
    }
    // Every gallery screen is wrapped in a SingleChildScrollView.
    SingleChildScrollView::vertical(column(kids).spacing(10.0))
}

fn nav_root() -> impl IntoWidget {
    let route = create_signal(0i32);
    let content = if route.get() == 0 {
        component(toggles_page).into_widget()
    } else {
        text("other screen").into_widget()
    };
    column(vec![
        // A nav "button" at the top: tapping it switches screens.
        GestureDetector::new(Container::new().width(140.0).height(30.0).color(palette::BLUE))
            .on_tap(action(move || route.update(|r| *r = 1 - *r)))
            .into_widget(),
        content,
    ])
}

#[test]
fn navigating_between_screens_never_crashes() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();

    let mut ui = Ui::new();
    let mut text_env = TextEnv::new();
    let window = Size::new(400.0, 500.0);
    ui.mount_root(View::new(palette::WHITE, component(nav_root)).into_widget());
    ui.layout(&mut text_env, window);

    let nav = Offset::new(70.0, 15.0);
    let input_pt = Offset::new(60.0, 50.0); // the text field, just below the nav bar
    let toggle_pts =
        [Offset::new(20.0, 80.0), Offset::new(20.0, 105.0), Offset::new(20.0, 130.0)];

    let mut now = 0.0_f64;
    // A frame does what the shell does every tick, in the shell's order: advance
    // animations + scroll springs, reconcile, relayout, then PAINT (into a CPU-side
    // vello Scene — a paint-time crash would surface here).
    let mut frame = |ui: &mut Ui, now: &mut f64| {
        *now += 0.016;
        pebbles_core::animation::tick(*now);
        ui.tick_scrolls(0.016);
        ui.rebuild_if_dirty();
        ui.layout(&mut text_env, window);
        let mut scene = pebbles_render::Scene::new();
        ui.paint(&mut scene);
    };

    for _ in 0..8 {
        // Focus the text field and type into it (registers it as the focused editor).
        ui.dispatch_pointer_down(input_pt);
        ui.dispatch_tap(input_pt);
        ui.dispatch_pointer_up(input_pt);
        frame(&mut ui, &mut now);
        ui.dispatch_key(KeyInput::Insert("hello".to_string()));
        frame(&mut ui, &mut now);

        // Hover + flip the toggles so their thumbs start animating.
        for p in toggle_pts {
            ui.dispatch_hover(p);
            frame(&mut ui, &mut now);
            ui.dispatch_pointer_down(p);
            ui.dispatch_tap(p);
            ui.dispatch_pointer_up(p);
            frame(&mut ui, &mut now);
        }
        // Fling the page so a scroll spring is in flight…
        ui.dispatch_scroll(Offset::new(20.0, 200.0), 400.0);
        frame(&mut ui, &mut now);
        // …then navigate away MID-scroll + MID-animation, unmounting the scroll view,
        // the animating toggles AND the focused editor.
        ui.dispatch_tap(nav);
        // Several frames: scroll spring / animation ticks fire after the subtree was
        // freed. Then keystrokes route to the (now unmounted) focused editor.
        for _ in 0..3 {
            frame(&mut ui, &mut now);
        }
        ui.dispatch_key(KeyInput::Insert("x".to_string()));
        ui.dispatch_key(KeyInput::Backspace);
        frame(&mut ui, &mut now);
        // Hover where a toggle used to be, then navigate back.
        ui.dispatch_hover(toggle_pts[0]);
        frame(&mut ui, &mut now);
        ui.dispatch_tap(nav);
        frame(&mut ui, &mut now);
    }
}
