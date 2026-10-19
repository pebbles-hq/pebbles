//! 02 §10 G3-5: `frames/image_churn_100` keeps the image-cache census within the 10 §6 budget
//! (dropped images are freed; bytes never exceed the budget). Needs a GPU.
#![cfg(debug_assertions)]

use render_check::candidate::Candidate;
use render_check::scene::{frame_scene, logical_scene};

#[test]
fn image_churn_census_within_budget() {
    let mut cand = match Candidate::new() {
        Ok(c) => c,
        Err(e) if std::env::var_os("PEBBLES_REQUIRE_GPU").is_none() => {
            eprintln!("skipping: no GPU ({e})");
            return;
        }
        Err(e) => panic!("PEBBLES_REQUIRE_GPU set but no GPU: {e}"),
    };
    let case =
        render_check::corpus::all().into_iter().find(|c| c.name == "frames/image_churn_100").expect("case");
    let budget =
        pebbles_gpu::GpuBudgets::defaults(pebbles_gpu::BudgetPlatform::current(), &wgpu::Limits::default())
            .image_cache_bytes;
    let mut peak = 0;
    for frame in 0..101 {
        let scene = frame_scene(&logical_scene(&case, frame), case.scale);
        cand.render_rgba(&scene, case.device_size(), case.background, 1.0).expect("frame");
        let c = cand.census();
        peak = peak.max(c.image_entries);
        assert!(c.image_bytes <= budget, "frame {frame}: {} bytes > budget {budget}", c.image_bytes);
    }
    // The case cycles through a pool of 50 images the app keeps alive: the cache holds at most
    // that working set (no duplicates, no growth from re-drawing).
    assert!(peak <= 50, "peak {peak} entries");
}
