use pebbles::prelude::*;

use crate::ui::{doc, gap_w, screen};

pub fn collapsibles() -> Element {
    screen(
        "Collapsible",
        "A single open/closed section (shadcn's Collapsible). It manages its own state — tap the header to toggle — and takes a custom trigger for anything fancier.",
        children![basic(), open_default(), with_callback(), custom_trigger()],
    )
}

fn panel(child: impl IntoWidget) -> impl IntoWidget {
    Container::new().width(380.0).child(Card::new(child).padding(EdgeInsets::all(6.0)))
}

fn basic() -> impl IntoWidget {
    doc(
        "Basic",
        "collapsible(title, content) starts closed and toggles itself on tap.",
        panel(collapsible(
            "Toggle details",
            body("Here are the details that were hidden until you opened this section."),
        )),
    )
}

fn open_default() -> impl IntoWidget {
    doc(
        "Open by default",
        "Seed the initial state with .open(true).",
        panel(collapsible("What's included", muted("Everything in Pro, plus priority support and SSO."))
            .open(true)),
    )
}

fn with_callback() -> impl IntoWidget {
    let note = create_signal(String::from("closed"));
    doc(
        "On toggle",
        "React to open/close with .on_toggle(|open| …).",
        column(children![
            panel(
                collapsible("Advanced settings", body("Flags, experiments and other advanced options."))
                    .on_toggle(move |open| note.set(if open { "open".into() } else { "closed".into() })),
            ),
            SizedBox::spacer(0.0, 8.0),
            muted(format!("state: {}", note.get())),
        ])
        .start()
        .min(),
    )
}

fn custom_trigger() -> impl IntoWidget {
    let repos =
        column(children![repo_row("@pebbles/foundation"), repo_row("@pebbles/render"), repo_row("@pebbles/widgets")])
            .start()
            .min()
            .spacing(6.0);

    doc(
        "Custom trigger",
        "Replace the whole header with .trigger(widget) — here a starred-repositories header à la shadcn.",
        panel(
            collapsible("", repos)
                .open(true)
                .trigger(
                    Container::new().padding(EdgeInsets::symmetric(8.0, 6.0)).child(
                        row(children![
                            text("★ 3 starred repositories").size(14.0).weight(500.0),
                            spacer(),
                            icon(IconKind::ChevronDown).size(16.0).color(theme().colors.muted_foreground),
                        ]),
                    ),
                ),
        ),
    )
}

fn repo_row(name: &str) -> impl IntoWidget {
    Container::new()
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .padding(EdgeInsets::symmetric(12.0, 10.0))
        .child(row(children![icon(IconKind::Circle).size(14.0), gap_w(10.0), text(name.to_string()).size(13.5)]).min())
}
