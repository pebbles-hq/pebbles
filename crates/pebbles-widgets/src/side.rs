//! [`Side`] — a shared edge/placement vocabulary. One enum used across the catalog:
//! [`Sheet`](crate::Sheet) anchors to a `Side`, [`tooltip`](crate::components::tooltip)
//! places its chip on a `Side`, and popovers/hover cards flip against it. Defined once
//! here (rule 0.1: one type, no per-module clones).

/// An edge or placement direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    /// The opposite side (used when flipping a tooltip/popover that would exit the
    /// window on this side).
    pub fn flip(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
        }
    }
}
