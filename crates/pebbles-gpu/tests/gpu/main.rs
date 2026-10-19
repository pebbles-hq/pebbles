//! GPU integration tests (pebbles-gpu), one binary (`--test gpu`) with topic modules. Each is `#[ignore = "gpu"]` and skips gracefully
//! without an adapter (DR-15). V01's harness replaces the test-local helpers after C15.

pub(crate) use kurbo::{Affine, Rect, Stroke};
pub(crate) use pebbles_render::paint::{Glyph, Painter, Scene};
pub(crate) use peniko::{Brush, Color, Fill};

pub(crate) use pebbles_gpu::{FrameParams, FrameTargets, GpuContext, GpuInit, RenderTarget, Renderer};

mod atlas_gpu;
mod capture;
mod clip;
mod color;
mod frame_targets_gpu;
mod gpu_color;
mod gpu_geometry;
mod gpu_smoke;
mod image_cache_gpu;
mod images;
mod layers;
mod order;
mod path_masks;
mod pixel_snap;
mod shadows;
mod steady_state;
mod support;
mod targets;
mod text;
mod tier_c;

/// Acquire a headless device + renderer, printing a skip note and returning `None` without an adapter.
pub(crate) fn setup(name: &str) -> Option<(wgpu::Device, wgpu::Queue, Renderer)> {
    let mut ctx = pollster::block_on(GpuContext::new(&GpuInit::headless()));
    let id = match pollster::block_on(ctx.request_device(None)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("SKIP {name}: {e}");
            return None;
        }
    };
    let gpu = ctx.device(id).expect("device just requested");
    let device = gpu.device().clone();
    let queue = gpu.queue().clone();
    let renderer = Renderer::new(gpu, id);
    Some((device, queue, renderer))
}

/// `setup` with explicit budgets (M01 tests force tiny atlases).
pub(crate) fn setup_with_budgets(
    name: &str,
    tweak: impl FnOnce(&mut pebbles_gpu::GpuBudgets),
) -> Option<(wgpu::Device, wgpu::Queue, Renderer)> {
    let mut ctx = pollster::block_on(GpuContext::new(&GpuInit::headless()));
    let id = match pollster::block_on(ctx.request_device(None)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("SKIP {name}: {e}");
            return None;
        }
    };
    let gpu = ctx.device(id).expect("device just requested");
    let mut budgets =
        pebbles_gpu::GpuBudgets::defaults(pebbles_gpu::BudgetPlatform::Native, &gpu.device().limits());
    tweak(&mut budgets);
    let (device, queue) = (gpu.device().clone(), gpu.queue().clone());
    Some((device, queue, Renderer::with_budgets(gpu, id, budgets)))
}

/// Render `scene` into a fresh `size` target and read the pixels back.
pub(crate) fn render_scene(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    scene: &Scene,
    size: (u32, u32),
    base: Color,
) -> Vec<u8> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.target"),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut targets = FrameTargets::new();
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets);
    renderer.render(target, scene, &FrameParams::new(base)).expect("render");
    queue.submit([]);
    readback(device, queue, &texture)
}

pub(crate) fn pixel(rgba: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3]]
}

pub(crate) fn is_white(c: [u8; 4]) -> bool {
    (c[0] as i32 - 255).abs() <= 1 && (c[1] as i32 - 255).abs() <= 1 && (c[2] as i32 - 255).abs() <= 1
}

/// A `w`×`h` straight-alpha RGBA image filled with `px`.
pub(crate) fn rgba_image(w: u32, h: u32, px: [u8; 4]) -> peniko::ImageData {
    peniko::ImageData {
        data: peniko::Blob::from(px.repeat((w * h) as usize)),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::Alpha,
        width: w,
        height: h,
    }
}

pub(crate) fn image_scene(image: &peniko::ImageData, transform: Affine) -> Scene {
    let mut scene = Scene::new();
    Painter::new(&mut scene).draw_image(&peniko::ImageBrush::new(image.clone()), transform);
    scene
}

pub(crate) fn target_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

pub(crate) fn offscreen(name: &str) -> Option<pebbles_gpu::OffscreenRenderer> {
    match pebbles_gpu::OffscreenRenderer::new() {
        Ok(o) => Some(o),
        Err(e) => {
            assert!(!std::env::var("PEBBLES_REQUIRE_GPU").is_ok_and(|v| v == "1"), "no GPU adapter: {e}");
            eprintln!("SKIP {name}: {e}");
            None
        }
    }
}

/// Copy a texture's rows (padded to 256 bytes) into an unpadded byte buffer.
pub(crate) fn readback(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let width = texture.width();
    let height = texture.height();
    let padded = width * 4
        + (wgpu::COPY_BYTES_PER_ROW_ALIGNMENT - (width * 4) % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let size = padded * height;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pebbles.gpu.test.readback"),
        size: u64::from(size),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    while rx.try_recv().is_err() {
        let _ = device.poll(wgpu::PollType::Poll);
    }
    let data = slice.get_mapped_range();
    let mut out = Vec::with_capacity((width * height * 4) as usize);
    for row in 0..height {
        let start = (row * padded) as usize;
        out.extend_from_slice(&data[start..start + (width * 4) as usize]);
    }
    drop(data);
    buffer.unmap();
    out
}
