//! A toast with a long description (a CLI error) keeps its action and close button inside
//! the card and the window, and the action is clickable through real hit-testing.

use std::cell::Cell;
use std::rc::Rc;

use pebbles_core::IntoWidget;
use pebbles_foundation::Offset;
use pebbles_render::RenderParagraph;
use pebbles_testing::Harness;
use pebbles_widgets::{OverlayHost, text, toast};

#[test]
fn long_description_wraps_and_the_action_stays_reachable() {
    let mut h = Harness::new().window(800.0, 600.0);
    h.mount(|| OverlayHost::wrap(text("app")).into_widget());
    h.draw();
    let hit = Rc::new(Cell::new(false));
    let hit2 = hit.clone();
    toast("Docker")
        .description(
            "Push nginx:latest failed: denied: requested access to the resource is denied \
             because the repository does not exist or the login has no push permission",
        )
        .action("Open Docker Log", move || hit2.set(true))
        .duration(0.0)
        .show();
    h.frames(30);

    let id = *h.find_all_text("Open Docker Log").first().expect("the action renders");
    let o = h.ui.render_tree().absolute_offset(id);
    let s = h.size_of(id);
    assert!(o.x >= 0.0 && o.x + s.width <= 800.0 - 16.0, "action inside the window: x={} w={}", o.x, s.width);
    assert!(o.y + s.height <= 600.0, "action inside the window vertically");

    // The description wrapped instead of running past the card.
    let desc = h
        .find_all::<RenderParagraph>()
        .into_iter()
        .find(|&p| {
            let x = h.ui.render_tree().absolute_offset(p).x;
            // In the card (bottom-right), left of the action, and taller than one line.
            x > 400.0 && x < o.x && h.size_of(p).height > 20.0 && h.size_of(p).height < 200.0
        })
        .expect("the description wraps onto more than one line");
    let d = h.ui.render_tree().absolute_offset(desc);
    assert!(
        d.x + h.size_of(desc).width <= o.x,
        "description ends before the action: desc x={} w={} h={}; action x={}",
        d.x,
        h.size_of(desc).width,
        h.size_of(desc).height,
        o.x
    );

    let at = Offset::new(o.x + s.width / 2.0, o.y + s.height / 2.0);
    assert!(h.ui.tap_target_at(at).is_some(), "the action is hit-testable");
    h.click(at);
    h.draw();
    assert!(hit.get(), "clicking the action runs it");
}
