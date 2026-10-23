//! The candidate renders the smoke rect and the runner carries it end to end (V14).
//! Needs a GPU.

use pebbles_render::paint::{Painter, Scene};
use render_check::Status;
use render_check::candidate::Candidate;
use render_check::run::{RunOptions, Runner};

fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    (0..4).all(|c| a[c].abs_diff(b[c]) <= tol)
}

fn gpu_or_skip<T>(r: Result<T, render_check::CheckError>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(e) if std::env::var_os("PEBBLES_REQUIRE_GPU").is_none() => {
            eprintln!("skipping: no GPU ({e})");
            None
        }
        Err(e) => panic!("PEBBLES_REQUIRE_GPU set but no GPU: {e}"),
    }
}

fn smoke_case() -> render_check::case::Case {
    render_check::case::ops(
        "smoke/solid_rect",
        render_check::case::Category::Exact,
        render_check::case::Slice::P1,
        64,
        64,
        render_check::corpus::solid_rect,
    )
}

#[test]
#[ignore = "gpu"]
fn candidate_renders_solid_rect() {
    let Some(mut c) = gpu_or_skip(Candidate::new()) else { return };
    let mut scene = Scene::new();
    render_check::corpus::solid_rect(&mut Painter::new(&mut scene));
    let img = c.render_rgba(&scene, (64, 64), peniko::Color::WHITE, 1.0).expect("candidate render");
    assert!(near(img.px(32, 32), [0x33, 0x66, 0xFF, 255], 1), "{:?}", img.px(32, 32));
    assert_eq!(img.px(2, 2), [255, 255, 255, 255]);
}

/// V14 AC3: the exact rect passes end to end — judged against the golden model (V44/V47), with
/// the expected candidate pixels.
#[test]
#[ignore = "gpu"]
fn exact_rect_passes_end_to_end() {
    let Some(mut runner) = gpu_or_skip(Runner::new(RunOptions::default())) else { return };
    let out = runner.run_case(&smoke_case());
    assert_eq!(out.result.status, Status::Pass, "{}", out.result.note);
    let img = out.candidate.expect("candidate image");
    assert!(near(img.px(32, 32), [0x33, 0x66, 0xFF, 255], 1));
}

#[test]
#[ignore = "gpu"]
fn panicking_build_is_a_panic_and_the_next_case_runs() {
    let Some(mut runner) = gpu_or_skip(Runner::new(RunOptions::default())) else { return };
    let mut bad = smoke_case();
    bad.name = "smoke/panics".into();
    bad.build = render_check::case::Build::Ops(|_| panic!("deliberate build panic"));
    let out = runner.run_case(&bad);
    assert_eq!(out.result.status, Status::Panic);
    assert!(out.result.note.contains("deliberate build panic"), "{}", out.result.note);
    runner.recreate().expect("recreate");
    assert_eq!(runner.run_case(&smoke_case()).result.status, Status::Pass);
}
