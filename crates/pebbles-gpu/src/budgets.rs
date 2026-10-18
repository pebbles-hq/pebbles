//! Resource budgets (10 §8, M03): the §8.2 table is the only definition of these names.
//! Built once per Renderer from defaults + `PEBBLES_GPU_*` env overrides; tests construct it
//! directly (fields set that way are never clamped).

use pebbles_foundation::log::{Cat, Level, logf};

const MIB: u64 = 1 << 20;

/// Resource budgets for one Renderer. Built from defaults + env overrides by
/// `GpuBudgets::from_env`; tests build it directly for determinism.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GpuBudgets {
    /// Image cache byte budget (M02).
    pub image_cache_bytes: u64,
    /// Glyph atlas page side cap, a power of two (M01).
    pub glyph_atlas_max_side: u32,
    /// Mask atlas array layers (M01).
    pub glyph_mask_max_layers: u32,
    /// Color atlas array layers (M01).
    pub glyph_color_max_layers: u32,
    /// Byte budget for Tier B path masks and clip masks in the coverage atlas (DR-20).
    pub path_mask_bytes: u64,
    /// GPU clip-mask budget (L09).
    pub gpu_mask_bytes: u64,
    /// Retained-layer budget (M29).
    pub retained_bytes: u64,
    /// Gradient LUT rows (L34).
    pub gradient_lut_max_rows: u32,
    /// Per-target texture budget (M13).
    pub frame_targets_bytes: u64,
    /// Per-pool layer budget (M16, 09 §3.8).
    pub layer_pool_bytes: u64,
    /// Largest GPU arena (M06, M07).
    pub arena_max_bytes: u64,
}

/// Which default column of §8.2 applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BudgetPlatform {
    /// Desktop and mobile native.
    Native,
    /// wasm32 (WebGPU / WebGL2).
    Wasm,
}

impl BudgetPlatform {
    /// The platform this crate is compiled for.
    pub const fn current() -> Self {
        if cfg!(target_family = "wasm") { BudgetPlatform::Wasm } else { BudgetPlatform::Native }
    }
}

/// Largest power of two `<= v` (`v >= 1`).
fn pow2_floor(v: u32) -> u32 {
    if v == 0 { 0 } else { 1 << (31 - v.leading_zeros()) }
}

impl GpuBudgets {
    /// Defaults for `platform`, clamped to `limits` (the device's REQUESTED limits).
    pub fn defaults(platform: BudgetPlatform, limits: &wgpu::Limits) -> Self {
        let native = platform == BudgetPlatform::Native;
        let pick = |n: u64, w: u64| if native { n } else { w };
        Self {
            image_cache_bytes: pick(256, 128) * MIB,
            glyph_atlas_max_side: pow2_floor(2048.min(limits.max_texture_dimension_2d)),
            glyph_mask_max_layers: 4,
            glyph_color_max_layers: 2,
            path_mask_bytes: pick(8, 4) * MIB,
            gpu_mask_bytes: pick(16, 8) * MIB,
            retained_bytes: pick(8, 4) * MIB,
            gradient_lut_max_rows: 2048.min(limits.max_texture_dimension_2d),
            frame_targets_bytes: pick(192, 96) * MIB,
            layer_pool_bytes: pick(96, 48) * MIB,
            arena_max_bytes: (16 * MIB).min(limits.max_buffer_size),
        }
    }

    /// Defaults for the current platform + `PEBBLES_GPU_*` env overrides (read once per
    /// Renderer); an unparsable or out-of-range value logs one warning and is replaced.
    pub fn from_env(limits: &wgpu::Limits) -> Self {
        let mut b = Self::defaults(BudgetPlatform::current(), limits);
        let env = |name: &str| std::env::var(name).ok();
        let read = |name: &str, default: u64, min: u64, max: u64| -> u64 {
            let raw = env(name);
            let (v, warn) = parse_budget(raw.as_deref(), default, min, max);
            if warn {
                let raw = raw.unwrap_or_default();
                logf(Level::Warn, Cat::Gpu, format_args!("{name}={raw} is not valid; using {v}"));
            }
            v
        };
        let mib = |v: u64| v / MIB;
        let max_tex = u64::from(limits.max_texture_dimension_2d);
        b.image_cache_bytes = read("PEBBLES_GPU_IMAGE_BUDGET_MB", mib(b.image_cache_bytes), 16, 4096) * MIB;
        let side = read(
            "PEBBLES_GPU_GLYPH_ATLAS_MAX_SIDE",
            u64::from(b.glyph_atlas_max_side),
            512,
            4096.min(max_tex),
        );
        b.glyph_atlas_max_side = pow2_floor(u32::try_from(side).unwrap_or(u32::MAX));
        b.glyph_mask_max_layers = u32::try_from(read(
            "PEBBLES_GPU_GLYPH_ATLAS_MAX_LAYERS",
            u64::from(b.glyph_mask_max_layers),
            2,
            5,
        ))
        .unwrap_or(4);
        b.glyph_color_max_layers = u32::try_from(read(
            "PEBBLES_GPU_COLOR_ATLAS_MAX_LAYERS",
            u64::from(b.glyph_color_max_layers),
            2,
            5,
        ))
        .unwrap_or(2);
        let atlas_max = u64::from(b.glyph_atlas_max_side).pow(2) * u64::from(b.glyph_mask_max_layers) / 2;
        b.path_mask_bytes =
            read("PEBBLES_GPU_PATH_MASK_BUDGET_MB", mib(b.path_mask_bytes), 1, mib(atlas_max).max(1)) * MIB;
        b.gpu_mask_bytes = read("PEBBLES_GPU_MASK_BUDGET_MB", mib(b.gpu_mask_bytes), 2, 256) * MIB;
        b.retained_bytes = read("PEBBLES_GPU_RETAINED_BUDGET_MB", mib(b.retained_bytes), 1, 128) * MIB;
        b.gradient_lut_max_rows = u32::try_from(read(
            "PEBBLES_GPU_GRADIENT_LUT_MAX_ROWS",
            u64::from(b.gradient_lut_max_rows),
            16,
            max_tex,
        ))
        .unwrap_or(2048);
        b.frame_targets_bytes =
            read("PEBBLES_GPU_TARGETS_BUDGET_MB", mib(b.frame_targets_bytes), 32, 4096) * MIB;
        b.layer_pool_bytes = read("PEBBLES_GPU_LAYER_POOL_MB", mib(b.layer_pool_bytes), 16, 2048) * MIB;
        b.arena_max_bytes =
            read("PEBBLES_GPU_ARENA_MAX_MB", mib(b.arena_max_bytes), 1, mib(limits.max_buffer_size).max(1))
                * MIB;
        b
    }
}

/// Pure + unit-tested. `raw` = the env value (None = unset). Returns the value to use and
/// whether a warning is due (unparsable or clamped).
pub(crate) fn parse_budget(raw: Option<&str>, default: u64, min: u64, max: u64) -> (u64, bool) {
    let Some(raw) = raw else { return (default, false) };
    match raw.trim().parse::<u64>() {
        Err(_) => (default, true),
        Ok(v) if v < min => (min, true),
        Ok(v) if v > max => (max, true),
        Ok(v) => (v, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_unset_is_default() {
        assert_eq!(parse_budget(None, 256, 16, 4096), (256, false));
    }

    #[test]
    fn parse_garbage_warns_and_defaults() {
        assert_eq!(parse_budget(Some("lots"), 256, 16, 4096), (256, true));
        assert_eq!(parse_budget(Some(" 512 "), 256, 16, 4096), (512, false));
    }

    #[test]
    fn parse_clamps_low_and_high() {
        assert_eq!(parse_budget(Some("1"), 256, 16, 4096), (16, true));
        assert_eq!(parse_budget(Some("99999"), 256, 16, 4096), (4096, true));
    }

    #[test]
    fn wasm_defaults_are_smaller() {
        let b = GpuBudgets::defaults(BudgetPlatform::Wasm, &wgpu::Limits::downlevel_webgl2_defaults());
        assert_eq!(b.image_cache_bytes, 128 << 20);
        assert!(
            b.layer_pool_bytes
                < GpuBudgets::defaults(BudgetPlatform::Native, &wgpu::Limits::default()).layer_pool_bytes
        );
    }

    #[test]
    fn defaults_respect_device_limits() {
        let limits = wgpu::Limits { max_texture_dimension_2d: 1024, ..wgpu::Limits::default() };
        assert_eq!(GpuBudgets::defaults(BudgetPlatform::Native, &limits).glyph_atlas_max_side, 1024);
    }

    #[test]
    fn max_side_rounds_down_to_pow2() {
        // The env path: 3000 parses within 512..=4096, then rounds down to a power of two.
        let (v, warn) = parse_budget(Some("3000"), 2048, 512, 4096);
        assert_eq!((pow2_floor(u32::try_from(v).unwrap()), warn), (2048, false));
    }
}
