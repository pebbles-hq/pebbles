//! Rapid-resize + event-flood robustness: the frame pipeline survives a storm of
//! window resizes — including tiny, huge, 0×0, and extreme aspect ratios — each
//! interleaved with pointer and keyboard events, without panicking, and settles at
//! the end. (A storm test previously existed only in the markdown/code-editor repos.)

use pebbles_core::{IntoWidget, KeyInput};
use pebbles_foundation::Offset;
use pebbles_testing::Harness;
use pebbles_widgets::{ListView, button, column, container, text, text_field};

fn busy() -> impl IntoWidget {
    column(vec![
        text("Header").into_widget(),
        text_field().label("Name").into_widget(),
        button("Save").into_widget(),
        container()
            .height(300.0)
            .child(ListView::builder(200, 32.0, |i| text(format!("row {i}"))))
            .into_widget(),
    ])
}

#[test]
fn rapid_resize_and_event_flood_never_panics() {
    let mut h = Harness::new().window(400.0, 400.0);
    h.mount(busy);
    h.draw();

    // Absurd geometry incl. degenerate (0×0, 1px), huge (4K, 10k²), and extreme
    // aspect ratios — the exact inputs a real window drag / mixed-DPI move produces.
    let sizes = [
        (1.0, 1.0),
        (2000.0, 30.0),
        (30.0, 2000.0),
        (1.0, 900.0),
        (3840.0, 2160.0),
        (0.0, 0.0),
        (10000.0, 10000.0),
        (375.0, 812.0),
        (100.0, 100.0),
    ];
    for _ in 0..5 {
        for &(w, ht) in &sizes {
            h.resize(w, ht);
            h.click(Offset::new(20.0, 20.0));
            h.key(KeyInput::Insert("x".to_string()));
        }
    }

    // Back to a sane size and let it settle — the tree must still build.
    h.resize(400.0, 400.0);
    h.settle();
    assert!(h.render_node_count() > 0, "the tree survived the resize/event storm and still builds");
}
