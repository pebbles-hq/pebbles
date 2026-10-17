//! RGBA8 PNG encode/decode (§6.9).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use crate::CheckError;
use crate::metric::Rgba8;

/// Write `img` as an 8-bit RGBA PNG (`Compression::High`), creating parent dirs.
pub fn encode(path: &Path, img: &Rgba8) -> Result<(), CheckError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| CheckError::Io(format!("{}: {e}", dir.display())))?;
    }
    let file = File::create(path).map_err(|e| CheckError::Io(format!("{}: {e}", path.display())))?;
    let mut enc = png::Encoder::new(BufWriter::new(file), img.width, img.height);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::High);
    let png_err = |e: png::EncodingError| CheckError::Png(format!("{}: {e}", path.display()));
    let mut writer = enc.write_header().map_err(png_err)?;
    writer.write_image_data(&img.data).map_err(png_err)?;
    writer.finish().map_err(png_err)
}

/// Read a PNG as RGBA8 (RGB gets alpha 255; 16-bit output is rejected).
pub fn decode(path: &Path) -> Result<Rgba8, CheckError> {
    let file = File::open(path).map_err(|e| CheckError::Io(format!("{}: {e}", path.display())))?;
    let mut dec = png::Decoder::new(BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND);
    let png_err = |e: png::DecodingError| CheckError::Png(format!("{}: {e}", path.display()));
    let mut reader = dec.read_info().map_err(png_err)?;
    let size = reader.output_buffer_size().ok_or_else(|| CheckError::Png("image too large".into()))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(png_err)?;
    if info.bit_depth != png::BitDepth::Eight {
        return Err(CheckError::Png(format!("{}: {:?} output, want 8-bit", path.display(), info.bit_depth)));
    }
    let px = &buf[..info.buffer_size()];
    let data = match info.color_type {
        png::ColorType::Rgba => px.to_vec(),
        png::ColorType::Rgb => px.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => {
            px.as_chunks::<2>().0.iter().flat_map(|p| [p[0], p[0], p[0], p[1]]).collect()
        }
        png::ColorType::Grayscale => px.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => {
            return Err(CheckError::Png(format!("{}: unexpanded palette", path.display())));
        }
    };
    Ok(Rgba8 { width: info.width, height: info.height, data })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip_is_lossless() {
        let mut img = Rgba8::filled(7, 5, [1, 2, 3, 4]);
        img.set(6, 4, [255, 0, 128, 0]);
        img.set(0, 0, [9, 99, 199, 255]);
        let path = std::env::temp_dir().join(format!("render-check-png-{}.png", std::process::id()));
        encode(&path, &img).unwrap();
        let back = decode(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        assert_eq!(back, img);
    }
}
