//! R15 AC2: Tier B path masks on the GPU match the golden model (05 §6.7a) within the Geometry
//! thresholds (05 §8): a star, an even-odd ring and a dashed circle stroke.
//!
//! zeno accumulates signed area, so where a fill's own contours overlap inside one pixel
//! (kurbo's dash pieces abut and overlap near a closed path's start) it double-counts; the
//! golden model resolves winding exactly. Those pixels — where the golden non-zero and even-odd
//! coverage of the outline differ — are excluded from the threshold and must stay < 0.1 %.

use kurbo::{Affine, BezPath, Circle, Shape, Stroke};
use pebbles_render::paint::{Painter, Scene};
use peniko::{Color, Fill};
use render_check::args::Tolerance;
use render_check::candidate::Candidate;
use render_check::case::Category;
use render_check::golden::Golden;
use render_check::golden::coverage::fill_coverage;
use render_check::golden::stroke::outline;
use render_check::metric::{Rgba8, compare};
use render_check::thresholds::{limits, passes};

const SIZE: u32 = 96;

/// A named scene builder and its excluded self-overlap pixels.
type Case = (&'static str, Box<dyn Fn(&mut Painter<'_>)>, Vec<bool>);

fn star() -> BezPath {
    let mut p = BezPath::new();
    for k in 0..10 {
        let r = if k % 2 == 0 { 40.0 } else { 16.0 };
        let a = std::f64::consts::PI * f64::from(k) / 5.0 - std::f64::consts::FRAC_PI_2;
        let pt = (48.0 + r * a.cos(), 48.0 + r * a.sin());
        if k == 0 { p.move_to(pt) } else { p.line_to(pt) }
    }
    p.close_path();
    p
}

fn ring() -> BezPath {
    let mut p = Circle::new((48.0, 48.0), 36.0).to_path(0.01);
    p.extend(Circle::new((48.0, 48.0), 20.0).to_path(0.01));
    p
}

/// Pixels where `path` (a non-zero fill) overlaps itself: exact non-zero ≠ even-odd coverage.
fn self_overlap(path: &BezPath) -> Vec<bool> {
    let nz = fill_coverage(path, Affine::IDENTITY, Fill::NonZero, SIZE, SIZE);
    let eo = fill_coverage(path, Affine::IDENTITY, Fill::EvenOdd, SIZE, SIZE);
    nz.iter().zip(&eo).map(|(a, b)| (a - b).abs() > 1e-9).collect()
}

/// `img` with the masked pixels replaced by `from`'s.
fn masked(img: &Rgba8, from: &Rgba8, mask: &[bool]) -> Rgba8 {
    let mut out = img.clone();
    for (i, &m) in mask.iter().enumerate() {
        if m {
            out.data[i * 4..i * 4 + 4].copy_from_slice(&from.data[i * 4..i * 4 + 4]);
        }
    }
    out
}

#[test]
#[ignore = "gpu"]
fn path_mask_matches_golden() {
    let Ok(mut cand) = Candidate::new() else {
        eprintln!("SKIP path_mask_matches_golden: no adapter");
        return;
    };
    let lim = limits(Category::Geometry, Tolerance::Canonical).expect("geometry limits");
    let mut dashed = Stroke::new(4.0);
    dashed.dash_pattern = [10.0, 6.0].into_iter().collect();
    let circle = Circle::new((48.0, 48.0), 30.0);
    let dashed_outline = outline(&circle.to_path(1.0 / 1024.0), &dashed, Affine::IDENTITY);
    let no_overlap = vec![false; (SIZE * SIZE) as usize];
    let cases: [Case; 3] = [
        (
            "star",
            Box::new(|p| p.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &star())),
            no_overlap.clone(),
        ),
        (
            "ring_evenodd",
            Box::new(|p| p.fill(Fill::EvenOdd, Affine::IDENTITY, Color::BLACK, None, &ring())),
            no_overlap,
        ),
        (
            "dashed_circle",
            Box::new(move |p| p.stroke(&dashed, Affine::IDENTITY, Color::BLACK, None, &circle)),
            self_overlap(&dashed_outline),
        ),
    ];
    for (name, draw, overlap) in &cases {
        let mut scene = Scene::new();
        draw(&mut Painter::new(&mut scene));
        let got = cand.render_rgba(&scene, (SIZE, SIZE), Color::WHITE, 1.0).expect("candidate");
        assert_eq!(cand.last_frame_stats().tier_b_draws, 1, "{name} is a Tier B mask");
        let want = Golden::render(&scene, SIZE, SIZE, Color::WHITE).expect("golden");
        let n_overlap = overlap.iter().filter(|&&o| o).count();
        assert!(n_overlap * 1000 < overlap.len(), "{name}: {n_overlap} self-overlap pixels");
        let m = compare(&want, &masked(&got, &want, overlap), lim.over_threshold).expect("same size");
        println!("{name}: {m:?} (self-overlap pixels excluded: {n_overlap})");
        assert!(passes(&m, &lim), "{name}: {m:?} vs {lim:?}");
    }
}
