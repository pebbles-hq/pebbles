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

const _: () = assert!(size_of::<Globals>() == 32);
const _: () = assert!(core::mem::offset_of!(Globals, inv_target_size) == 16);
const _: () = assert!(core::mem::offset_of!(Globals, color_flags) == 24);

/// Bit 0 of `Globals::color_flags`: decode sRGB output for the GL sRGB-only fallback (DR-05).
pub(crate) const COLOR_FLAG_DECODE_SRGB_OUTPUT: u32 = 1;

#[cfg(test)]
mod tests {
    use super::Globals;

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
