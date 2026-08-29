//! [`Theme`] — design tokens shared across the component catalog, in the spirit of
//! shadcn/ui's CSS variables. A global "current" theme is read by components at
//! build time (until an inherited-widget `ThemeProvider` lands, this is a simple
//! thread-local you set once at startup with [`Theme::make_current`]).

use std::cell::RefCell;

use pebbles_foundation::Color;

/// The semantic color roles a component can reference.
#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub background: Color,
    pub foreground: Color,
    pub card: Color,
    pub card_foreground: Color,
    pub popover: Color,
    pub popover_foreground: Color,
    pub primary: Color,
    pub primary_foreground: Color,
    pub secondary: Color,
    pub secondary_foreground: Color,
    pub muted: Color,
    pub muted_foreground: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub destructive: Color,
    pub destructive_foreground: Color,
    pub success: Color,
    pub warning: Color,
    pub border: Color,
    pub input: Color,
    pub ring: Color,
}

/// The full token set: colors, corner radius, spacing unit and base font size.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub colors: Colors,
    /// Base corner radius (buttons, inputs, cards).
    pub radius: f64,
    /// Base spacing unit; paddings/gaps are multiples of this.
    pub spacing: f64,
    /// Base body font size.
    pub font_size: f32,
    pub dark: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

impl Theme {
    /// The default light theme (zinc/neutral palette).
    pub fn light() -> Self {
        Theme {
            colors: Colors {
                background: rgb(0xFF, 0xFF, 0xFF),
                foreground: rgb(0x0A, 0x0A, 0x0A),
                card: rgb(0xFF, 0xFF, 0xFF),
                card_foreground: rgb(0x0A, 0x0A, 0x0A),
                popover: rgb(0xFF, 0xFF, 0xFF),
                popover_foreground: rgb(0x0A, 0x0A, 0x0A),
                primary: rgb(0x18, 0x18, 0x1B),
                primary_foreground: rgb(0xFA, 0xFA, 0xFA),
                secondary: rgb(0xF4, 0xF4, 0xF5),
                secondary_foreground: rgb(0x18, 0x18, 0x1B),
                muted: rgb(0xF4, 0xF4, 0xF5),
                muted_foreground: rgb(0x71, 0x71, 0x7A),
                accent: rgb(0xF4, 0xF4, 0xF5),
                accent_foreground: rgb(0x18, 0x18, 0x1B),
                destructive: rgb(0xEF, 0x44, 0x44),
                destructive_foreground: rgb(0xFA, 0xFA, 0xFA),
                success: rgb(0x22, 0xC5, 0x5E),
                warning: rgb(0xF5, 0x9E, 0x0B),
                border: rgb(0xE4, 0xE4, 0xE7),
                input: rgb(0xE4, 0xE4, 0xE7),
                ring: rgb(0xA1, 0xA1, 0xAA),
            },
            radius: 8.0,
            spacing: 8.0,
            font_size: 14.0,
            dark: false,
        }
    }

    /// The default dark theme.
    pub fn dark() -> Self {
        Theme {
            colors: Colors {
                background: rgb(0x0A, 0x0A, 0x0B),
                foreground: rgb(0xFA, 0xFA, 0xFA),
                card: rgb(0x14, 0x14, 0x16),
                card_foreground: rgb(0xFA, 0xFA, 0xFA),
                popover: rgb(0x14, 0x14, 0x16),
                popover_foreground: rgb(0xFA, 0xFA, 0xFA),
                primary: rgb(0xFA, 0xFA, 0xFA),
                primary_foreground: rgb(0x18, 0x18, 0x1B),
                secondary: rgb(0x27, 0x27, 0x2A),
                secondary_foreground: rgb(0xFA, 0xFA, 0xFA),
                muted: rgb(0x27, 0x27, 0x2A),
                muted_foreground: rgb(0xA1, 0xA1, 0xAA),
                accent: rgb(0x27, 0x27, 0x2A),
                accent_foreground: rgb(0xFA, 0xFA, 0xFA),
                destructive: rgb(0x7F, 0x1D, 0x1D),
                destructive_foreground: rgb(0xFA, 0xFA, 0xFA),
                success: rgb(0x16, 0xA3, 0x4A),
                warning: rgb(0xD9, 0x77, 0x06),
                border: rgb(0x27, 0x27, 0x2A),
                input: rgb(0x27, 0x27, 0x2A),
                ring: rgb(0x52, 0x52, 0x5B),
            },
            radius: 8.0,
            spacing: 8.0,
            font_size: 14.0,
            dark: true,
        }
    }

    /// Install this theme as the process-wide current theme.
    pub fn make_current(self) {
        CURRENT.with(|c| *c.borrow_mut() = self);
    }
}

thread_local! {
    static CURRENT: RefCell<Theme> = RefCell::new(Theme::light());
}

/// The current theme (cheap `Copy`). Components call this in `build`.
pub fn theme() -> Theme {
    CURRENT.with(|c| *c.borrow())
}

/// Scale a color's RGB channels by `factor` (`<1.0` darkens, `>1.0` lightens),
/// keeping alpha. Used for hover/pressed state shading.
pub fn shade(color: Color, factor: f32) -> Color {
    let [r, g, b, a] = color.components;
    Color::new([
        (r * factor).clamp(0.0, 1.0),
        (g * factor).clamp(0.0, 1.0),
        (b * factor).clamp(0.0, 1.0),
        a,
    ])
}

/// Linearly interpolate between two colors (`t` in `0.0..=1.0`). Used for hover/
/// pressed overlays that stay visible on both light and dark surfaces.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let [ar, ag, ab, aa] = a.components;
    let [br, bg, bb, ba] = b.components;
    let l = |x: f32, y: f32| x + (y - x) * t;
    Color::new([l(ar, br), l(ag, bg), l(ab, bb), l(aa, ba)])
}
