//! [`BoxDecoration`] — the visual styling a box can paint: background color,
//! border, corner radius and drop shadows. This is what makes cards, buttons,
//! inputs and surfaces look like a design system rather than flat rectangles.

use pebbles_foundation::{Color, Offset};

/// Per-corner radii, in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct BorderRadius {
    pub top_left: f64,
    pub top_right: f64,
    pub bottom_right: f64,
    pub bottom_left: f64,
}

impl BorderRadius {
    pub const ZERO: BorderRadius = BorderRadius::all(0.0);

    /// The same radius on all four corners.
    pub const fn all(r: f64) -> Self {
        BorderRadius { top_left: r, top_right: r, bottom_right: r, bottom_left: r }
    }

    /// The largest single radius (used where a uniform radius is required).
    pub fn max(&self) -> f64 {
        self.top_left.max(self.top_right).max(self.bottom_right).max(self.bottom_left)
    }

    pub(crate) fn to_radii(self) -> vello::kurbo::RoundedRectRadii {
        vello::kurbo::RoundedRectRadii::new(
            self.top_left,
            self.top_right,
            self.bottom_right,
            self.bottom_left,
        )
    }
}

/// A uniform border drawn around a box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Border {
    pub color: Color,
    pub width: f64,
}

impl Border {
    pub fn new(color: Color, width: f64) -> Self {
        Border { color, width }
    }
}

/// A soft drop shadow.
#[derive(Clone, Copy, Debug)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Offset,
    /// Gaussian blur radius.
    pub blur: f64,
    /// How far to grow (or shrink, if negative) the shadow rect before blurring.
    pub spread: f64,
}

impl BoxShadow {
    pub fn new(color: Color, offset: Offset, blur: f64, spread: f64) -> Self {
        BoxShadow { color, offset, blur, spread }
    }
}

/// The complete visual description of a box's surface.
#[derive(Clone, Debug, Default)]
pub struct BoxDecoration {
    pub color: Option<Color>,
    pub border: Option<Border>,
    pub radius: BorderRadius,
    pub shadows: Vec<BoxShadow>,
}

impl BoxDecoration {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn border(mut self, border: Border) -> Self {
        self.border = Some(border);
        self
    }
    pub fn radius(mut self, radius: BorderRadius) -> Self {
        self.radius = radius;
        self
    }
    pub fn shadow(mut self, shadow: BoxShadow) -> Self {
        self.shadows.push(shadow);
        self
    }
}
