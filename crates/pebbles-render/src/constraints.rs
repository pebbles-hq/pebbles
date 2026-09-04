//! [`BoxConstraints`] — the immutable size bounds a parent imposes on a child.
//!
//! Pebbles uses Flutter's box layout protocol verbatim: **constraints go down,
//! sizes come up, the parent sets the position**. A parent hands each child a
//! `BoxConstraints`; the child returns a [`Size`] that honors it; the parent then
//! decides where to place the child.

use pebbles_foundation::{EdgeInsets, Size};

/// Immutable min/max bounds on a box's width and height, in logical pixels.
///
/// A constraint is *tight* on an axis when `min == max` (the child has no choice),
/// and *loose* when `min == 0`. `max` may be [`f64::INFINITY`] (unbounded), which
/// means "size yourself to your content".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoxConstraints {
    pub min_width: f64,
    pub max_width: f64,
    pub min_height: f64,
    pub max_height: f64,
}

impl BoxConstraints {
    /// Constraints allowing any size from zero up to (and including) infinity.
    pub const UNBOUNDED: BoxConstraints = BoxConstraints {
        min_width: 0.0,
        max_width: f64::INFINITY,
        min_height: 0.0,
        max_height: f64::INFINITY,
    };

    /// Constraints that require exactly `size`.
    pub fn tight(size: Size) -> Self {
        BoxConstraints {
            min_width: size.width,
            max_width: size.width,
            min_height: size.height,
            max_height: size.height,
        }
    }

    /// Constraints requiring exactly the given width and height.
    pub fn tight_for(width: f64, height: f64) -> Self {
        BoxConstraints { min_width: width, max_width: width, min_height: height, max_height: height }
    }

    /// Constraints from zero up to `size` on each axis.
    pub fn loose(size: Size) -> Self {
        BoxConstraints { min_width: 0.0, max_width: size.width, min_height: 0.0, max_height: size.height }
    }

    /// Loosens `self` by setting both minimums to zero, keeping the maximums.
    pub fn loosen(&self) -> Self {
        BoxConstraints {
            min_width: 0.0,
            max_width: self.max_width,
            min_height: 0.0,
            max_height: self.max_height,
        }
    }

    /// The largest [`Size`] permitted. When an axis is unbounded this is infinite.
    pub fn biggest(&self) -> Size {
        Size::new(self.max_width, self.max_height)
    }

    /// The smallest [`Size`] permitted (the two minimums).
    pub fn smallest(&self) -> Size {
        Size::new(self.min_width, self.min_height)
    }

    pub fn has_bounded_width(&self) -> bool {
        self.max_width.is_finite()
    }

    pub fn has_bounded_height(&self) -> bool {
        self.max_height.is_finite()
    }

    pub fn has_tight_width(&self) -> bool {
        self.min_width >= self.max_width
    }

    pub fn has_tight_height(&self) -> bool {
        self.min_height >= self.max_height
    }

    pub fn is_tight(&self) -> bool {
        self.has_tight_width() && self.has_tight_height()
    }

    /// Clamp a single width into `[min_width, max_width]`.
    pub fn constrain_width(&self, width: f64) -> f64 {
        width.clamp(self.min_width, self.max_width)
    }

    /// Clamp a single height into `[min_height, max_height]`.
    pub fn constrain_height(&self, height: f64) -> f64 {
        height.clamp(self.min_height, self.max_height)
    }

    /// Return the closest [`Size`] to `size` that satisfies these constraints.
    pub fn constrain(&self, size: Size) -> Size {
        Size::new(self.constrain_width(size.width), self.constrain_height(size.height))
    }

    /// Shrink the constraints by `insets` on all sides (minimums floored at zero,
    /// maximums reduced but never below zero). Used by padding-like render objects.
    pub fn deflate(&self, insets: EdgeInsets) -> Self {
        let h = insets.horizontal();
        let v = insets.vertical();
        let min_w = (self.min_width - h).max(0.0);
        let min_h = (self.min_height - v).max(0.0);
        BoxConstraints {
            min_width: min_w,
            max_width: (self.max_width - h).max(min_w),
            min_height: min_h,
            max_height: (self.max_height - v).max(min_h),
        }
    }

    /// Return constraints that are as close as possible to `self` while also
    /// satisfying `constraints` (Flutter's `BoxConstraints.enforce`).
    pub fn enforce(&self, constraints: BoxConstraints) -> Self {
        BoxConstraints {
            min_width: self.min_width.clamp(constraints.min_width, constraints.max_width),
            max_width: self.max_width.clamp(constraints.min_width, constraints.max_width),
            min_height: self.min_height.clamp(constraints.min_height, constraints.max_height),
            max_height: self.max_height.clamp(constraints.min_height, constraints.max_height),
        }
    }

    /// Tighten the constraints toward the optionally-given width/height, staying
    /// within the existing bounds.
    pub fn tighten(&self, width: Option<f64>, height: Option<f64>) -> Self {
        let (min_width, max_width) = match width {
            Some(w) => {
                let c = self.constrain_width(w);
                (c, c)
            }
            None => (self.min_width, self.max_width),
        };
        let (min_height, max_height) = match height {
            Some(h) => {
                let c = self.constrain_height(h);
                (c, c)
            }
            None => (self.min_height, self.max_height),
        };
        BoxConstraints { min_width, max_width, min_height, max_height }
    }
}
