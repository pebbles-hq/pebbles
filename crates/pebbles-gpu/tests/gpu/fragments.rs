//! Fragments (09 §7, L50): appended sub-scenes render exactly like their flattened ops.

use super::*;
use peniko::Mix;

/// One "card": a clip, a translucent group of two fills and a stroke, in local space.
fn card(p: &mut Painter<'_>, xf: Affine) {
    p.push_layer(Fill::NonZero, Mix::Normal, 1.0, xf, &kurbo::RoundedRect::new(0.0, 0.0, 40.0, 30.0, 6.0));
    p.push_layer(Fill::NonZero, Mix::Normal, 0.6, xf, &Rect::new(0.0, 0.0, 40.0, 30.0));
    p.fill(Fill::NonZero, xf, Color::from_rgba8(220, 40, 40, 255), None, &Rect::new(0.0, 0.0, 30.0, 20.0));
    p.fill(Fill::NonZero, xf, Color::from_rgba8(40, 80, 220, 255), None, &Rect::new(10.0, 10.0, 40.0, 30.0));
    p.pop_layer();
    p.stroke(&Stroke::new(2.0), xf, Color::BLACK, None, &kurbo::Circle::new((20.0, 15.0), 8.0));
    p.pop_layer();
}

#[test]
#[ignore = "gpu"]
fn nested_fragments_match_flattened_scene() {
    let Some((device, queue, mut r)) = setup("nested_fragments_match_flattened_scene") else { return };
    let (outer, inner) = (Affine::translate((8.0, 6.0)), Affine::rotate_about(0.2, (20.0, 15.0)));
    let placements = [
        Affine::IDENTITY,
        Affine::translate((48.0, 0.0)),
        Affine::translate((0.0, 40.0)) * Affine::scale(1.2),
    ];
    // Flat: every op carries its full transform.
    let mut flat = Scene::new();
    {
        let mut p = Painter::new(&mut flat);
        for t in placements {
            card(&mut p, outer * t * inner);
        }
    }
    // Nested: card (under `inner`) → placed fragment (under each t) → outer fragment (`outer`).
    let mut leaf = Scene::new();
    card(&mut Painter::new(&mut leaf), Affine::IDENTITY);
    let mut mid = Scene::new();
    for t in placements {
        let mut one = Scene::new();
        one.append(&leaf, Some(inner));
        mid.append(&one, Some(t));
    }
    let mut nested = Scene::new();
    nested.append(&mid, Some(outer));
    let a = render_scene(&device, &queue, &mut r, &flat, (128, 96), Color::WHITE);
    let b = render_scene(&device, &queue, &mut r, &nested, (128, 96), Color::WHITE);
    assert!(a == b, "nested fragments differ from the flattened scene");
}
