//! A butt cap ends exactly at the endpoint (Skia / CSS `stroke-linecap: butt`).

use super::*;
use pebbles_render::paint::display_list::PathHint;

const W: u32 = 32;

fn row(hint: PathHint, x0: f64, x1: f64) -> Option<Vec<u8>> {
    let (device, queue, mut r) = setup("butt_cap")?;
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.set_path_hint(hint);
    let style = Stroke::new(1.0).with_caps(kurbo::Cap::Butt);
    p.stroke(&style, Affine::IDENTITY, Color::BLACK, None, &kurbo::Line::new((x0, 10.5), (x1, 10.5)));
    let px = render_scene(&device, &queue, &mut r, &s, (W, W), Color::WHITE);
    if support::noop_backend() {
        return None;
    }
    Some((0..W).map(|x| 255 - pixel(&px, W, x, 10)[0]).collect())
}

#[test]
#[ignore = "gpu"]
fn one_px_butt_cap_ends_at_endpoint() {
    for hint in [PathHint::Auto, PathHint::Volatile] {
        let Some(cov) = row(hint, 10.0, 20.0) else { return };
        eprintln!("{hint:?}: {cov:?}");
        assert_eq!(cov[9], 0, "{hint:?}: nothing before the start cap: {cov:?}");
        assert_eq!(cov[20], 0, "{hint:?}: nothing past the end cap: {cov:?}");
        assert!(cov[10..20].iter().all(|&c| c == 255), "{hint:?}: body fully covered: {cov:?}");
        let Some(cov) = row(hint, 10.25, 20.5) else { return };
        eprintln!("frac {hint:?}: {cov:?}");
    }
}
