//! The [`Globals`] uniform: pass-target size/origin plus color flags, one dynamic-offset slot per
//! pass target, staged in one reused buffer and uploaded once per frame (10 §2.6, M08).

/// The shared uniform for every pipeline (group 0, binding 0). 48 bytes; one slot per pass
/// target (slot 0 = the root, then layers, Tier C runs and GPU masks).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Globals {
    pub target_size: [f32; 2],
    pub target_origin: [f32; 2],
    pub inv_target_size: [f32; 2],
    pub color_flags: u32,
    /// Grayscale text enhanced contrast (GPUI `grayscale_enhanced_contrast`; 0 = off).
    pub text_contrast: f32,
    /// Text gamma-correction ratios ([`text_gamma_ratios`]); all zero = gamma 1.0 (off).
    pub text_gamma: [f32; 4],
}

const _: () = assert!(size_of::<Globals>() == GLOBALS_SIZE as usize);
const _: () = assert!(core::mem::offset_of!(Globals, text_gamma) == 32);

/// Byte size of [`Globals`] (the binding window and layout minimum).
pub(crate) const GLOBALS_SIZE: u64 = 48;

/// Default text gamma (GPUI / Zed `ZED_FONTS_GAMMA`, DirectWrite's default 1.8).
pub const DEFAULT_TEXT_GAMMA: f32 = 1.8;
/// Default grayscale enhanced contrast (GPUI / Zed `ZED_FONTS_GRAYSCALE_ENHANCED_CONTRAST`).
pub const DEFAULT_TEXT_CONTRAST: f32 = 1.0;

/// The alpha-correction ratios for text `gamma` (clamped to 1.0..=2.2, nearest 0.1): the
/// DirectWrite / Windows Terminal "gamma-incorrect target" table that GPUI also uses (MIT,
/// microsoft/terminal `dwrite.hlsl`), normalised to coverage in 0..=1. Gamma 1.0 → all zero.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // clamped to 10..=22
pub fn text_gamma_ratios(gamma: f32) -> [f32; 4] {
    const RATIOS: [[f32; 4]; 13] = [
        [0.0000, 0.0000, 0.0000, 0.0000], // 1.0
        [0.0166, -0.0807, 0.2227, -0.0751],
        [0.0350, -0.1760, 0.4325, -0.1370],
        [0.0543, -0.2821, 0.6302, -0.1876],
        [0.0739, -0.3963, 0.8167, -0.2287],
        [0.0933, -0.5161, 0.9926, -0.2616],
        [0.1121, -0.6395, 1.1588, -0.2877],
        [0.1300, -0.7649, 1.3159, -0.3080],
        [0.1469, -0.8911, 1.4644, -0.3234], // 1.8
        [0.1627, -1.0170, 1.6051, -0.3347],
        [0.1773, -1.1420, 1.7385, -0.3426],
        [0.1908, -1.2652, 1.8650, -0.3476],
        [0.2031, -1.3864, 1.9851, -0.3501], // 2.2
    ];
    const NORM13: f32 = (65536.0 / (255.0 * 255.0) * 4.0) as f32;
    const NORM24: f32 = (256.0 / 255.0 * 4.0) as f32;
    let g = if gamma.is_finite() { gamma } else { DEFAULT_TEXT_GAMMA };
    let r = RATIOS[((g * 10.0).round().clamp(10.0, 22.0) as usize) - 10];
    [r[0] / 4.0 * NORM13, r[1] / 4.0 * NORM24, r[2] / 4.0 * NORM13, r[3] / 4.0 * NORM24]
}

/// CPU mirror of the glyph shader's `text_coverage` (GPUI `apply_contrast_and_gamma_correction`):
/// coverage `a` of a run whose straight sRGB color is `rgb`.
pub fn text_coverage(a: f32, rgb: [f32; 3], contrast: f32, ratios: [f32; 4]) -> f32 {
    let b = 0.30 * rgb[0] + 0.59 * rgb[1] + 0.11 * rgb[2];
    let k = contrast * (4.0 * (0.75 - b)).clamp(0.0, 1.0);
    let a = a * (k + 1.0) / (a * k + 1.0);
    let correction = (ratios[0] * b + ratios[1]) * a + (ratios[2] * b + ratios[3]);
    (a + a * (1.0 - a) * correction).clamp(0.0, 1.0)
}
const _: () = assert!(core::mem::offset_of!(Globals, inv_target_size) == 16);
const _: () = assert!(core::mem::offset_of!(Globals, color_flags) == 24);

/// Bit 0 of `Globals::color_flags`: decode sRGB output for the GL sRGB-only fallback (DR-05).
pub(crate) const COLOR_FLAG_DECODE_SRGB_OUTPUT: u32 = 1;

/// The frame's Globals slots (or any other per-slot uniform records, M33), staged for ONE
/// upload (10 §2.6): slot `i` lives at `i × stride`; the gaps stay zero. The `Vec` is reused across frames (cleared, capacity kept).
#[derive(Debug, Default)]
pub(crate) struct UniformSlots {
    staging: Vec<u8>,
    stride: u64,
    count: u32,
}

impl UniformSlots {
    /// Start a frame with slots `stride` bytes apart.
    pub(crate) fn begin(&mut self, stride: u64) {
        self.staging.clear();
        self.stride = stride;
        self.count = 0;
    }

    /// Append the next slot; returns its index.
    pub(crate) fn push<T: bytemuck::Pod>(&mut self, g: &T) -> u32 {
        let slot = self.count;
        self.set(slot, g);
        slot
    }

    /// Write slot `slot` (zero-filling any gap before it).
    pub(crate) fn set<T: bytemuck::Pod>(&mut self, slot: u32, g: &T) {
        let Ok(at) = usize::try_from(u64::from(slot) * self.stride) else { return };
        let end = at + size_of::<T>();
        if self.staging.len() < end {
            self.staging.resize(end, 0);
        }
        self.staging[at..end].copy_from_slice(bytemuck::bytes_of(g));
        self.count = self.count.max(slot + 1);
    }

    /// The staged bytes (`(count − 1) × stride + size_of` the last slot long).
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.staging
    }

    /// M33: stage the slots for `buffer` (`label`, recreated → new `generation`) at offset 0 as
    /// ONE region of the frame's upload, skipped when the buffer already holds them.
    pub(crate) fn write(
        &self,
        uploads: &mut super::uploads::FrameUploads,
        label: &'static str,
        generation: u64,
        queue: &wgpu::Queue,
        buffer: &wgpu::Buffer,
    ) {
        uploads.write_whole(label, generation, queue, buffer, &self.staging);
    }
}

#[cfg(test)]
mod tests {
    use super::{Globals, UniformSlots, text_coverage, text_gamma_ratios};

    #[test]
    fn gamma_ratios_match_gpui_table() {
        assert_eq!(text_gamma_ratios(1.0), [0.0; 4]);
        let r = text_gamma_ratios(1.8);
        let norm13 = 65536.0 / (255.0 * 255.0) * 4.0;
        assert!((f64::from(r[0]) - 0.1469 / 4.0 * norm13).abs() < 1e-6);
        assert!((f64::from(r[1]) - -0.8911 / 4.0 * (256.0 / 255.0 * 4.0)).abs() < 1e-6);
        // Clamped and rounded to the table.
        assert_eq!(text_gamma_ratios(9.0), text_gamma_ratios(2.2));
        assert_eq!(text_gamma_ratios(0.2), text_gamma_ratios(1.0));
        assert_eq!(text_gamma_ratios(1.84), r);
    }

    #[test]
    fn gamma_curve_shape() {
        let r = text_gamma_ratios(1.8);
        for rgb in [[0.0; 3], [1.0; 3], [0.83; 3]] {
            // Endpoints are fixed; the curve is monotonic.
            assert_eq!(text_coverage(0.0, rgb, 1.0, r), 0.0);
            assert!((text_coverage(1.0, rgb, 1.0, r) - 1.0).abs() < 1e-6);
            let mut prev = 0.0;
            for i in 1..=100 {
                let v = text_coverage(i as f32 / 100.0, rgb, 1.0, r);
                assert!(v >= prev, "monotonic at {i} for {rgb:?}");
                prev = v;
            }
        }
        // Light-on-dark text is thickened (enhanced contrast + gamma), dark text less so.
        let light = text_coverage(0.5, [0.83; 3], 1.0, r);
        let dark = text_coverage(0.5, [0.14; 3], 1.0, r);
        assert!(light > 0.55, "light text {light}");
        assert!(light > dark, "light {light} vs dark {dark}");
        // Off: identity.
        assert_eq!(text_coverage(0.37, [0.5; 3], 0.0, text_gamma_ratios(1.0)), 0.37);
    }

    #[test]
    fn slots_are_stride_aligned_and_zero_padded() {
        let stride = 256u64;
        let su = 256usize;
        let g = |v: f32| Globals {
            target_size: [v, v],
            target_origin: [0.0, 1.0],
            inv_target_size: [1.0 / v, 1.0 / v],
            color_flags: 1,
            text_contrast: 0.5,
            text_gamma: [1.0, 2.0, 3.0, 4.0],
        };
        let mut s = UniformSlots::default();
        s.begin(stride);
        let slots = [s.push(&g(1.0)), s.push(&g(2.0)), s.push(&g(3.0))];
        assert_eq!(slots, [0, 1, 2]);
        let b = s.bytes();
        assert_eq!(b.len() as u64, 2 * stride + 48);
        for (i, v) in [1.0, 2.0, 3.0].into_iter().enumerate() {
            let at = i * su;
            assert_eq!(&b[at..at + 48], bytemuck::bytes_of(&g(v)));
            if i < 2 {
                assert!(b[at + 48..at + su].iter().all(|&x| x == 0), "gap {i} is zero");
            }
        }
        // A new frame reuses the buffer.
        s.begin(stride);
        assert!(s.bytes().is_empty());
    }

    /// Byte size of each WGSL scalar/vector type used by `Globals`.
    fn wgsl_size(ty: &str) -> usize {
        match ty {
            "vec2<f32>" => 8,
            "vec4<f32>" => 16,
            "u32" | "f32" => 4,
            other => panic!("unexpected WGSL type in Globals: {other}"),
        }
    }

    /// The WGSL `Globals` in prelude.wgsl has the same fields, order and size as the Rust struct.
    #[test]
    fn globals_layout_matches_wgsl() {
        let src = include_str!("../../shaders/prelude.wgsl");
        let body = src.split("struct Globals {").nth(1).and_then(|s| s.split("};").next()).expect("struct");
        let fields: Vec<(&str, &str)> = body
            .lines()
            .filter_map(|l| l.trim().strip_suffix(','))
            .filter_map(|l| l.split_once(':'))
            .map(|(n, t)| (n.trim(), t.trim()))
            .collect();
        let names: Vec<&str> = fields.iter().map(|f| f.0).collect();
        assert_eq!(names, ["target_size", "target_origin", "inv_target_size", "color_flags", "text_contrast", "text_gamma"]);
        let wgsl_bytes: usize = fields.iter().map(|f| wgsl_size(f.1)).sum();
        assert_eq!(wgsl_bytes, size_of::<Globals>());
        assert_eq!(core::mem::offset_of!(Globals, target_origin), 8);
        assert_eq!(core::mem::offset_of!(Globals, text_contrast), 28);
    }
}
