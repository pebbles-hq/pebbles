//! The scene corpus (§7): `all()` concatenates one module per family.

use pebbles_render::paint::{Painter, kurbo, peniko};

use crate::case::Case;

pub mod adversarial;
pub mod brush;
pub mod clip;
pub mod effects;
pub mod frag;
pub mod frames;
pub mod grad;
pub mod img;
pub mod layer;
pub mod mesh;
pub mod prim;
pub mod screen;
pub mod shadow;
pub mod stress;
pub mod stroke;
pub mod text;
pub mod widget;
pub mod xform;

/// Every case, in family order.
pub fn all() -> Vec<Case> {
    let mut out = Vec::new();
    out.extend(prim::cases());
    out.extend(stroke::cases());
    out.extend(xform::cases());
    out.extend(clip::cases());
    out.extend(text::cases());
    out.extend(text::international());
    out.extend(img::cases());
    out.extend(shadow::cases());
    out.extend(layer::cases());
    out.extend(grad::cases());
    out.extend(frag::cases());
    out.extend(brush::cases());
    out.extend(effects::cases());
    out.extend(mesh::cases());
    out.extend(widget::cases());
    out.extend(screen::cases());
    out.extend(stress::cases());
    out.extend(adversarial::cases());
    out.extend(frames::cases());
    out
}

/// The V01 rect (`#3366FF` over (8,8)–(56,56), 64×64 on white). No longer a registered
/// case (V20 removed the `smoke` family); the GPU smoke tests of V13/V14 still draw it.
pub fn solid_rect(p: &mut Painter<'_>) {
    p.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgba8(0x33, 0x66, 0xFF, 0xFF),
        None,
        &kurbo::Rect::new(8.0, 8.0, 56.0, 56.0),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::case::Expect;

    fn valid_card(id: &str) -> bool {
        let b = id.as_bytes();
        b.len() == 3 && b"CRLTMKPSV".contains(&b[0]) && b[1].is_ascii_digit() && b[2].is_ascii_digit()
    }

    #[test]
    fn corpus_names_unique_and_valid() {
        let families = [
            "prim",
            "stroke",
            "xform",
            "clip",
            "text",
            "img",
            "shadow",
            "layer",
            "grad",
            "frag",
            "brush",
            "effect",
            "mesh",
            "sprite",
            "widget",
            "screen",
            "stress",
            "adversarial",
            "frames",
        ];
        let mut seen = HashSet::new();
        for c in all() {
            assert!(seen.insert(c.name.clone()), "duplicate {}", c.name);
            assert!(
                c.name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'/'),
                "bad chars in {}",
                c.name
            );
            assert_eq!(c.name.matches('/').count(), 1, "{}", c.name);
            let family = c.name.split('/').next().unwrap_or_default();
            assert!(families.contains(&family), "{}: family is not a corpus module", c.name);
            if let Expect::Xfail { until, .. } = c.expect {
                assert!(valid_card(until), "{}: xfail card `{until}`", c.name);
            }
        }
    }

    #[test]
    fn corpus_v1_counts() {
        let all = all();
        let n = |f: &str| all.iter().filter(|c| c.name.starts_with(f)).count();
        assert!(n("prim/") >= 15, "prim/ {}", n("prim/"));
        assert!(n("stroke/") >= 50, "stroke/ {}", n("stroke/"));
        assert!(n("xform/") >= 7, "xform/ {}", n("xform/"));
        assert!(n("clip/") >= 7, "clip/ {}", n("clip/"));
        assert!(!all.iter().any(|c| c.name.starts_with("smoke/")), "the smoke family is removed");
    }

    #[test]
    fn blend_and_compose_families_complete() {
        let all = all();
        let n = |prefix: &str| all.iter().filter(|c| c.name.starts_with(prefix)).count();
        assert_eq!(n("layer/blend_") - n("layer/blend_inside_clip") - n("layer/blend_in_sdf_clip"), 16);
        assert_eq!(n("layer/compose_"), 14);
    }
}
