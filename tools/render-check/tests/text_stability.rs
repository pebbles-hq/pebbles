//! 02 §10 G2-5: 300-frame stability of `text/latin_sizes` — the atlas census and the ink count
//! are constant after frame 1 (no growth, no eviction churn, no disappearing glyphs). Needs a GPU.
#![cfg(debug_assertions)]

use render_check::candidate::Candidate;
use render_check::scene::{frame_scene, logical_scene};

#[test]
fn latin_sizes_300_frames_census_and_ink_constant() {
    let mut cand = match Candidate::new() {
        Ok(c) => c,
        Err(e) if std::env::var_os("PEBBLES_REQUIRE_GPU").is_none() => {
            eprintln!("skipping: no GPU ({e})");
            return;
        }
        Err(e) => panic!("PEBBLES_REQUIRE_GPU set but no GPU: {e}"),
    };
    let case = render_check::corpus::all().into_iter().find(|c| c.name == "text/latin_sizes").expect("case");
    let scene = frame_scene(&logical_scene(&case, 0), case.scale);
    let size = case.device_size();
    let ink =
        |img: &render_check::metric::Rgba8| img.data.as_chunks::<4>().0.iter().filter(|p| p[0] < 250).count();
    let first = cand.render_rgba(&scene, size, case.background, 1.0).expect("frame 1");
    let (ink1, census1) = (ink(&first), cand.census());
    assert!(ink1 > 1000, "frame 1 ink {ink1}");
    assert!(census1.glyph_entries > 50, "{census1:?}");
    for frame in 2..=300 {
        let img = cand.render_rgba(&scene, size, case.background, 1.0).expect("frame");
        assert_eq!(cand.census(), census1, "census changed at frame {frame}");
        if frame % 50 == 0 || frame == 300 {
            assert_eq!(ink(&img), ink1, "ink changed at frame {frame}");
        }
    }
}
