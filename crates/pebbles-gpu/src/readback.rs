//! Texture readback for captures: padded copy, bounded wait, unpad, unpremultiply.

use crate::error::CaptureError;

/// Copy `texture` (RGBA8, one mip) into a tightly packed `width * height * 4` buffer. Rows are
/// padded to `COPY_BYTES_PER_ROW_ALIGNMENT` for the copy and unpadded here. Waits at most
/// `timeout` for the GPU.
pub(crate) fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    timeout: std::time::Duration,
) -> Result<Vec<u8>, CaptureError> {
    let (width, height) = (texture.width(), texture.height());
    let unpadded = width as usize * 4;
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded = unpadded.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("pebbles.gpu.readback.buffer"),
        size: (padded * height as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.gpu.readback") });
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
                bytes_per_row: Some(u32::try_from(padded).map_err(|_| CaptureError::ReadbackAborted)?),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device
        .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(timeout) })
        .map_err(CaptureError::Poll)?;
    rx.recv().map_err(|_| CaptureError::ReadbackAborted)?.map_err(CaptureError::Map)?;

    let mapped = slice.get_mapped_range();
    let mut out = Vec::with_capacity(unpadded * height as usize);
    for row in mapped.chunks_exact(padded) {
        out.extend_from_slice(&row[..unpadded]);
    }
    drop(mapped);
    buffer.unmap();
    Ok(out)
}

/// Premultiplied → straight alpha: alpha 0 → all zero, else `round(c * 255 / a)` clamped.
pub(crate) fn unpremultiply_in_place(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a == 0 {
            *px = [0; 4];
            continue;
        }
        for c in &mut px[..3] {
            let v = (u32::from(*c) * 255 + a / 2) / a;
            *c = u8::try_from(v.min(255)).unwrap_or(u8::MAX);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::unpremultiply_in_place;

    #[test]
    fn unpremultiply_rounds_and_zeroes_transparent() {
        let mut px = [128, 0, 0, 128, 7, 7, 7, 0, 255, 255, 255, 255];
        unpremultiply_in_place(&mut px);
        assert_eq!(px, [255, 0, 0, 128, 0, 0, 0, 0, 255, 255, 255, 255]);
    }
}
