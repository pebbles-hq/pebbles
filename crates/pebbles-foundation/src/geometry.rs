//! Geometry primitives. Thin, ergonomic wrappers and aliases over `kurbo`.
//!
//! Pebbles speaks in Flutter's vocabulary — [`Offset`], [`Size`], [`EdgeInsets`],
//! [`Alignment`] — but the underlying math is kurbo's, so the whole 2D ecosystem
//! (paths, affines, strokes) is available without conversion.

pub use kurbo::{Affine, Point, Vec2};

/// A 2D displacement, i.e. a position relative to an origin. Flutter's `Offset`.
///
/// This is an alias of [`kurbo::Vec2`]: `dx`/`dy` map to `x`/`y`.
pub type Offset = kurbo::Vec2;

/// A width/height pair. Flutter's `Size`, backed by [`kurbo::Size`].
pub type Size = kurbo::Size;

/// An axis-aligned rectangle. Backed by [`kurbo::Rect`].
pub type Rect = kurbo::Rect;

/// Insets from the edges of a box, in logical pixels. Flutter's `EdgeInsets`.
///
/// Convertible into [`kurbo::Insets`] for use with the geometry crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeInsets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl EdgeInsets {
    pub const ZERO: EdgeInsets = EdgeInsets { left: 0.0, top: 0.0, right: 0.0, bottom: 0.0 };

    /// The same inset on all four sides.
    pub const fn all(value: f64) -> Self {
        EdgeInsets { left: value, top: value, right: value, bottom: value }
    }

    /// Symmetric horizontal / vertical insets.
    pub const fn symmetric(horizontal: f64, vertical: f64) -> Self {
        EdgeInsets { left: horizontal, top: vertical, right: horizontal, bottom: vertical }
    }

    /// Insets given per-side; omitted sides are zero.
    pub const fn only(left: f64, top: f64, right: f64, bottom: f64) -> Self {
        EdgeInsets { left, top, right, bottom }
    }

    /// Total horizontal inset (`left + right`).
    pub fn horizontal(&self) -> f64 {
        self.left + self.right
    }

    /// Total vertical inset (`top + bottom`).
    pub fn vertical(&self) -> f64 {
        self.top + self.bottom
    }

    /// The combined size consumed by these insets.
    pub fn collapsed_size(&self) -> Size {
        Size::new(self.horizontal(), self.vertical())
    }

    /// The offset of the inner (content) origin relative to the outer origin.
    pub fn top_left(&self) -> Offset {
        Offset::new(self.left, self.top)
    }
}

impl From<EdgeInsets> for kurbo::Insets {
    fn from(e: EdgeInsets) -> Self {
        kurbo::Insets::new(e.left, e.top, e.right, e.bottom)
    }
}

/// A point within a rectangle expressed in the normalized range `-1.0..=1.0`,
/// where `(-1, -1)` is the top-left, `(0, 0)` the center and `(1, 1)` the
/// bottom-right. Flutter's `Alignment`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Alignment {
    pub x: f64,
    pub y: f64,
}

impl Alignment {
    pub const TOP_LEFT: Alignment = Alignment { x: -1.0, y: -1.0 };
    pub const TOP_CENTER: Alignment = Alignment { x: 0.0, y: -1.0 };
    pub const TOP_RIGHT: Alignment = Alignment { x: 1.0, y: -1.0 };
    pub const CENTER_LEFT: Alignment = Alignment { x: -1.0, y: 0.0 };
    pub const CENTER: Alignment = Alignment { x: 0.0, y: 0.0 };
    pub const CENTER_RIGHT: Alignment = Alignment { x: 1.0, y: 0.0 };
    pub const BOTTOM_LEFT: Alignment = Alignment { x: -1.0, y: 1.0 };
    pub const BOTTOM_CENTER: Alignment = Alignment { x: 0.0, y: 1.0 };
    pub const BOTTOM_RIGHT: Alignment = Alignment { x: 1.0, y: 1.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Alignment { x, y }
    }

    /// Given an outer container of `parent` size and an inner `child` size,
    /// returns the top-left offset that positions the child per this alignment.
    pub fn inscribe(&self, child: Size, parent: Size) -> Offset {
        let free_x = (parent.width - child.width).max(0.0);
        let free_y = (parent.height - child.height).max(0.0);
        Offset::new(free_x * (self.x + 1.0) / 2.0, free_y * (self.y + 1.0) / 2.0)
    }
}
