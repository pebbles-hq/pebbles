//! The ONLY place pebbles-gpu converts between floats and integers or narrows f64 to f32.
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // every cast below is preceded by a clamp / finiteness check

/// f64 device coordinate → f32 vertex component; `None` if not finite after narrowing.
pub(crate) fn to_f32(v: f64) -> Option<f32> {
    if !v.is_finite() {
        return None;
    }
    let f = v as f32;
    f.is_finite().then_some(f)
}

/// f64 → f32 narrowing with NO repair (NaN stays NaN, overflow becomes ±inf): for values that
/// must reach a validator such as lyon unchanged.
pub(crate) fn narrow_f32(v: f64) -> f32 {
    v as f32
}

/// Floor into `0..=max`; `None` for NaN.
pub(crate) fn floor_px(v: f64, max: u32) -> Option<u32> {
    if v.is_nan() {
        return None;
    }
    Some(v.floor().clamp(0.0, f64::from(max)) as u32)
}

/// Ceil into `0..=max`; `None` for NaN.
pub(crate) fn ceil_px(v: f64, max: u32) -> Option<u32> {
    if v.is_nan() {
        return None;
    }
    Some(v.ceil().clamp(0.0, f64::from(max)) as u32)
}

/// `Vec` length → u32 index, saturating (callers enforce the 2^24 vertex cap first).
pub(crate) fn index_u32(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Non-negative f32 → nearest u32; `None` if not finite, negative or above `u32::MAX`.
pub(crate) fn to_u32_round(v: f32) -> Option<u32> {
    let r = v.round();
    (r.is_finite() && r >= 0.0 && r <= u32::MAX as f32).then_some(r as u32)
}
