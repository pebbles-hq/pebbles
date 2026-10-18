//! Acceptance checks on the V26 corpus for R04 (culling is invisible) and R05 (degenerate
//! dashes prepare fast). GPU tests: `--ignored`, skipped without an adapter.

use render_check::candidate::Candidate;
use render_check::corpus;
use render_check::scene::{frame_scene, logical_scene};

fn case(name: &str) -> render_check::case::Case {
    corpus::all().into_iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{name} missing"))
}

/// R04 AC2: `stress/rects_10k` renders identically with and without culling. The scene is
/// drawn shifted by (−200, −200), which pushes hundreds of rects fully off-target (culled) and
/// clamps the ones crossing the edge; pixel (x, y) must equal the unshifted render's pixel
/// (x + 200, y + 200), where nothing near that region is culled.
#[test]
#[ignore = "gpu"]
fn rects_10k_identical_with_and_without_culling() {
    let Ok(mut cand) = Candidate::new() else {
        eprintln!("SKIP: no adapter");
        return;
    };
    let c = case("stress/rects_10k");
    let logical = logical_scene(&c, 0);
    let plain = frame_scene(&logical, 1.0);
    let mut shifted = pebbles_render::paint::Scene::new();
    shifted.append(&logical, Some(kurbo::Affine::translate((-200.0, -200.0))));
    let full = cand.render_rgba(&plain, (1920, 1080), c.background, 1.0).expect("render");
    let culled = cand.render_rgba(&shifted, (1920, 1080), c.background, 1.0).expect("render");
    let n = cand.last_frame_stats().culled;
    println!("culled with the shift: {n}");
    assert!(n > 100, "the shift must cull many rects, culled {n}");
    for y in 0..880 {
        for x in 0..1720 {
            assert_eq!(culled.px(x, y), full.px(x + 200, y + 200), "({x},{y})");
        }
    }
}

/// R05 AC2: `adversarial/dash_degenerate` prepares in < 50 ms in a release build.
#[test]
#[ignore = "gpu"]
fn dash_degenerate_prepares_fast() {
    let Ok(mut cand) = Candidate::new() else {
        eprintln!("SKIP: no adapter");
        return;
    };
    let c = case("adversarial/dash_degenerate");
    let scene = frame_scene(&logical_scene(&c, 0), 1.0);
    cand.render_rgba(&scene, c.device_size(), c.background, 1.0).expect("render");
    let us = cand.last_frame_stats().prepare_us;
    println!("dash_degenerate prepare: {us} µs");
    if !cfg!(debug_assertions) {
        assert!(us < 50_000, "prepare took {us} µs");
    }
}
