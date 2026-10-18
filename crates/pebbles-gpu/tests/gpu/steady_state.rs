//! Steady-state counters (10 §9.1, M04): FrameStats counts real work.

use super::*;

/// M04 AC2: a 100×100 grid of 8×8 rects in two alternating colors (the §11.1 recipe).
#[test]
#[ignore = "gpu"]
fn stats_count_draws_and_passes() {
    let Some((device, queue, mut r)) = setup("stats_count_draws_and_passes") else { return };
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    for y in 0..100u32 {
        for x in 0..100u32 {
            let c = if (x + y) % 2 == 0 { Color::BLACK } else { Color::from_rgb8(37, 99, 235) };
            let (fx, fy) = (f64::from(x) * 8.0, f64::from(y) * 8.0);
            p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &Rect::new(fx, fy, fx + 8.0, fy + 8.0));
        }
    }
    let _ = render_scene(&device, &queue, &mut r, &s, (800, 800), Color::WHITE);
    let st = r.last_frame_stats();
    assert_eq!(st.render_passes, 1, "{st}");
    assert!(st.draw_calls >= 1, "{st}");
    assert!(st.buffer_bytes_uploaded > 0, "{st}");
    assert_eq!(st.instances, 10_000, "{st}");
}
