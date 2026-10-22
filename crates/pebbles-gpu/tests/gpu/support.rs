//! Shared helpers for pebbles-gpu GPU integration tests (05 §4, V01).
#![allow(dead_code)] // each test module uses a subset

use std::ops::{Deref, DerefMut};

use pebbles_gpu::{GpuInit, OffscreenRenderer, backend_error_count, install_log_bridge};

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

/// A test's renderer. On drop it fails the test if wgpu/naga logged an error-level record
/// while it lived (backend validation layers report there, not through the error scope; S56).
/// GPU lanes run `--test-threads=1`, so the process-global count is per test.
pub struct GpuTest {
    renderer: OffscreenRenderer,
    test: String,
    errors_at_start: u64,
}

impl Deref for GpuTest {
    type Target = OffscreenRenderer;
    fn deref(&self) -> &OffscreenRenderer {
        &self.renderer
    }
}

impl DerefMut for GpuTest {
    fn deref_mut(&mut self) -> &mut OffscreenRenderer {
        &mut self.renderer
    }
}

impl Drop for GpuTest {
    fn drop(&mut self) {
        let errors = backend_error_count() - self.errors_at_start;
        if !std::thread::panicking() {
            assert_eq!(
                errors, 0,
                "{}: the GPU backend logged {errors} error(s); see the log above",
                self.test
            );
        }
    }
}

/// A headless renderer, or `None` after printing `SKIP <test>: <reason>`.
pub fn gpu_or_skip(test: &str) -> Option<GpuTest> {
    install_log_bridge();
    let errors_at_start = backend_error_count();
    match OffscreenRenderer::with_init(strict_init()) {
        Ok(renderer) => {
            eprintln!("GPU {test}: {}", adapter_summary(&renderer));
            Some(GpuTest { renderer, test: test.to_owned(), errors_at_start })
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
