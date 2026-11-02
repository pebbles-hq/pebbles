//! Touchpad scrolling through scroll views and virtualized lists: a stream of small
//! pixel deltas (a swipe + OS kinetic momentum) or fractional line deltas (X11 smooth
//! scrolling) accumulates 1:1 and never snaps back toward the top.

use pebbles_core::IntoWidget;
use pebbles_foundation::Offset;
use pebbles_render::{RenderList, RenderScroll, ScrollUnit};
use pebbles_testing::Harness;
use pebbles_widgets::{ListView, SizedBox, column, gap_h, scroll_view, text};

fn stream(h: &mut Harness, read: impl Fn(&Harness) -> f64, unit_lines: bool) -> (f64, f64) {
    let at = Offset::new(100.0, 100.0);
    let mut sent = 0.0;
    let mut prev = read(h);
    let mut v = 8.0;
    for i in 0..80 {
        if i > 40 {
            v *= 0.93; // momentum tail
        }
        let unit = if unit_lines { ScrollUnit::Lines(Offset::new(0.0, v / 20.0)) } else { ScrollUnit::Pixels };
        h.wheel(at, Offset::new(0.0, v), unit);
        sent += v;
        h.settle();
        let now = read(h);
        assert!(now >= prev - 0.01, "snapped back at {i}: {prev} -> {now}");
        prev = now;
    }
    (sent, read(h))
}

fn scroll_offset(h: &Harness) -> f64 {
    let id = h.find_all::<RenderScroll>()[0];
    h.object::<RenderScroll>(id).offset
}

fn list_offset(h: &Harness) -> f64 {
    let id = h.find_all::<RenderList>()[0];
    h.object::<RenderList>(id).offset
}

#[test]
fn scroll_view_follows_touchpad_pixels_one_to_one() {
    for lines in [false, true] {
        let mut h = Harness::new().window(200.0, 200.0);
        let rows: Vec<_> = (0..200).map(|_| gap_h(30.0).into_widget()).collect();
        h.mount_widget(SizedBox::exact(200.0, 200.0, scroll_view(column(rows))));
        h.frame();
        let (sent, got) = stream(&mut h, scroll_offset, lines);
        assert!((got - sent).abs() < 1.0, "lines={lines}: sent {sent}, scrolled {got}");
    }
}

#[test]
fn virtualized_list_follows_touchpad_pixels_one_to_one() {
    for lines in [false, true] {
        let mut h = Harness::new().window(200.0, 200.0);
        h.mount_widget(SizedBox::exact(200.0, 200.0, ListView::builder(500, 28.0, |i| text(format!("row {i}")))));
        h.frames(2);
        let (sent, got) = stream(&mut h, list_offset, lines);
        assert!((got - sent).abs() < 1.0, "lines={lines}: sent {sent}, scrolled {got}");
    }
}
