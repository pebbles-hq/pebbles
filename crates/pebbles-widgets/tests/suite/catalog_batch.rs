//! Widget-catalog batch: the `create_timeout` one-shot delay primitive (SI-3) and the
//! AlertDialog preset (3.4). Driven headlessly through the real driver / modal machinery.

use std::cell::{Cell, RefCell};

use pebbles_core::animation;
use pebbles_core::{AnyWidget, IntoWidget, Signal, Ui, component, create_signal, create_timeout};
use pebbles_foundation::{Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{View, column, dialog, text};

thread_local! {
    static FIRED: Cell<u32> = const { Cell::new(0) };
    static SHOW: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
}

fn timer_root() -> impl IntoWidget {
    create_timeout(0.05, || FIRED.with(|c| c.set(c.get() + 1)));
    text("waiting")
}

/// A shell that mounts the timer component only while `SHOW` is true — flipping it
/// false unmounts the timer through normal reconciliation.
fn toggle_shell() -> impl IntoWidget {
    let show = SHOW.with(|c| c.borrow().expect("SHOW set before mount"));
    let kids: Vec<AnyWidget> =
        if show.get() { vec![component(timer_root).into_widget()] } else { vec![text("gone").into_widget()] };
    column(kids)
}

#[test]
fn create_timeout_fires_once_after_the_delay() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    animation::reset();
    FIRED.with(|c| c.set(0));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(timer_root)).into_widget());
    ui.layout(&mut env, Size::new(100.0, 100.0));

    // First tick anchors the deadline at now + 0.05 = 0.06.
    assert!(animation::tick(0.01), "a pending timeout keeps the driver active");
    assert_eq!(FIRED.with(Cell::get), 0, "not yet due");

    // Past the deadline → fires exactly once.
    animation::tick(0.10);
    assert_eq!(FIRED.with(Cell::get), 1, "fired at the deadline");

    // Never fires again, and the driver goes idle.
    let still = animation::tick(0.30);
    assert_eq!(FIRED.with(Cell::get), 1, "one-shot: no re-fire");
    assert!(!still, "driver idle once the timeout is spent");
}

#[test]
fn create_timeout_is_cancelled_when_its_component_unmounts() {
    pebbles_widgets::overlay::init();
    pebbles_core::focus::init();
    animation::reset();
    FIRED.with(|c| c.set(0));
    let show = create_signal(true);
    SHOW.with(|c| *c.borrow_mut() = Some(show));

    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(toggle_shell)).into_widget());
    ui.layout(&mut env, Size::new(100.0, 100.0));
    animation::tick(0.01); // deadline armed at 0.06

    // Flip SHOW → the timer component unmounts via reconciliation (cleanup removes it).
    show.set(false);
    ui.rebuild_if_dirty();
    ui.layout(&mut env, Size::new(100.0, 100.0));

    animation::tick(0.20); // well past the old deadline
    assert_eq!(FIRED.with(Cell::get), 0, "an unmounted timeout never fires");
}

#[test]
fn alert_dialog_is_non_dismissible_by_default() {
    dialog::init();

    let id = dialog::alert_dialog("Delete this?")
        .description("This cannot be undone.")
        .confirm("Delete")
        .destructive(true)
        .on_confirm(|| {})
        .open();
    assert!(dialog::is_open(), "the alert dialog opened");

    // Escape / outside-click must NOT close it (shadcn semantics: explicit choice).
    dialog::dismiss_top();
    assert!(dialog::is_open(), "non-dismissible by default");

    // An explicit close (what the Cancel/Confirm buttons call) does close it.
    dialog::close_dialog(id);
    assert!(!dialog::is_open(), "explicit close works");
}

#[test]
fn alert_dialog_can_opt_into_dismissible() {
    dialog::init();
    dialog::alert_dialog("Heads up").dismissible(true).open();
    dialog::dismiss_top();
    assert!(!dialog::is_open(), "dismissible(true) honors Escape/outside-click");
}

#[test]
fn sheet_opens_and_dismisses() {
    use pebbles_widgets::{Side, sheet};
    sheet::init();

    let id = sheet(text("filters")).side(Side::Right).size(320.0).title("Filters").open();
    assert!(sheet::is_open(), "the sheet opened");
    // Dismissible by default → Escape/scrim closes.
    sheet::dismiss_top();
    assert!(!sheet::is_open(), "dismissed");
    let _ = id;

    // Non-dismissible ignores dismiss but honors an explicit close.
    let id2 = sheet(text("x")).dismissible(false).open();
    sheet::dismiss_top();
    assert!(sheet::is_open(), "non-dismissible ignores dismiss");
    sheet::close_sheet(id2);
    assert!(!sheet::is_open(), "explicit close works");
}
