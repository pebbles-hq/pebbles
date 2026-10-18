//! R14: zeno Tier B masks against the golden model (V44), 300 seeded random polygons.
//!
//! Simple (star-shaped) polygons must be within ±1 LSB on ≥ 99.9 % of pixels; self-intersecting
//! ones on ≥ 99.8 % (lyon resolves the winding into disjoint triangles before zeno integrates
//! area — without it, overlaps inside one pixel were double-counted: 99.64 %, max Δ 218).

use kurbo::{Affine, BezPath};
use peniko::Fill;
use render_check::golden::coverage::fill_coverage;
use render_check::rng::XorShift64;

const W: u32 = 48;
const H: u32 = 48;

/// (fraction within ±1 LSB, max delta) over `n` polygons from `make`.
fn agreement(seed: u64, n: u32, mut make: impl FnMut(&mut XorShift64) -> BezPath) -> (f64, u8) {
    let mut rng = XorShift64::new(seed);
    let (mut total, mut within, mut max) = (0usize, 0usize, 0u8);
    for i in 0..n {
        let p = make(&mut rng);
        let rule = if i % 2 == 0 { Fill::NonZero } else { Fill::EvenOdd };
        let mask = pebbles_gpu::__tier_b_mask_for_check(&p, Affine::IDENTITY, rule, W, H);
        let gold = fill_coverage(&p, Affine::IDENTITY, rule, W, H);
        for (&m, &g) in mask.iter().zip(&gold) {
            let want = (g.clamp(0.0, 1.0) * 255.0 + 0.5).floor() as i32;
            let d = (i32::from(m) - want).unsigned_abs().min(255) as u8;
            max = max.max(d);
            total += 1;
            within += usize::from(d <= 1);
        }
    }
    (within as f64 / total as f64, max)
}

/// A simple polygon: 3–12 vertices at sorted angles around a center, every gap < π (it may
/// exceed the target, exercising the mask clip).
fn star_shaped(rng: &mut XorShift64) -> BezPath {
    let (cx, cy) = (rng.range_f64(4.0, 44.0), rng.range_f64(4.0, 44.0));
    let n = rng.range_u32(3, 13);
    // Star-shaped about the center is simple only when every angular gap is below π.
    let angles = loop {
        let mut a: Vec<f64> = (0..n).map(|_| rng.range_f64(0.0, std::f64::consts::TAU)).collect();
        a.sort_by(f64::total_cmp);
        let wrap = a[0] + std::f64::consts::TAU - a[a.len() - 1];
        if a.windows(2).map(|w| w[1] - w[0]).chain([wrap]).all(|g| g < std::f64::consts::PI) {
            break a;
        }
    };
    let mut p = BezPath::new();
    for (k, a) in angles.iter().enumerate() {
        let r = rng.range_f64(2.0, 30.0);
        let pt = (cx + r * a.cos(), cy + r * a.sin());
        if k == 0 { p.move_to(pt) } else { p.line_to(pt) }
    }
    p.close_path();
    p
}

fn arbitrary(rng: &mut XorShift64) -> BezPath {
    let mut p = BezPath::new();
    let n = rng.range_u32(3, 12);
    p.move_to((rng.range_f64(-4.0, 52.0), rng.range_f64(-4.0, 52.0)));
    for _ in 1..n {
        p.line_to((rng.range_f64(-4.0, 52.0), rng.range_f64(-4.0, 52.0)));
    }
    p.close_path();
    p
}

#[test]
fn random_simple_polygons_match_golden() {
    let (frac, max) = agreement(0x5EED_0014, 300, star_shaped);
    println!("tier B vs golden (simple): {frac:.5} within ±1 LSB, max delta {max}");
    assert!(frac >= 0.999, "{frac} within ±1 LSB (max {max})");
}

#[test]
fn random_self_intersecting_polygons_are_close() {
    let (frac, max) = agreement(0x5EED_0015, 300, arbitrary);
    println!("tier B vs golden (self-intersecting): {frac:.5} within ±1 LSB, max delta {max}");
    assert!(frac >= 0.998, "{frac} within ±1 LSB (max {max})");
}
