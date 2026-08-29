use pebbles::prelude::*;

use crate::styles;
use crate::ui::{gap_h, hstack, screen, section, vstack};

pub fn styling() -> impl IntoWidget {
    screen(
        "Styling",
        "One general Style value, applied to any widget — React-Native / CSS-like.",
        children![
            section(
                "SAME STYLE, ANY WIDGET — .styled(style) applies box properties",
                vstack(
                    children![
                        // A pill style on a Text...
                        text("styled Text").styled(styles::pill()),
                        gap_h(10.0),
                        // ...the same pill on a Row of icons...
                        hstack(children![icon(IconKind::Star).size(18.0), icon(IconKind::Check).size(18.0)], 8.0)
                            .styled(styles::pill()),
                        gap_h(10.0),
                        // ...and a card style on a Column.
                        column(children![title("Card via style()"), muted("background + border + radius + shadow")])
                            .cross_axis_alignment(CrossAxisAlignment::Start)
                            .main_axis_min()
                            .styled(styles::card()),
                    ],
                    0.0,
                ),
            ),
            section(
                "TEXT STYLE — text(..).style(style) applies text + box properties",
                text("Heading rendered from a global style").style(styles::heading()),
            ),
            section(
                "MERGE — layer styles, later wins (like stacking CSS classes)",
                text("card().merge(danger())")
                    .style(styles::card().merge(styles::danger()).padding_all(16.0)),
            ),
            section(
                "MUTED PANEL — reused anywhere",
                column(children![
                    text("This whole panel is one reusable style."),
                    gap_h(8.0),
                    muted("styles::muted_panel()"),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_min()
                .styled(styles::muted_panel()),
            ),
        ],
    )
}
