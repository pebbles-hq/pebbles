//! The [`Globals`] uniform: pass-target size/origin plus color flags, one dynamic-offset slot per
//! pass target, staged in one reused buffer and uploaded once per frame (10 §2.6, M08).

/// The shared uniform for every pipeline (group 0, binding 0). 32 bytes; one slot per pass
/// target (slot 0 = the root, then layers, Tier C runs and GPU masks).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Globals {
    pub target_size: [f32; 2],
    pub target_origin: [f32; 2],
    pub inv_target_size: [f32; 2],
    pub color_flags: u32,
    pub _pad0: u32,
}

const _: () = assert!(size_of::<Globals>() == 32);
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
    use super::{Globals, UniformSlots};

    #[test]
    fn slots_are_stride_aligned_and_zero_padded() {
        let stride = 256u64;
        let su = 256usize;
        let g = |v: f32| Globals {
            target_size: [v, v],
            target_origin: [0.0, 1.0],
            inv_target_size: [1.0 / v, 1.0 / v],
            color_flags: 1,
            _pad0: 0,
        };
        let mut s = UniformSlots::default();
        s.begin(stride);
        let slots = [s.push(&g(1.0)), s.push(&g(2.0)), s.push(&g(3.0))];
        assert_eq!(slots, [0, 1, 2]);
        let b = s.bytes();
        assert_eq!(b.len() as u64, 2 * stride + 32);
        for (i, v) in [1.0, 2.0, 3.0].into_iter().enumerate() {
            let at = i * su;
            assert_eq!(&b[at..at + 32], bytemuck::bytes_of(&g(v)));
            if i < 2 {
                assert!(b[at + 32..at + su].iter().all(|&x| x == 0), "gap {i} is zero");
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
        assert_eq!(names, ["target_size", "target_origin", "inv_target_size", "color_flags", "_pad0"]);
        let wgsl_bytes: usize = fields.iter().map(|f| wgsl_size(f.1)).sum();
        assert_eq!(wgsl_bytes, size_of::<Globals>());
        assert_eq!(core::mem::offset_of!(Globals, target_origin), 8);
        assert_eq!(core::mem::offset_of!(Globals, _pad0), 28);
    }
}
