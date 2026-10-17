//! The transitional smoke reference renders the smoke rect (V13). Needs a GPU.

use pebbles_render::paint::{Painter, Scene};
use render_check::reference::HybridReference;

fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    (0..4).all(|c| a[c].abs_diff(b[c]) <= tol)
}

#[test]
#[ignore = "gpu"]
fn reference_renders_solid_rect() {
    let gpu = match pebbles_gpu::OffscreenRenderer::with_init(pebbles_gpu::GpuInit::from_env()) {
        Ok(g) => g,
        Err(e) if std::env::var_os("PEBBLES_REQUIRE_GPU").is_none() => {
            eprintln!("skipping: no GPU ({e})");
            return;
        }
        Err(e) => panic!("PEBBLES_REQUIRE_GPU set but no GPU: {e}"),
    };
    let mut scene = Scene::new();
    render_check::corpus::solid_rect(&mut Painter::new(&mut scene));
    let mut reference = HybridReference::new(gpu.device().clone(), gpu.queue().clone());
    let px = reference.render_rgba(&scene, 64, 64, peniko::Color::WHITE).expect("smoke render");
    let at = |x: usize, y: usize| {
        let i = (y * 64 + x) * 4;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    };
    assert!(near(at(32, 32), [0x33, 0x66, 0xFF, 255], 1), "{:?}", at(32, 32));
    assert_eq!(at(2, 2), [255, 255, 255, 255]);
}
