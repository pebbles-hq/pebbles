//! A selection group inside a virtualized list whose rows shift (a console / terminal
//! appending lines): reused row elements change their group index, and the old index's
//! entry must go — a drag over a row selects exactly that row, never a stale one.

use pebbles_core::IntoWidget;
use pebbles_foundation::Offset;
use pebbles_render::RenderParagraph;
use pebbles_testing::Harness;
use pebbles_widgets::{ListView, SizedBox, selection_group, span, text_rich};

thread_local! {
    static COUNT: std::cell::Cell<Option<pebbles_core::Signal<usize>>> = const { std::cell::Cell::new(None) };
    static PICKED: std::cell::RefCell<String> = const { std::cell::RefCell::new(String::new()) };
}

fn console() -> SizedBox {
    let count = pebbles_core::create_signal(20usize);
    COUNT.with(|c| c.set(Some(count)));
    let n = count.get();
    let group = selection_group(7);
    SizedBox::exact(
        300.0,
        200.0,
        ListView::builder(n, 20.0, move |i| {
            let idx = n - 1 - i;
            text_rich(vec![span(format!("line {idx}"))])
                .selectable_in(group, idx)
                .on_selection(|t| PICKED.with(|p| *p.borrow_mut() = t.to_string()))
                .into_widget()
        })
        .reverse(),
    )
}

#[test]
fn drag_selects_the_row_under_the_pointer_after_rows_shift() {
    for round in 0..8 {
        let mut h = Harness::new().window(300.0, 200.0);
        h.mount(console);
        h.frames(2);
        for _ in 0..6 {
            COUNT.with(|c| c.get()).unwrap().update(|n| *n += 3);
            h.frames(2);
        }
        let n = COUNT.with(|c| c.get()).unwrap().peek();
        let want = format!("line {}", n - 1);
        let tree = h.ui.render_tree();
        let p = tree
            .find_all::<RenderParagraph>()
            .into_iter()
            .find(|&p| tree.object_ref(p).downcast_ref::<RenderParagraph>().unwrap().text == want)
            .expect("bottom row");
        let at = tree.absolute_offset(p);
        let w = h.size_of(p).width;
        h.advance(1.0);
        h.drag(Offset::new(at.x + 1.0, at.y + 10.0), Offset::new(at.x + w - 1.0, at.y + 10.0));
        h.frames(1);
        assert_eq!(PICKED.with(|p| p.borrow().clone()), want, "round {round}");
    }
}
