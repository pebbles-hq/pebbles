//! A "stylesheet" — just a module of functions returning [`Style`] values. Organize
//! these however you like (one file, many files); apply them anywhere with
//! `.styled(..)` (box props) or `text(..).style(..)` (text + box props).

use pebbles::prelude::*;

pub fn card() -> Style {
    let c = theme().colors;
    style()
        .background(c.card)
        .padding_all(16.0)
        .radius_all(12.0)
        .border(Border::new(c.border, 1.0))
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 18), Offset::new(0.0, 2.0), 8.0, 0.0))
}

pub fn pill() -> Style {
    style().background(theme().colors.secondary).padding_xy(14.0, 6.0).radius_all(999.0)
}

pub fn heading() -> Style {
    style().color(theme().colors.foreground).font_size(28.0).bold()
}

pub fn muted_panel() -> Style {
    style().background(theme().colors.muted).padding_all(20.0).radius_all(10.0)
}

/// Overrides that turn any surface "dangerous" — layer it with `.merge`.
pub fn danger() -> Style {
    let c = theme().colors;
    style().background(c.destructive).color(c.destructive_foreground)
}
