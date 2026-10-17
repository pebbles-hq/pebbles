//! Transitional vello-hybrid smoke renderer (§6.7). **Smoke only (DR-21)**: never a
//! correctness reference; judged only by the Smoke metric and deleted at Phase 7 (V43).

use std::collections::HashMap;

use pebbles_render::paint::Scene;
use vello_hybrid::{RenderSize, RenderTargetConfig, Renderer, Resources, TextureBindings, TextureId};

use crate::CheckError;
use crate::readback::{read_texture_rgba, unpremultiply_in_place};

/// vello-hybrid on the candidate's device and queue.
pub struct HybridReference {
    device: wgpu::Device,
    queue: wgpu::Queue,
    inner: Option<(Renderer, Resources)>,
    images: HashMap<u64, wgpu::Texture>,
}

impl HybridReference {
    /// Share `device` / `queue` (clones of the candidate's).
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self { device, queue, inner: None, images: HashMap::new() }
    }

    /// Render `scene` (already in device px) over an opaque `background`; returns
    /// unpremultiplied RGBA8 like the candidate.
    pub fn render_rgba(
        &mut self,
        scene: &Scene,
        width: u32,
        height: u32,
        background: peniko::Color,
    ) -> Result<Vec<u8>, CheckError> {
        if width == 0 || height == 0 || width > u32::from(u16::MAX) || height > u32::from(u16::MAX) {
            return Err(CheckError::Size { width, height });
        }
        // `ImageUpload::id` is a pointer identity that a later case may reuse: never cache.
        self.images.clear();
        let (device, queue) = (self.device.clone(), self.queue.clone());
        let target = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("check.smoke.target"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let oom = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let validation = device.push_error_scope(wgpu::ErrorFilter::Validation);

        let (renderer, resources) = self.inner.get_or_insert_with(|| {
            let cfg = RenderTargetConfig { format: wgpu::TextureFormat::Rgba8Unorm, width, height };
            Renderer::new(&device, &cfg)
        });
        let mut hyb = vello_hybrid::Scene::new(width as u16, height as u16);
        hyb.set_paint(background.with_alpha(1.0));
        hyb.fill_rect(&kurbo::Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));
        scene.flush(&mut hyb, resources);

        let mut bindings = TextureBindings::new();
        for (i, up) in scene.image_uploads().iter().enumerate() {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("check.smoke.image"),
                size: wgpu::Extent3d { width: up.width, height: up.height, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &up.rgba8,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(up.width * 4),
                    rows_per_image: Some(up.height),
                },
                wgpu::Extent3d { width: up.width, height: up.height, depth_or_array_layers: 1 },
            );
            bindings
                .insert(TextureId(i as u64), texture.create_view(&wgpu::TextureViewDescriptor::default()));
            self.images.insert(i as u64, texture);
        }

        let mut encoder = device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("check.smoke.render") });
        let rendered = renderer
            .render(
                &hyb,
                resources,
                &device,
                &queue,
                &mut encoder,
                &RenderSize { width, height },
                &view,
                &bindings,
            )
            .map_err(|e| CheckError::ReferenceGpu(format!("vello_hybrid render: {e:?}")));
        queue.submit([encoder.finish()]);
        let pixels = rendered.and_then(|()| read_texture_rgba(&device, &queue, &target));

        // Pop LIFO: Validation, then OutOfMemory. No allow-list (V19 has proven none).
        let errors = [pollster::block_on(validation.pop()), pollster::block_on(oom.pop())];
        if let Some(e) = errors.into_iter().flatten().next() {
            return Err(CheckError::ReferenceGpu(e.to_string()));
        }
        let mut pixels = pixels?;
        unpremultiply_in_place(&mut pixels);
        Ok(pixels)
    }
}
