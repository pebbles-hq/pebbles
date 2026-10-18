//! R12: the negotiated MSAA sample count renders correctly either way.

use super::*;

const W: u32 = 64;

#[test]
#[ignore = "gpu"]
fn negotiated_count_renders() {
    let Some((device, queue, mut r)) = setup("negotiated_count_renders") else { return };
    let mut s = Scene::new();
    Painter::new(&mut s).fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::BLACK,
        None,
        &Rect::new(10.5, 8.0, 30.0, 56.0),
    );
    let px = render_scene(&device, &queue, &mut r, &s, (W, W), Color::WHITE);
    assert!([1, 4].contains(&r.msaa_sample_count()), "count {}", r.msaa_sample_count());
    for y in 9..=54 {
        let edge = pixel(&px, W, 10, y);
        assert!((0..3).all(|i| (125..=130).contains(&edge[i])), "edge ({y}) {edge:?}");
    }
}
