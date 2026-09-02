//! C8h — routing: RouteView renders only the active route's builder; NavStack is a
//! push/pop history you keep in a signal.

use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn routing_screen() -> Element {
    // One history drives both demos (the current route + the push/pop stack).
    let stack = create_signal(NavStack::new("inbox"));
    let cur = stack.get().current().to_string();
    let depth = stack.get().depth();

    screen("Routing — NavStack & RouteView")
        .description(
            "RouteView renders only the active route's builder, so inactive pages are never \
             constructed. NavStack is a Clone + Default history kept in a signal.",
        )
        .body(children![
            doc("RouteView")
                .description("Switch the visible page — only the matching route builds.")
                .body(column(children![
                    row(children![
                        button("Inbox").on_pressed(move || stack.update(|s| s.replace("inbox"))),
                        gap_w(8.0),
                        button("Sent").on_pressed(move || stack.update(|s| s.replace("sent"))),
                    ]),
                    gap_h(12.0),
                    Container::new().height(60.0).child(
                        route_view(cur.clone())
                            .route("inbox", || text("Inbox — 3 unread").size(15.0).into_widget())
                            .route("sent", || text("Sent — 128 messages").size(15.0).into_widget())
                            .fallback(|| text("Not found").into_widget()),
                    ),
                ])),
            doc("NavStack")
                .description("A push/pop history with a live depth and a Back button.")
                .body(column(children![
                    text(format!("current: {cur} · depth {depth}")).size(14.0),
                    gap_h(10.0),
                    row(children![
                        button("Push settings").on_pressed(move || stack.update(|s| s.push("settings"))),
                        gap_w(8.0),
                        button("Back").on_pressed(move || {
                            stack.update(|s| {
                                s.pop();
                            })
                        }),
                    ]),
                ])),
        ])
}
