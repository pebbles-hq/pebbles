//! Padded texture readback (§6.9; mirrors pebbles-shell's capture readback).

use std::time::Duration;

use crate::CheckError;

/// How long a readback waits for the GPU.
pub const READBACK_TIMEOUT: Duration = Duration::from_secs(30);

/// `width * 4` rounded up to `COPY_BYTES_PER_ROW_ALIGNMENT` (256).
pub fn padded_bytes_per_row(width: u32) -> u32 {
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT; // 256
    (width * 4).div_ceil(align) * align
}

/// Drop the per-row padding of a mapped buffer.
pub fn unpad_rows(mapped: &[u8], width: u32, height: u32, padded: u32) -> Vec<u8> {
    let (row, padded) = (width as usize * 4, padded as usize);
    let mut out = Vec::with_capacity(row * height as usize);
    for y in 0..height as usize {
        out.extend_from_slice(&mapped[y * padded..y * padded + row]);
    }
    out
}

/// Premultiplied → straight alpha (`round(c * 255 / a)`; alpha 0 → all zero), so the
/// smoke image is in the candidate's (unpremultiplied) encoding.
pub fn unpremultiply_in_place(rgba: &mut [u8]) {
    for px in rgba.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a == 0 {
            *px = [0; 4];
            continue;
        }
        for c in &mut px[..3] {
            *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
        }
    }
}

/// Copy `texture` (Rgba8Unorm, COPY_SRC) into a MAP_READ buffer and return tight RGBA8.
pub fn read_texture_rgba(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Result<Vec<u8>, CheckError> {
    let (w, h) = (texture.width(), texture.height());
    let padded = padded_bytes_per_row(w);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("check.readback"),
        size: u64::from(padded) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("check.readback") });
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
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(READBACK_TIMEOUT) })
        .map_err(|e| CheckError::Readback(e.to_string()))?;
    rx.recv()
        .map_err(|_| CheckError::Readback("map callback dropped".into()))?
        .map_err(|e| CheckError::Readback(e.to_string()))?;
    let out = unpad_rows(&slice.get_mapped_range(), w, h, padded);
    buffer.unmap();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn padded_bytes_per_row_aligns_to_256() {
        let got: Vec<u32> = [1, 63, 64, 65, 16384].into_iter().map(padded_bytes_per_row).collect();
        assert_eq!(got, [256, 256, 256, 512, 65536]);
    }

    #[test]
    fn unpad_rows_drops_padding() {
        let (w, h) = (3u32, 4u32);
        let padded = padded_bytes_per_row(w);
        let mut buf = vec![0xAB; (padded * h) as usize];
        for y in 0..h as usize {
            for i in 0..(w * 4) as usize {
                buf[y * padded as usize + i] = (y * 16 + i) as u8;
            }
        }
        let out = unpad_rows(&buf, w, h, padded);
        assert_eq!(out.len(), (w * h * 4) as usize);
        assert!(!out.contains(&0xAB));
    }

    #[test]
    fn unpremultiply_restores_straight_alpha() {
        let mut px = [64, 0, 32, 128, 9, 9, 9, 0, 255, 255, 255, 255];
        unpremultiply_in_place(&mut px);
        assert_eq!(px, [128, 0, 64, 128, 0, 0, 0, 0, 255, 255, 255, 255]);
    }
}
