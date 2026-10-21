//! Shared helpers for the shell's GPU tests (05 §4.1): analytic pixel probes and a typed
//! skip when the machine has no adapter (DR-12).

#![allow(dead_code)]

use pebbles_core::AnyWidget;
use pebbles_foundation::palette;
use pebbles_gpu::{CaptureError, GpuError};
use pebbles_shell::capture;

/// `PEBBLES_REQUIRE_GPU=1` turns a missing adapter into a failure instead of a skip.
pub fn require_gpu() -> bool {
    std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1")
}

/// The RGBA pixel at (`x`, `y`) of a `w`-wide frame.
pub fn px(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

/// Every channel of `a` is within `tol` of `b`.
pub fn near(a: [u8; 4], b: [u8; 4], tol: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= tol)
}

/// Rec. 709 luma in 0..=255.
pub fn luma(p: [u8; 4]) -> u32 {
    (2126 * u32::from(p[0]) + 7152 * u32::from(p[1]) + 722 * u32::from(p[2])) / 10000
}

/// Capture `root` at `w`×`h` on white; `None` (after printing `SKIP`) when there is no adapter.
pub fn capture_or_skip(test: &str, root: AnyWidget, w: u32, h: u32) -> Option<Vec<u8>> {
    capture_on_or_skip(test, root, w, h, palette::WHITE)
}

/// [`capture_or_skip`] on an explicit background.
pub fn capture_on_or_skip(
    test: &str,
    root: AnyWidget,
    w: u32,
    h: u32,
    background: pebbles_foundation::Color,
) -> Option<Vec<u8>> {
    match capture::capture_rgba(root, w, h, background) {
        Ok(rgba) => Some(rgba),
        Err(e) => match e.downcast_ref::<CaptureError>() {
            Some(CaptureError::Gpu(GpuError::NoAdapter { .. })) if !require_gpu() => {
                println!("SKIP {test}: {e}");
                None
            }
            _ => panic!("{test}: capture failed: {e}"),
        },
    }
}
