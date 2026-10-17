//! The scene corpus (§7): `all()` concatenates one module per family.

use pebbles_render::paint::{Painter, kurbo, peniko};

use crate::case::{Case, Category, Slice, ops};

/// Every case, in family order.
pub fn all() -> Vec<Case> {
    let mut out = Vec::new();
    out.extend(smoke());
    out
}

/// The V01 rect: `#3366FF` over `(8,8)-(56,56)` on white, 64×64.
fn smoke() -> Vec<Case> {
    vec![ops("smoke/solid_rect", Category::Exact, Slice::P1, 64, 64, solid_rect)]
}

/// `smoke/solid_rect`.
pub fn solid_rect(p: &mut Painter<'_>) {
    p.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgba8(0x33, 0x66, 0xFF, 0xFF),
        None,
        &kurbo::Rect::new(8.0, 8.0, 56.0, 56.0),
    );
}
