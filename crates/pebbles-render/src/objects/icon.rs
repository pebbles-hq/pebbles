//! [`RenderIcon`] — draws a built-in vector icon (Lucide-style line glyphs) inside
//! its box. Icons are defined in a 24×24 space and scaled to the requested size.

use pebbles_foundation::{Color, Offset, Size};
use vello::kurbo::{Affine, BezPath, Cap, Circle, Join, Stroke};
use vello::peniko::Fill;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::tree::{LayoutCx, PaintCx};

/// The built-in icon set (a pragmatic subset; extend as needed).
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

/// A leaf render object that paints an [`IconKind`].
pub struct RenderIcon {
    pub kind: IconKind,
    pub size: f64,
    pub color: Color,
}

impl RenderIcon {
    pub fn new(kind: IconKind, size: f64, color: Color) -> Self {
        RenderIcon { kind, size, color }
    }
}

/// Polylines (in 24-unit space) for stroke-based icons.
fn polylines(kind: IconKind) -> &'static [&'static [(f64, f64)]] {
    match kind {
        IconKind::Check => &[&[(5.0, 12.5), (10.0, 17.0), (19.0, 6.5)]],
        IconKind::Close => &[&[(6.0, 6.0), (18.0, 18.0)], &[(18.0, 6.0), (6.0, 18.0)]],
        IconKind::Plus => &[&[(12.0, 5.0), (12.0, 19.0)], &[(5.0, 12.0), (19.0, 12.0)]],
        IconKind::Minus => &[&[(5.0, 12.0), (19.0, 12.0)]],
        IconKind::ChevronDown => &[&[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]],
        IconKind::ChevronUp => &[&[(6.0, 15.0), (12.0, 9.0), (18.0, 15.0)]],
        IconKind::ChevronRight => &[&[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)]],
        IconKind::ChevronLeft => &[&[(15.0, 6.0), (9.0, 12.0), (15.0, 18.0)]],
        IconKind::Menu => {
            &[&[(4.0, 6.0), (20.0, 6.0)], &[(4.0, 12.0), (20.0, 12.0)], &[(4.0, 18.0), (20.0, 18.0)]]
        }
        IconKind::ArrowRight => {
            &[&[(4.0, 12.0), (20.0, 12.0)], &[(13.0, 5.0), (20.0, 12.0), (13.0, 19.0)]]
        }
        IconKind::Warning => {
            &[&[(12.0, 3.0), (22.0, 20.0), (2.0, 20.0), (12.0, 3.0)], &[(12.0, 10.0), (12.0, 14.0)]]
        }
        IconKind::Mail => &[
            &[(3.0, 6.0), (21.0, 6.0), (21.0, 18.0), (3.0, 18.0), (3.0, 6.0)],
            &[(3.5, 7.0), (12.0, 13.0), (20.5, 7.0)],
        ],
        IconKind::Calendar => &[
            &[(4.0, 6.0), (20.0, 6.0), (20.0, 20.0), (4.0, 20.0), (4.0, 6.0)],
            &[(8.0, 3.5), (8.0, 7.5)],
            &[(16.0, 3.5), (16.0, 7.5)],
            &[(4.0, 10.0), (20.0, 10.0)],
        ],
        IconKind::Lock => &[
            &[(5.0, 11.0), (19.0, 11.0), (19.0, 20.0), (5.0, 20.0), (5.0, 11.0)],
            &[(8.0, 11.0), (8.0, 8.0), (9.0, 6.4), (12.0, 5.8), (15.0, 6.4), (16.0, 8.0), (16.0, 11.0)],
        ],
        IconKind::User => &[&[(5.0, 20.5), (6.6, 15.0), (17.4, 15.0), (19.0, 20.5)]],
        IconKind::Phone => &[&[(7.0, 3.5), (17.0, 3.5), (17.0, 20.5), (7.0, 20.5), (7.0, 3.5)]],
        _ => &[],
    }
}

impl RenderObject for RenderIcon {
    fn layout(&mut self, _cx: &mut LayoutCx, constraints: BoxConstraints) -> Size {
        constraints.constrain(Size::new(self.size, self.size))
    }

    fn paint(&self, cx: &mut PaintCx, offset: Offset) {
        let scale = self.size / 24.0;
        let t = Affine::translate((offset.x, offset.y)) * Affine::scale(scale);
        let stroke = Stroke::new(2.0).with_caps(Cap::Round).with_join(Join::Round);

        for poly in polylines(self.kind) {
            let mut path = BezPath::new();
            for (i, &(x, y)) in poly.iter().enumerate() {
                if i == 0 {
                    path.move_to((x, y));
                } else {
                    path.line_to((x, y));
                }
            }
            cx.scene.stroke(&stroke, t, self.color, None, &path);
        }

        match self.kind {
            IconKind::Search => {
                cx.scene.stroke(&stroke, t, self.color, None, &Circle::new((10.5, 10.5), 7.0));
                let mut line = BezPath::new();
                line.move_to((15.5, 15.5));
                line.line_to((21.0, 21.0));
                cx.scene.stroke(&stroke, t, self.color, None, &line);
            }
            IconKind::Info => {
                cx.scene.stroke(&stroke, t, self.color, None, &Circle::new((12.0, 12.0), 9.0));
                let mut line = BezPath::new();
                line.move_to((12.0, 11.0));
                line.line_to((12.0, 16.0));
                cx.scene.stroke(&stroke, t, self.color, None, &line);
                cx.scene.fill(Fill::NonZero, t, self.color, None, &Circle::new((12.0, 8.0), 1.0));
            }
            IconKind::Dot => {
                cx.scene.fill(Fill::NonZero, t, self.color, None, &Circle::new((12.0, 12.0), 4.0));
            }
            IconKind::Circle => {
                cx.scene.stroke(&stroke, t, self.color, None, &Circle::new((12.0, 12.0), 9.0));
            }
            IconKind::Star => {
                cx.scene.fill(Fill::NonZero, t, self.color, None, &star_path());
            }
            IconKind::Eye | IconKind::EyeOff => {
                // A smooth almond outline (cubic curves, symmetric top/bottom).
                let mut lens = BezPath::new();
                lens.move_to((2.0, 12.0));
                lens.curve_to((6.0, 6.0), (18.0, 6.0), (22.0, 12.0));
                lens.curve_to((18.0, 18.0), (6.0, 18.0), (2.0, 12.0));
                lens.close_path();
                cx.scene.stroke(&stroke, t, self.color, None, &lens);
                cx.scene.stroke(&stroke, t, self.color, None, &Circle::new((12.0, 12.0), 3.0));
                if self.kind == IconKind::EyeOff {
                    let mut slash = BezPath::new();
                    slash.move_to((3.5, 3.5));
                    slash.line_to((20.5, 20.5));
                    cx.scene.stroke(&stroke, t, self.color, None, &slash);
                }
            }
            IconKind::User => {
                cx.scene.stroke(&stroke, t, self.color, None, &Circle::new((12.0, 8.5), 3.5));
            }
            IconKind::Phone => {
                cx.scene.fill(Fill::NonZero, t, self.color, None, &Circle::new((12.0, 17.8), 0.9));
            }
            _ => {}
        }
    }

    fn debug_name(&self) -> &'static str {
        "RenderIcon"
    }
}

/// A filled 5-point star in 24-unit space.
fn star_path() -> BezPath {
    // (cx, cy) = (12, 12); outer r = 9, inner r = 3.7.
    const PTS: [(f64, f64); 10] = [
        (12.0, 3.0),
        (13.9, 9.2),
        (20.4, 9.2),
        (15.2, 13.0),
        (17.1, 19.2),
        (12.0, 15.4),
        (6.9, 19.2),
        (8.8, 13.0),
        (3.6, 9.2),
        (10.1, 9.2),
    ];
    let mut path = BezPath::new();
    path.move_to(PTS[0]);
    for p in &PTS[1..] {
        path.line_to(*p);
    }
    path.close_path();
    path
}
