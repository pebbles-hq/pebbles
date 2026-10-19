//! M25: the device-free prepare microbench (10 §11.3).

use std::time::Instant;

use kurbo::Affine;
use pebbles_render::paint::{Painter, Scene};

use super::{PrepareInput, PrepareScratch, prepare};
use crate::stats::FrameStats;
use crate::testing::bench_scenes::BENCH_CASES;

/// Nearest-rank percentile, the 10 §11.2 `pct`.
fn pct(sorted: &[u64], p: usize) -> u64 {
    sorted.get(((sorted.len().max(1) - 1) * p + 50) / 100).copied().unwrap_or(0)
}

/// CPU prepare time of every `BENCH_CASES` scene: 20 warm-up calls with a reused scratch, then
/// 200 timed calls; prints `"{case}\t{p50_us}\t{p95_us}"`.
///
/// Run with `--release` for meaningful numbers; informative, never a CI gate.
#[test]
#[ignore = "bench"]
#[allow(clippy::print_stdout)]
fn prepare_bench() {
    let limits = crate::test_support::webgl2_limits();
    for (name, size, f) in BENCH_CASES {
        let mut scene = Scene::new();
        f(&mut Painter::new(&mut scene));
        let input = PrepareInput {
            target_size: *size,
            root_transform: Affine::IDENTITY,
            vertex_limit: u32::MAX,
            scale_factor: 1.0,
            glyph_hint: None,
        };
        let mut scratch = PrepareScratch::default();
        let mut stats = FrameStats::default();
        for _ in 0..20 {
            prepare(&scene, &input, &limits, &mut scratch, &mut stats);
            scratch.plan.release_refs();
        }
        let mut us: Vec<u64> = (0..200)
            .map(|_| {
                let t = Instant::now();
                prepare(&scene, &input, &limits, &mut scratch, &mut stats);
                let d = u64::try_from(t.elapsed().as_micros()).unwrap_or(u64::MAX);
                scratch.plan.release_refs();
                d
            })
            .collect();
        us.sort_unstable();
        println!("{name}\t{}\t{}", pct(&us, 50), pct(&us, 95));
    }
}
