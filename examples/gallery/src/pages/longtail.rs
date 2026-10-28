//! Long-tail widgets: `Placeholder` (the dev "not built yet" box) and `Banner`
//! (Flutter's MaterialBanner — a full-width message bar).

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

// ===========================================================================
// Placeholder
// ===========================================================================

pub fn placeholder_screen() -> Element {
    screen("Placeholder").body(children![
        doc("placeholder()").body(
            row(children![
                SizedBox::exact(160.0, 110.0, placeholder()),
                gap_w(20.0),
                placeholder().size(110.0, 110.0).color(theme().colors.primary),
                gap_w(20.0),
                placeholder().size(140.0, 80.0).stroke_width(4.0).color(palette::violet::S500),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .main_axis_size(MainAxisSize::Min)
        )
    ])
}

// ===========================================================================
// Banner
// ===========================================================================

pub fn banner_screen() -> Element {
    screen("Banner").body(children![
        doc("banner(message).icon(..).action(..)").body(
            column(children![
                banner("Your trial ends in 3 days.")
                    .icon(IconKind::Info)
                    .action(button("Dismiss").variant(ButtonVariant::Ghost))
                    .action(button("Upgrade")),
                gap_h(16.0),
                banner("We couldn't reach the server. Retrying…")
                    .icon(IconKind::Warning)
                    .action(button("Retry").variant(ButtonVariant::Outline)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)
        )
    ])
}
