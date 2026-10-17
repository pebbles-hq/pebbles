//! Target-size admission (S07): a zero-size target is a no-op, an over-limit one a typed error.

use crate::limits::RendererLimits;

/// What a render entry point does with a target of a given size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TargetCheck {
    /// A side is 0: nothing to draw; return `Ok` without creating an encoder.
    Skip,
    /// Renderable.
    Ok,
    /// A side exceeds the device texture limit.
    TooLarge { width: u32, height: u32, max: u32 },
}

/// Classify a `w`×`h` target against the device limits.
pub(crate) fn check_target_size(w: u32, h: u32, limits: &RendererLimits) -> TargetCheck {
    let max = limits.max_texture_dim;
    if w == 0 || h == 0 {
        TargetCheck::Skip
    } else if w > max || h > max {
        TargetCheck::TooLarge { width: w, height: h, max }
    } else {
        TargetCheck::Ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> RendererLimits {
        RendererLimits::from_device_limits(&wgpu::Limits::downlevel_webgl2_defaults())
    }

    #[test]
    fn zero_size_is_skip() {
        assert_eq!(check_target_size(0, 10, &limits()), TargetCheck::Skip);
        assert_eq!(check_target_size(10, 0, &limits()), TargetCheck::Skip);
    }

    #[test]
    fn over_limit_is_too_large() {
        let max = limits().max_texture_dim;
        assert_eq!(
            check_target_size(max + 1, 1, &limits()),
            TargetCheck::TooLarge { width: max + 1, height: 1, max }
        );
        assert_eq!(check_target_size(max, max, &limits()), TargetCheck::Ok);
    }
}
