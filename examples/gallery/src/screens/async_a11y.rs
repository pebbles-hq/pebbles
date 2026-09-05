//! Async & accessibility long-tail — `stream_builder` (Flutter's StreamBuilder, over
//! the reactive `Channel`) and the semantics combinators `merge_semantics` /
//! `exclude_semantics` / `block_semantics`. The combinators are screen-reader-only —
//! they change nothing visible — so they're shown wrapping ordinary content with a note.

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

fn framed(child: impl IntoWidget) -> impl IntoWidget {
    Container::new()
        .decoration(
            BoxDecoration::new()
                .color(theme().colors.muted)
                .radius(BorderRadius::all(10.0))
                .border(Border::all(BorderSide::new(theme().colors.border, 1.0))),
        )
        .padding(EdgeInsets::all(16.0))
        .child(child)
}

// ===========================================================================
// StreamBuilder
// ===========================================================================

pub fn stream_builder_screen() -> Element {
    // A stable channel + counter held across renders.
    let ch = create_signal(channel::<u32>());
    let n = create_signal(0u32);

    screen("Stream Builder")
        .description("Rebuild when a Channel emits. Flutter's StreamBuilder — a thin reactive builder over Pebbles' Channel (no component boilerplate).")
        .body(children![
            doc("stream_builder(channel, |latest| ..)")
                .description("Each send re-renders the builder with the latest value — None before the first message.")
                .body(column(children![
                    row(children![
                        button("Send a tick").on_pressed(move || {
                            n.update(|v| *v += 1);
                            ch.peek().send(n.peek());
                        }),
                        gap_w(10.0),
                        button("Reset").variant(ButtonVariant::Outline).on_pressed(move || {
                            n.set(0);
                            ch.peek().send(0);
                        }),
                    ])
                    .main_axis_size(MainAxisSize::Min),
                    gap_h(16.0),
                    framed(stream_builder(ch.get(), |v| {
                        let label = match v {
                            Some(x) => format!("latest message: {x}"),
                            None => "waiting for the first message…".to_string(),
                        };
                        text(&label).size(15.0).color(theme().colors.foreground)
                    })),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))
        ])
}

// ===========================================================================
// Semantics combinators
// ===========================================================================

fn caption(s: &str) -> impl IntoWidget {
    text(s).size(12.5).color(theme().colors.muted_foreground)
}

pub fn semantics_combinators_screen() -> Element {
    screen("Semantics Combinators")
        .description("Reshape how a subtree appears to a screen reader — merge_semantics / exclude_semantics / block_semantics (Flutter's MergeSemantics / ExcludeSemantics / BlockSemantics). These are ACCESSIBILITY-ONLY: they change nothing visible, so each is shown wrapping ordinary content. Their effect is verified by headless tests against the semantics tree.")
        .body(children![
            doc("merge_semantics(child)")
                .description("Collapses a subtree into ONE announcement — a screen reader reads the group as a single item instead of each label separately.")
                .body(framed(merge_semantics(
                    row(children![
                        icon(IconKind::Info).size(16.0).color(theme().colors.primary),
                        gap_w(8.0),
                        text("Storage").size(14.0).semibold(),
                        gap_w(8.0),
                        caption("42 GB of 128 GB used"),
                    ])
                    .main_axis_size(MainAxisSize::Min)
                    .cross_axis_alignment(CrossAxisAlignment::Center),
                ))),
            doc("exclude_semantics(child)")
                .description("Hides a decorative subtree from the accessibility tree entirely (still painted). Use it for redundant or purely visual elements.")
                .body(framed(column(children![
                    exclude_semantics(caption("★ ★ ★ ★ ☆  (decorative rating — hidden from AT)")),
                    gap_h(6.0),
                    text("Rated 4 out of 5").size(13.0).color(theme().colors.foreground),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))),
            doc("block_semantics(child)")
                .description("Drops the semantics of everything painted below it in the same parent — a modal barrier, so a screen reader can't reach the content behind a dialog.")
                .body(framed(column(children![
                    caption("A barrier over its scrim — only the barrier's own content is reachable to AT."),
                    gap_h(8.0),
                    block_semantics(text("Dialog content (reachable)").size(14.0).semibold()),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min))),
        ])
}
