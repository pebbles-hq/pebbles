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

/// How children are distributed within a wrap's run (main axis), or how runs are
/// distributed along the cross axis — Flutter's `WrapAlignment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WrapAlignment {
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
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

/// How a child is scaled to fit (or cover) a box of a different size — Flutter's
/// `BoxFit`. A `FittedBox` uses this to scale a child laid out at its natural
/// size into the box it is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoxFit {
    /// Scale down (never up) to fit entirely inside the box, preserving aspect.
    /// A child smaller than the box stays at natural size.
    #[default]
    Contain,
    /// Scale to cover the whole box, preserving aspect (cropping overflow).
    Cover,
    /// Stretch to exactly fill the box (distorting aspect when they differ).
    Fill,
    /// No scaling — the child keeps its natural size, positioned by alignment.
    None,
    /// Scale to match the box's width; height follows the child's aspect ratio.
    FitWidth,
    /// Scale to match the box's height; width follows the child's aspect ratio.
    FitHeight,
    /// Like [`Contain`](BoxFit::Contain) but never scales UP — `min(1, contain)`.
    ScaleDown,
}
