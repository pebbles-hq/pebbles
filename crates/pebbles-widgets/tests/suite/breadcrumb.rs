//! [`Breadcrumb`]: `max_visible` collapses the middle segments into a "…" dropdown
//! that lists the hidden ones — and the trail paints.

use pebbles_core::IntoWidget;
use pebbles_foundation::{CrossAxisAlignment, Offset};
use pebbles_testing::Harness;
use pebbles_widgets::{breadcrumb, column, overlay};

fn crumbs() -> impl IntoWidget {
    column(vec![
        breadcrumb(
            ["Home", "Workspace", "Design", "Components", "Input", "Select", "pebbles.rs"]
                .into_iter()
                .map(String::from)
                .collect(),
        )
        .max_visible(4)
        .into_widget(),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Start)
}

#[test]
fn ellipsis_opens_a_menu_of_hidden_segments() {
    let mut h = Harness::new().window(500.0, 200.0);
    h.mount(crumbs);
    h.draw();

    assert!(!overlay::is_open(), "closed initially");

    // Visible trail: Home › … › Select › pebbles.rs. The "…" sits right of
    // "Home" (13px text ≈ 36px + two 6px gaps + a 14px chevron ≈ 62px in).
    h.click(Offset::new(70.0, 9.0));
    h.draw();
    assert!(overlay::is_open(), "clicking the … opens the hidden-segments menu");

    overlay::hide_overlay();
    h.draw();
    assert!(!overlay::is_open(), "and it dismisses");
}
