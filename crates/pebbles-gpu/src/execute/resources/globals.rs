//! The single [`Globals`] uniform: pass-target size/origin plus color flags (10 §2.6).

/// The shared uniform for every pipeline (group 0, binding 0). 32 bytes, dynamic-offset
/// slots start at M08; C12 lands one slot.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Globals {
    pub target_size: [f32; 2],
    pub target_origin: [f32; 2],
    pub inv_target_size: [f32; 2],
    pub color_flags: u32,
    pub _pad0: u32,
}

const _: () = assert!(std::mem::size_of::<Globals>() == 32);
const _: () = assert!(std::mem::offset_of!(Globals, inv_target_size) == 16);
const _: () = assert!(std::mem::offset_of!(Globals, color_flags) == 24);

/// Bit 0 of `Globals::color_flags`: decode sRGB output for the GL sRGB-only fallback (DR-05).
pub(crate) const COLOR_FLAG_DECODE_SRGB_OUTPUT: u32 = 1;
