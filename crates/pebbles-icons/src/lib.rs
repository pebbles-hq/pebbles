//! The Pebbles icon **data model** and the bundled **Lucide** icon set.
//!
//! An icon is plain data — [`IconData`], a `Copy`, `'static`-friendly value
//! describing drawable geometry in an SVG-style viewbox. Nothing here renders;
//! `pebbles-render`'s `RenderIcon` consumes an [`IconData`] and paints it. This
//! is what makes the icon set **pluggable**: the default set is Lucide, but any
//! `IconData` — yours included — drops straight in.
//!
//! ```ignore
//! use pebbles::prelude::*;
//!
//! icon(IconKind::Check);      // a named built-in (resolves to a Lucide glyph)
//! icon(lucide::CAMERA);       // any of the ~1800 bundled Lucide icons
//! icon(lucide::by_name("circle-check").unwrap());   // …or look one up by name
//!
//! // Bring your own — a compile-time const, no framework buy-in:
//! const BRAND: IconData = IconData::filled(24.0, &[IconPrim::Path("M12 2 …")]);
//! icon(BRAND);
//! ```

pub mod lucide;

/// One drawable primitive in an icon's viewbox, mirroring the SVG element set
/// Lucide uses. Coordinates are in the icon's own `view` units (24 for Lucide).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IconPrim {
    /// An SVG path `d` string (parsed with `kurbo` at paint time).
    Path(&'static str),
    /// A straight segment `(x1, y1) → (x2, y2)`.
    Line(f64, f64, f64, f64),
    /// An open polyline through the given points.
    Polyline(&'static [(f64, f64)]),
    /// A closed polygon through the given points.
    Polygon(&'static [(f64, f64)]),
    /// A circle `(cx, cy)` radius `r`.
    Circle(f64, f64, f64),
    /// An ellipse `(cx, cy)` radii `(rx, ry)`.
    Ellipse(f64, f64, f64, f64),
    /// A rectangle `(x, y, w, h)` with corner radii `(rx, ry)`.
    Rect(f64, f64, f64, f64, f64, f64),
}

/// A renderable icon: geometry plus how to ink it. Cheap to copy and store; the
/// bundled Lucide glyphs are `const` values built entirely from `&'static` data.
///
/// `view` is the side of the (square) viewbox the primitives are authored in;
/// the renderer scales it to the requested pixel size. `fill = false` strokes
/// the geometry (the Lucide style); `fill = true` fills it (for solid glyphs).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct IconData {
    /// Side length of the authoring viewbox (Lucide uses 24).
    pub view: f64,
    /// Fill the geometry instead of stroking it.
    pub fill: bool,
    /// Stroke width, in viewbox units (Lucide uses 2).
    pub stroke_width: f64,
    /// The primitives that make up the glyph.
    pub prims: &'static [IconPrim],
}

impl IconData {
    /// A stroked glyph in a `view`-unit box (stroke width 2 — the Lucide style).
    pub const fn stroked(view: f64, prims: &'static [IconPrim]) -> Self {
        IconData { view, fill: false, stroke_width: 2.0, prims }
    }

    /// A filled glyph in a `view`-unit box (for solid icons / logos).
    pub const fn filled(view: f64, prims: &'static [IconPrim]) -> Self {
        IconData { view, fill: true, stroke_width: 0.0, prims }
    }

    /// Override the stroke width (viewbox units).
    pub const fn with_stroke_width(mut self, w: f64) -> Self {
        self.stroke_width = w;
        self
    }

    /// Build an icon at runtime from SVG path `d` strings — e.g. a glyph loaded
    /// from disk. The strings are **leaked** to `'static`, so this is intended
    /// for a bounded, register-once set (not a per-frame call).
    pub fn leak_svg_paths(view: f64, fill: bool, paths: impl IntoIterator<Item = String>) -> Self {
        let prims: Vec<IconPrim> =
            paths.into_iter().map(|d| IconPrim::Path(String::leak(d))).collect();
        let prims: &'static [IconPrim] = Vec::leak(prims);
        IconData { view, fill, stroke_width: if fill { 0.0 } else { 2.0 }, prims }
    }
}

/// The built-in **named** icons. These are ergonomic handles used across the
/// widget catalog; each resolves to a bundled Lucide glyph. For the full set,
/// reach for [`lucide`] directly. Any `IconData` also works wherever an
/// `impl Into<IconData>` is accepted, so custom icons need no enum entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    Check,
    Close,
    Plus,
    Minus,
    ChevronDown,
    ChevronUp,
    ChevronRight,
    ChevronLeft,
    Menu,
    Search,
    Star,
    Dot,
    Info,
    Warning,
    ArrowRight,
    Circle,
    Eye,
    EyeOff,
    Mail,
    Calendar,
    Lock,
    User,
    Phone,
}

impl IconKind {
    /// The Lucide glyph this named icon resolves to.
    pub const fn data(self) -> IconData {
        match self {
            IconKind::Check => lucide::CHECK,
            IconKind::Close => lucide::X,
            IconKind::Plus => lucide::PLUS,
            IconKind::Minus => lucide::MINUS,
            IconKind::ChevronDown => lucide::CHEVRON_DOWN,
            IconKind::ChevronUp => lucide::CHEVRON_UP,
            IconKind::ChevronRight => lucide::CHEVRON_RIGHT,
            IconKind::ChevronLeft => lucide::CHEVRON_LEFT,
            IconKind::Menu => lucide::MENU,
            IconKind::Search => lucide::SEARCH,
            IconKind::Star => lucide::STAR,
            IconKind::Dot => lucide::DOT,
            IconKind::Info => lucide::INFO,
            IconKind::Warning => lucide::TRIANGLE_ALERT,
            IconKind::ArrowRight => lucide::ARROW_RIGHT,
            IconKind::Circle => lucide::CIRCLE,
            IconKind::Eye => lucide::EYE,
            IconKind::EyeOff => lucide::EYE_OFF,
            IconKind::Mail => lucide::MAIL,
            IconKind::Calendar => lucide::CALENDAR,
            IconKind::Lock => lucide::LOCK,
            IconKind::User => lucide::USER,
            IconKind::Phone => lucide::PHONE,
        }
    }
}

impl From<IconKind> for IconData {
    fn from(k: IconKind) -> IconData {
        k.data()
    }
}
