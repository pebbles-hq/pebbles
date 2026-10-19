//! GPU coverage masks (09 §2.9, L09): clip paths too large or too volatile for a CPU mask are
//! filled (lyon mesh) into a pooled R8 target before the frame's first pass, then sampled by
//! every content shader exactly like a Tier B mask.

use super::plan::ScissorRect;

/// `GpuBudgets::gpu_mask_bytes` native default (10 §8).
pub(crate) const DEFAULT_BUDGET_BYTES: u64 = 16 * 1024 * 1024;

/// Pool bytes one job holds: its 4× MSAA render target plus the single-sample resolve.
pub(crate) fn job_bytes(rect: ScissorRect) -> u64 {
    u64::from(rect.width()) * u64::from(rect.height()) * 5
}
