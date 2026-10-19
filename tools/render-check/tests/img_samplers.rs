//! 02 R30 AC2: one renderer drawing the whole `img/` slice creates at most 18 samplers
//! (10 §3.5 bound). Needs a GPU.
#![cfg(debug_assertions)]

use render_check::candidate::Candidate;
use render_check::scene::{frame_scene, logical_scene};

#[test]
fn img_slice_creates_at_most_18_samplers() {
    let mut cand = match Candidate::new() {
        Ok(c) => c,
        Err(e) if std::env::var_os("PEBBLES_REQUIRE_GPU").is_none() => {
            eprintln!("skipping: no GPU ({e})");
            return;
        }
        Err(e) => panic!("PEBBLES_REQUIRE_GPU set but no GPU: {e}"),
    };
    let cases: Vec<_> =
        render_check::corpus::all().into_iter().filter(|c| c.name.starts_with("img/")).collect();
    assert!(cases.len() > 10);
    for case in &cases {
        let scene = frame_scene(&logical_scene(case, 0), case.scale);
        cand.render_rgba(&scene, case.device_size(), case.background, 1.0).expect(&case.name);
    }
    let n = cand.census().samplers;
    assert!(n <= 18, "{n} samplers over the img/ slice");
}
