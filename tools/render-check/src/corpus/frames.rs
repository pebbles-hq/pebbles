//! frames/ — Slice Robustness, SelfConsistency (§7.2, V26): byte stability across frames and
//! against a fresh renderer (atlas exhaustion C09, image churn C13, FrameTargets staleness C14).

use std::sync::OnceLock;

use pebbles_core::IntoWidget as _;
use pebbles_render::paint::{Painter, kurbo, peniko};
use pebbles_testing::Harness;

use kurbo::Affine;

use crate::case::{Case, Category::SelfConsistency, Slice::Robustness, frame_ops, harness, ops};
use crate::probe::noise_rgba8;

pub fn cases() -> Vec<Case> {
    let mut v = vec![
        harness("frames/text_300_frames", SelfConsistency, Robustness, 640, 480, dense_paragraph).frames(300),
        frame_ops("frames/image_churn_100", Robustness, 400, 80, 1, image_churn).last_vs_fresh(101),
    ];
    // Device size 800+k × 600, one case per size on the shared renderer, each compared with
    // a fresh renderer.
    for k in 0..10u32 {
        let mut c = ops("frames/resize_sequence", SelfConsistency, Robustness, 800 + k, 600, resize_scene)
            .last_vs_fresh(1);
        c.name = format!("frames/resize_sequence_{k}");
        v.push(c);
    }
    let mut dash =
        ops("frames/dashboard_300_frames", SelfConsistency, Robustness, 1200, 800, super::screen::dashboard)
            .frames(300);
    dash.scale = 0.5;
    v.push(dash);
    v
}

fn dense_paragraph(h: &mut Harness) {
    let para = "The quick brown fox jumps over the lazy dog. Pack my box with five dozen liquor jugs. \
                Sphinx of black quartz, judge my vow. How vexingly quick daft zebras jump! ";
    h.mount_widget(pebbles_widgets::text(para.repeat(8)).into_widget());
}

/// 50 distinct images with stable identities (the same `Blob`s every frame).
fn images() -> &'static [peniko::ImageBrush] {
    static IMAGES: OnceLock<Vec<peniko::ImageBrush>> = OnceLock::new();
    IMAGES.get_or_init(|| {
        (0..50u64)
            .map(|i| {
                peniko::ImageBrush::new(peniko::ImageData {
                    data: peniko::Blob::from(noise_rgba8(16, 16, 100 + i)),
                    format: peniko::ImageFormat::Rgba8,
                    alpha_type: peniko::ImageAlphaType::Alpha,
                    width: 16,
                    height: 16,
                })
            })
            .collect()
    })
}

/// Frame k draws 6 of the 50 images: `(k*7 + i) % 50`.
fn image_churn(p: &mut Painter<'_>, k: u32) {
    for i in 0..6u32 {
        let img = &images()[((k * 7 + i) % 50) as usize];
        p.draw_image(img, Affine::translate((f64::from(8 + i * 64), 8.0)) * Affine::scale(3.0));
    }
}

fn resize_scene(p: &mut Painter<'_>) {
    super::screen::dashboard(p);
}
