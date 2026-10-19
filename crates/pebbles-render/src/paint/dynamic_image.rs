//! Dynamic images (02 R37, DR-24): pixels that change every frame (video, camera, pixel
//! canvases). Identified by a process-wide id — never by `Blob::id`, which a new blob per frame
//! would churn — plus a generation; the renderer keeps one texture per id and uploads only the
//! dirty rectangle of each new generation.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// A pixel rectangle `x, y, w, h` (image px).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IRect {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// An updatable image. Pixels are premultiplied RGBA8, `width · height · 4` bytes.
#[derive(Clone, Debug)]
pub struct DynamicImage {
    id: u64,
    width: u32,
    height: u32,
    generation: u64,
    data: peniko::Blob<u8>,
    dirty: Option<IRect>,
}

/// One frame's snapshot of a [`DynamicImage`] (cheap: a blob refcount bump).
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicImageFrame {
    pub id: u64,
    pub generation: u64,
    pub width: u32,
    pub height: u32,
    /// Premultiplied RGBA8.
    pub data: peniko::Blob<u8>,
    /// What changed since the previous generation; `None` = the whole image.
    pub dirty: Option<IRect>,
}

impl DynamicImage {
    /// A transparent `width × height` image.
    pub fn new(width: u32, height: u32) -> Self {
        let len = width as usize * height as usize * 4;
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            generation: 0,
            data: peniko::Blob::from(vec![0u8; len]),
            dirty: None,
        }
    }

    /// Replace the pixels (premultiplied RGBA8); `dirty` = the changed rectangle (`None` = all).
    pub fn update(&mut self, premultiplied_rgba8: peniko::Blob<u8>, dirty: Option<IRect>) {
        debug_assert_eq!(premultiplied_rgba8.len(), self.width as usize * self.height as usize * 4);
        self.data = premultiplied_rgba8;
        self.dirty = dirty;
        self.generation += 1;
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The snapshot a draw records.
    pub fn frame(&self) -> DynamicImageFrame {
        DynamicImageFrame {
            id: self.id,
            generation: self.generation,
            width: self.width,
            height: self.height,
            data: self.data.clone(),
            dirty: self.dirty,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_image_generation_bumps() {
        let mut img = DynamicImage::new(4, 2);
        let id = img.frame().id;
        assert_eq!(img.generation(), 0);
        img.update(peniko::Blob::from(vec![255u8; 32]), Some(IRect { x: 0, y: 1, w: 4, h: 1 }));
        let f = img.frame();
        assert_eq!((f.id, f.generation, f.dirty), (id, 1, Some(IRect { x: 0, y: 1, w: 4, h: 1 })));
        assert_ne!(DynamicImage::new(4, 2).frame().id, id, "ids are unique");
    }
}
