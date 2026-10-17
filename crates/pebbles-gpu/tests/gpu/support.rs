//! Shared helpers for pebbles-gpu GPU integration tests (05 §4, V01).
#![allow(dead_code)] // each test module uses a subset

use pebbles_gpu::{GpuInit, OffscreenRenderer};

/// `PEBBLES_REQUIRE_GPU=1` turns a missing adapter into a failure (GPU lanes set it).
pub fn require_gpu() -> bool {
    std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1")
}

/// The init every GPU test uses: headless and strict (11 §4.6). A test that injects an error on
/// purpose builds its own init with `strict_validation = false`.
pub fn strict_init() -> GpuInit {
    let mut init = GpuInit::headless();
    init.strict_validation = true;
    init
}

/// A headless renderer, or `None` after printing `SKIP <test>: <reason>`.
pub fn gpu_or_skip(test: &str) -> Option<OffscreenRenderer> {
    match OffscreenRenderer::with_init(strict_init()) {
        Ok(r) => {
            eprintln!("GPU {test}: {}", adapter_summary(&r));
            Some(r)
        }
        Err(e) if require_gpu() => panic!("{test}: PEBBLES_REQUIRE_GPU=1 but no GPU adapter: {e}"),
        Err(e) => {
            eprintln!("SKIP {test}: no GPU adapter ({e})");
            None
        }
    }
}

pub fn adapter_summary(r: &OffscreenRenderer) -> String {
    let i = r.adapter_info();
    format!("{} | {:?} | {:?} | {} {}", i.name, i.backend, i.device_type, i.driver, i.driver_info)
}

/// `false` on the noop backend (it renders nothing; only validation runs).
pub fn pixels_meaningful(r: &OffscreenRenderer) -> bool {
    r.adapter_info().backend != wgpu::Backend::Noop
}

pub fn px(rgba: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * width + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

pub fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}
