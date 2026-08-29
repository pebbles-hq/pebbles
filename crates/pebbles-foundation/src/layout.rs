//! The Flutter layout vocabulary: axes, alignments and flex behavior.

/// The two directions a box can lay children out along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    /// The axis perpendicular to this one.
    pub fn flip(self) -> Axis {
        match self {
            Axis::Horizontal => Axis::Vertical,
            Axis::Vertical => Axis::Horizontal,
        }
    }
}

/// How children are placed along the main axis of a flex container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAxisAlignment {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// How children are placed along the cross axis of a flex container.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAxisAlignment {
    Start,
    End,
    Center,
    /// Stretch children to fill the cross axis.
    Stretch,
    /// Align text baselines (falls back to `Start` when no baseline is available).
    Baseline,
}

/// Whether a flex container shrink-wraps its children or expands to fill the
/// available space along the main axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainAxisSize {
    Min,
    Max,
}

/// How a flexible child is allowed to size itself within its flex allotment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexFit {
    /// The child must exactly fill its allotment (`Expanded`).
    Tight,
    /// The child may be smaller than its allotment (`Flexible`).
    Loose,
}

/// The reading/stacking direction along the horizontal axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextDirection {
    Ltr,
    Rtl,
}

/// The stacking direction along the vertical axis (which end is "start").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerticalDirection {
    Up,
    Down,
}

/// Horizontal alignment of text within its line box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,
}

/// A horizontal line used for vertically aligning text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBaseline {
    Alphabetic,
    Ideographic,
}
