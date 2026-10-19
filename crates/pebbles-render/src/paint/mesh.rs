//! Triangle meshes and sprite batches (02 §12, Flutter `Canvas.drawVertices` / `drawAtlas`).

use kurbo::Rect;
use peniko::Color;

/// How `positions` form triangles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VertexMode {
    /// Every 3 vertices.
    Triangles,
    /// `(i, i+1, i+2)` for every i (alternate winding is irrelevant: meshes are not culled).
    TriangleStrip,
    /// `(0, i, i+1)`.
    TriangleFan,
}

/// How per-vertex colors combine with the brush.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum VertexColorMode {
    /// `vertex color × brush` (Flutter `BlendMode.modulate`).
    #[default]
    Modulate,
    /// The vertex color alone.
    Replace,
}

/// Why a [`Mesh`] was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MeshError {
    TooManyVertices,
    ColorsLength,
    UvsLength,
    IndexOutOfRange,
}

/// A triangle mesh in local px. `uvs` are image pixels (for an image brush).
#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    pub mode: VertexMode,
    pub positions: Vec<[f32; 2]>,
    pub colors: Option<Vec<Color>>,
    pub uvs: Option<Vec<[f32; 2]>>,
    pub indices: Option<Vec<u16>>,
}

impl Mesh {
    /// At most 65 535 positions; `colors` / `uvs` match `positions`; indices in range.
    pub fn new(
        mode: VertexMode,
        positions: Vec<[f32; 2]>,
        colors: Option<Vec<Color>>,
        uvs: Option<Vec<[f32; 2]>>,
        indices: Option<Vec<u16>>,
    ) -> Result<Self, MeshError> {
        let n = positions.len();
        if n > 65_535 {
            return Err(MeshError::TooManyVertices);
        }
        if colors.as_ref().is_some_and(|c| c.len() != n) {
            return Err(MeshError::ColorsLength);
        }
        if uvs.as_ref().is_some_and(|u| u.len() != n) {
            return Err(MeshError::UvsLength);
        }
        if indices.as_ref().is_some_and(|ix| ix.iter().any(|&i| usize::from(i) >= n)) {
            return Err(MeshError::IndexOutOfRange);
        }
        Ok(Self { mode, positions, colors, uvs, indices })
    }

    /// The vertex sequence (indices applied) expanded to a triangle list of vertex indices.
    pub fn triangles(&self) -> Vec<u32> {
        let seq: Vec<u32> = match &self.indices {
            Some(ix) => ix.iter().map(|&i| u32::from(i)).collect(),
            None => (0..self.positions.len() as u32).collect(),
        };
        match self.mode {
            VertexMode::Triangles => seq[..seq.len() / 3 * 3].to_vec(),
            VertexMode::TriangleStrip => seq.windows(3).flat_map(|w| [w[0], w[1], w[2]]).collect(),
            VertexMode::TriangleFan => match seq.first() {
                Some(&c) => seq[1..].windows(2).flat_map(|w| [c, w[0], w[1]]).collect(),
                None => Vec::new(),
            },
        }
    }
}

/// One sprite of a batch: `src` in image px; the sprite-local point `(x, y)` lands at
/// `(scos·x − ssin·y + tx, ssin·x + scos·y + ty)` (Flutter `RSTransform`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprite {
    pub src: Rect,
    pub scos: f32,
    pub ssin: f32,
    pub tx: f32,
    pub ty: f32,
    pub color: Option<Color>,
}

impl Sprite {
    /// Flutter's `RSTransform.fromComponents`.
    pub fn from_components(
        src: Rect,
        rotation: f32,
        scale: f32,
        anchor: (f32, f32),
        translate: (f32, f32),
    ) -> Self {
        let (s, c) = rotation.sin_cos();
        let (scos, ssin) = (c * scale, s * scale);
        let tx = translate.0 - scos * anchor.0 + ssin * anchor.1;
        let ty = translate.1 - ssin * anchor.0 - scos * anchor.1;
        Self { src, scos, ssin, tx, ty, color: None }
    }

    /// The sprite-local point mapped by the RSTransform.
    pub fn map(&self, x: f32, y: f32) -> (f32, f32) {
        (self.scos * x - self.ssin * y + self.tx, self.ssin * x + self.scos * y + self.ty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_and_fan_expand() {
        let p = vec![[0.0; 2]; 5];
        let strip = Mesh::new(VertexMode::TriangleStrip, p.clone(), None, None, None).unwrap();
        assert_eq!(strip.triangles(), [0, 1, 2, 1, 2, 3, 2, 3, 4]);
        let fan = Mesh::new(VertexMode::TriangleFan, p, None, None, None).unwrap();
        assert_eq!(fan.triangles(), [0, 1, 2, 0, 2, 3, 0, 3, 4]);
    }

    #[test]
    fn mesh_validation_rejects_bad_lengths() {
        let p = vec![[0.0; 2]; 3];
        assert_eq!(
            Mesh::new(VertexMode::Triangles, p.clone(), Some(vec![Color::BLACK]), None, None),
            Err(MeshError::ColorsLength)
        );
        assert_eq!(
            Mesh::new(VertexMode::Triangles, p.clone(), None, Some(vec![[0.0; 2]; 2]), None),
            Err(MeshError::UvsLength)
        );
        assert_eq!(
            Mesh::new(VertexMode::Triangles, p, None, None, Some(vec![0, 3])),
            Err(MeshError::IndexOutOfRange)
        );
        assert_eq!(
            Mesh::new(VertexMode::Triangles, vec![[0.0; 2]; 65_536], None, None, None),
            Err(MeshError::TooManyVertices)
        );
    }

    #[test]
    fn rs_transform_matches_flutter() {
        let s = Sprite::from_components(
            Rect::new(0.0, 0.0, 10.0, 10.0),
            std::f32::consts::FRAC_PI_2,
            2.0,
            (0.0, 0.0),
            (50.0, 50.0),
        );
        let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
        assert!(near(s.map(0.0, 0.0), (50.0, 50.0)));
        assert!(near(s.map(10.0, 0.0), (50.0, 70.0)), "{:?}", s.map(10.0, 0.0));
        assert!(near(s.map(0.0, 10.0), (30.0, 50.0)));
        assert!(near(s.map(10.0, 10.0), (30.0, 70.0)));
    }
}
