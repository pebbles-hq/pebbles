//! Shader brushes in prepare (09 §6.1, L43): the brush's device → local map, the shape's local
//! size and the zero-filled uniforms; the coverage itself routes by tier like an image brush.

use kurbo::{Affine, Size};
use pebbles_render::paint::ShaderBrush;

use super::num::narrow_f32;
use super::plan::ShaderBrushDraw;
use crate::execute::resources::image_cache::ImageKey;
use crate::gpu_types::ShaderBrushParams;

/// The draw of `brush` painted through `paint_xf` (brush local → device) over a shape whose
/// local bounds are `size`. `None` when `paint_xf` is singular or not finite.
pub(crate) fn draw(brush: &ShaderBrush, paint_xf: Affine, size: Size) -> Option<ShaderBrushDraw> {
    if !paint_xf.is_finite() || paint_xf.determinant().abs() < 1e-12 {
        return None;
    }
    let [a, b, c, d, e, f] = paint_xf.inverse().as_coeffs().map(narrow_f32);
    let mut values = [[0.0f32; 4]; 16];
    for (i, v) in brush.uniforms.iter().take(pebbles_render::paint::MAX_SHADER_UNIFORMS).enumerate() {
        if let Some(slot) = values.get_mut(i / 4).and_then(|r| r.get_mut(i % 4)) {
            *slot = if v.is_finite() { *v } else { 0.0 };
        }
    }
    let params = ShaderBrushParams {
        inv0: [a, c, e, 0.0],
        inv1: [b, d, f, 0.0],
        size: [narrow_f32(size.width), narrow_f32(size.height), 0.0, 0.0],
        values,
    };
    let images: Vec<_> = brush
        .images
        .iter()
        .take(pebbles_render::paint::MAX_SHADER_IMAGES)
        .filter(|i| i.image.width > 0 && i.image.height > 0)
        .map(|i| (i.image.clone(), ImageKey::of(&i.image)))
        .collect();
    let sampler =
        super::images::sampler_key(brush.images.first().map_or_else(Default::default, |i| i.sampler));
    Some(ShaderBrushDraw { program: brush.program.clone(), params, images, sampler })
}
