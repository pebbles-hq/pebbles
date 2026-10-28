//! C8g — the app-shell chrome: Scaffold arranging a TopPanel, a collapsible SideNav
//! and a BottomNav around a body. Each piece is standalone.

use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn chrome_screen() -> Element {
    let sel = create_signal(0usize);
    let collapsed = create_signal(false);

    screen("App Shell — Scaffold").body(children![
        doc("A bounded mini-app").body(
            container().height(380.0).child(
                scaffold(center(text("Body content").size(16.0)))
                    .top(top_panel("Dashboard").action(button("Action")))
                    .side(
                        side_nav()
                            .width(200.0)
                            .collapsible(true)
                            .collapsed(collapsed.get())
                            .on_collapse_changed(move |c| collapsed.set(c))
                            .item(
                                nav_item("Home")
                                    .icon(tabler::LAYOUT_DASHBOARD)
                                    .selected(sel.get() == 0)
                                    .on_select(move || sel.set(0)),
                            )
                            .item(
                                nav_item("Files")
                                    .icon(tabler::FOLDERS)
                                    .selected(sel.get() == 1)
                                    .on_select(move || sel.set(1)),
                            )
                            .item(
                                nav_item("Colors")
                                    .icon(tabler::PALETTE)
                                    .selected(sel.get() == 2)
                                    .on_select(move || sel.set(2)),
                            ),
                    )
                    .bottom(
                        bottom_nav()
                            .item(
                                bottom_nav_item(tabler::LAYOUT_DASHBOARD, "Home")
                                    .selected(sel.get() == 0)
                                    .on_select(move || sel.set(0)),
                            )
                            .item(
                                bottom_nav_item(tabler::FOLDERS, "Files")
                                    .selected(sel.get() == 1)
                                    .on_select(move || sel.set(1)),
                            )
                            .item(
                                bottom_nav_item(tabler::PALETTE, "Colors")
                                    .selected(sel.get() == 2)
                                    .on_select(move || sel.set(2)),
                            ),
                    ),
            ),
        ),
    ])
}
