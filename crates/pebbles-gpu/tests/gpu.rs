//! GPU integration tests (pebbles-gpu). Each is `#[ignore = "gpu"]` and skips gracefully
//! without an adapter (DR-15). V01's harness replaces the test-local helpers after C15.

use pebbles_gpu::{GpuContext, GpuInit, RenderParams, Renderer};

/// One `Renderer`, rendered alternately into an `Rgba8Unorm` and a `Bgra8Unorm` target:
/// every render is inside a `Validation` error scope, each pop is `None`, and the center
/// pixel of a red fill is red in both formats.
#[test]
#[ignore = "gpu"]
fn two_formats_alternate_without_validation_errors() {
    let mut ctx = pollster::block_on(GpuContext::new(&GpuInit::headless()));
    let id = match pollster::block_on(ctx.request_device(None)) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("SKIP two_formats_alternate_without_validation_errors: {e}");
            return;
        }
    };
    let gpu = ctx.device(id).expect("device just requested");
    let device = gpu.device();
    let queue = gpu.queue();
    let mut renderer = Renderer::new(gpu, id);

    for format in
        [wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureFormat::Bgra8Unorm].into_iter().cycle().take(10)
    {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.test.target"),
            size: wgpu::Extent3d { width: 64, height: 64, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut scene = pebbles_render::paint::Scene::new();
        pebbles_render::paint::Painter::new(&mut scene).fill(
            peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            peniko::Color::from_rgba8(255, 0, 0, 255),
            None,
            &kurbo::Rect::new(8.0, 8.0, 56.0, 56.0),
        );

        let guard = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let params = RenderParams { base_color: peniko::Color::WHITE, width: 64, height: 64 };
        renderer.render_to_texture(device, queue, &scene, &view, format, &params).expect("render");
        queue.submit([]);
        let err = pollster::block_on(guard.pop());
        assert!(err.is_none(), "validation error: {err:?}");

        let bytes = readback(device, queue, &texture);
        let px = &bytes[((32 * 64 + 32) * 4)..((32 * 64 + 32) * 4 + 4)];
        // The center pixel is RED in both targets; the byte order differs by format
        // (Bgra8Unorm stores B,G,R,A).
        let expected: [u8; 4] =
            if format == wgpu::TextureFormat::Bgra8Unorm { [0, 0, 255, 255] } else { [255, 0, 0, 255] };
        assert_eq!(px, expected, "center pixel {format:?}: {px:?}");
    }
}

/// Copy a texture's rows (padded to 256 bytes) into an unpadded byte buffer.
fn readback(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
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
