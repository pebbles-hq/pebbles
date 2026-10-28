//! OS file drag-and-drop (`file_drop_target`), driven the way the shell drives it
//! from winit's `HoveredFile` / `DroppedFile` / `HoveredFileCancelled`: enter/exit
//! follow the pointer between targets, a multi-file drop reaches `on_drop` once,
//! cancel fires exit, and a lone target catches drops anywhere in the window.

use std::cell::RefCell;
use std::path::PathBuf;

use pebbles_core::widget::AnyWidget;
use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Offset, Size, palette};
use pebbles_render::TextEnv;
use pebbles_widgets::{SizedBox, View, file_drop, file_drop_target, row};

thread_local! {
    static LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn log(s: String) {
    LOG.with(|l| l.borrow_mut().push(s));
}

fn take_log() -> Vec<String> {
    LOG.with(|l| std::mem::take(&mut *l.borrow_mut()))
}

fn zone(name: &'static str) -> impl IntoWidget {
    file_drop_target(move |hovering| {
        if hovering {
            log(format!("{name}:hovering"));
        }
        SizedBox::exact(200.0, 100.0, row(Vec::<AnyWidget>::new()))
    })
    .on_drag_entered(move |p| log(format!("{name}:enter:{}", p.len())))
    .on_drag_exited(move || log(format!("{name}:exit")))
    .on_drop(move |p| {
        let names: Vec<_> = p.iter().map(|p| p.display().to_string()).collect();
        log(format!("{name}:drop:{}", names.join(",")));
    })
}

fn two() -> impl IntoWidget {
    row(vec![zone("left").into_widget(), zone("right").into_widget()])
}

fn one() -> impl IntoWidget {
    row(vec![zone("only").into_widget()])
}

fn mount<W: IntoWidget + 'static>(root: fn() -> W) -> (Ui, TextEnv) {
    let mut ui = Ui::new();
    let mut text = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(root)).into_widget());
    frame(&mut ui, &mut text);
    frame(&mut ui, &mut text); // second pass: targets have their laid-out bounds
    (ui, text)
}

fn frame(ui: &mut Ui, text: &mut TextEnv) {
    ui.rebuild_if_dirty();
    ui.layout(text, Size::new(800.0, 600.0));
    ui.publish_bounds();
}

#[test]
fn file_drag_enters_moves_and_drops_once_per_batch() {
    let (mut ui, mut text) = mount(two);
    take_log();

    // First file of a two-file drag arrives over the left zone.
    assert!(file_drop::hover(PathBuf::from("/tmp/a.txt"), Offset::new(50.0, 300.0)));
    assert_eq!(take_log(), ["left:enter:1"]);
    frame(&mut ui, &mut text);
    assert_eq!(take_log(), ["left:hovering"], "the builder sees hovering = true");

    // Second file of the same drag: no second enter.
    file_drop::hover(PathBuf::from("/tmp/b.txt"), Offset::new(50.0, 300.0));
    assert_eq!(take_log(), Vec::<String>::new());
    assert_eq!(file_drop::hovered_paths().len(), 2);

    // Pointer crosses to the right zone.
    assert!(file_drop::move_to(Offset::new(300.0, 300.0)));
    assert_eq!(take_log(), ["left:exit", "right:enter:2"]);

    // Drop: one DroppedFile per file, then the shell flushes once.
    file_drop::drop_file(PathBuf::from("/tmp/a.txt"), Offset::new(300.0, 300.0));
    file_drop::drop_file(PathBuf::from("/tmp/b.txt"), Offset::new(300.0, 300.0));
    assert!(take_log().is_empty(), "nothing delivered before the flush");
    assert!(file_drop::flush());
    assert_eq!(take_log(), ["right:drop:/tmp/a.txt,/tmp/b.txt"]);
    assert!(file_drop::hovered_paths().is_empty(), "the drag session ends on drop");
    assert!(!file_drop::flush(), "a flush with nothing queued is a no-op");

    frame(&mut ui, &mut text);
    assert!(take_log().is_empty(), "no zone highlights after the drop");
}

#[test]
fn cancelled_file_drag_fires_exit() {
    let (_ui, _text) = mount(two);
    take_log();
    file_drop::hover(PathBuf::from("/tmp/a.txt"), Offset::new(300.0, 300.0));
    assert!(file_drop::cancel());
    assert_eq!(take_log(), ["right:enter:1", "right:exit"]);
    assert!(!file_drop::cancel(), "nothing to cancel twice");
}

#[test]
fn lone_target_catches_drop_outside_its_rect() {
    // winit/X11 sends no pointer motion during an XDND drag, so the pointer can be
    // stale; a single drop zone acts window-wide.
    let (_ui, _text) = mount(one);
    take_log();
    file_drop::hover(PathBuf::from("/tmp/x.png"), Offset::new(700.0, 550.0));
    file_drop::drop_file(PathBuf::from("/tmp/x.png"), Offset::new(700.0, 550.0));
    file_drop::flush();
    assert_eq!(take_log(), ["only:enter:1", "only:drop:/tmp/x.png"]);
}
