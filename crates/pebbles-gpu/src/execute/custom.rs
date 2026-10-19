//! Custom GPU primitives (02 §11.2, R60, DR-22): a [`CustomRenderer`] registered per
//! [`CustomPaint`] type draws `Op::Custom` in painter's order. The renderer ends its current
//! pass before the op, hands the custom renderer the pass's color attachment, then reopens
//! the pass with `LoadOp::Load`; ops never merge or reorder around it.

use std::any::TypeId;
use std::collections::HashMap;

use pebbles_render::paint::display_list::CustomPaint;

/// Draws one [`CustomPaint`] type. Output follows the color contract: premultiplied,
/// sRGB-encoded values into the Unorm target (02 §3); WebGL2 rules apply to every pipeline it
/// builds (08 §11). Only the scissor clip applies to custom output (SDF and mask clips do not);
/// a layer containing a custom op is always rendered offscreen, so group opacity applies.
pub trait CustomRenderer: Send + 'static {
    /// CPU + upload work for this frame (no render pass open). Called once per op, in order,
    /// before any of the frame's passes.
    fn prepare(&mut self, ctx: &mut CustomPrepare<'_>, paint: &dyn CustomPaint);
    /// Record drawing. The renderer has ended the current pass; begin your own pass on
    /// `ctx.target` with `LoadOp::Load`, honor `ctx.scissor`, and end it before returning.
    fn render(&mut self, ctx: &mut CustomRender<'_>, paint: &dyn CustomPaint);
}

/// Context of [`CustomRenderer::prepare`].
#[non_exhaustive]
pub struct CustomPrepare<'a> {
    /// The renderer's device.
    pub device: &'a wgpu::Device,
    /// The renderer's queue (uploads land before the frame's passes).
    pub queue: &'a wgpu::Queue,
}

/// Context of [`CustomRenderer::render`].
#[non_exhaustive]
pub struct CustomRender<'a> {
    /// The renderer's device.
    pub device: &'a wgpu::Device,
    /// The renderer's queue.
    pub queue: &'a wgpu::Queue,
    /// The frame's encoder, with no pass open.
    pub encoder: &'a mut wgpu::CommandEncoder,
    /// The color attachment of the interrupted pass (multisampled when `sample_count > 1`;
    /// the renderer resolves it when it reopens the pass).
    pub target: &'a wgpu::TextureView,
    /// Format of `target`.
    pub format: wgpu::TextureFormat,
    /// Sample count of `target`.
    pub sample_count: u32,
    /// Size of `target` in px.
    pub size: (u32, u32),
    /// Device-space clip of the op in `target` px: (x, y, width, height), never empty (the
    /// current ClipState scissor ∩ the op's device bounds).
    pub scissor: (u32, u32, u32, u32),
    /// Local → `target` px transform of the op.
    pub transform: kurbo::Affine,
    /// Present only when the renderer was registered with a depth request (R62): matches
    /// `target`'s size and sample count, cleared to 1.0; begin your pass with `LoadOp::Load`.
    pub depth: Option<&'a wgpu::TextureView>,
}

/// The registered custom renderers, by paint type.
#[derive(Default)]
pub(crate) struct CustomRegistry {
    renderers: HashMap<TypeId, CustomEntry>,
}

/// One registered renderer and its depth request (R62).
pub(crate) struct CustomEntry {
    pub renderer: Box<dyn CustomRenderer>,
    pub depth: Option<wgpu::TextureFormat>,
}

impl CustomRegistry {
    pub(crate) fn insert(
        &mut self,
        ty: TypeId,
        renderer: Box<dyn CustomRenderer>,
        depth: Option<wgpu::TextureFormat>,
    ) {
        self.renderers.insert(ty, CustomEntry { renderer, depth });
    }

    pub(crate) fn get_mut(&mut self, paint: &dyn CustomPaint) -> Option<&mut CustomEntry> {
        self.renderers.get_mut(&paint.as_any().type_id())
    }
}
