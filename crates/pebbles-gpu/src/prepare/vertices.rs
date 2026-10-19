//! Meshes and sprite batches (02 §12, R63/R64): meshes expand into device-space triangle lists
//! (no antialiasing, like Flutter); sprites become image quads sharing one texture, so a batch
//! is one instanced draw.

use kurbo::{Affine, Point};
use pebbles_render::paint::{Mesh, Sprite, VertexColorMode};

use super::Walk;
use super::num::{index_u32, narrow_f32};
use super::plan::DrawCmd;
use crate::gpu_types::{MeshVertex, QuadInstance};
use crate::stats::Unsupported;

impl Walk<'_> {
    pub(super) fn draw_mesh(
        &mut self,
        mesh: &Mesh,
        brush: &peniko::Brush,
        mode: VertexColorMode,
        xf: Affine,
        idx: u32,
    ) {
        let fold = self.alpha();
        let (tint, image) = match (brush, mode) {
            (_, VertexColorMode::Replace) => ([1.0; 4], None),
            (peniko::Brush::Solid(c), _) => (super::color::premul(*c), None),
            (peniko::Brush::Gradient(g), _) => {
                self.stats.record(Unsupported::MeshGradient);
                let c = g.stops.first().map_or(peniko::Color::BLACK, |s| s.color.to_alpha_color());
                (super::color::premul(c), None)
            }
            (peniko::Brush::Image(b), _) => {
                if super::images::validate_image(&b.image).is_err() {
                    return;
                }
                let key = crate::execute::resources::image_cache::ImageKey::of(&b.image);
                self.plan.mesh_images.push((b.image.clone(), key, super::images::sampler_key(b.sampler)));
                let a = b.sampler.alpha.clamp(0.0, 1.0);
                ([a; 4], Some((index_u32(self.plan.mesh_images.len() - 1), b.image.width, b.image.height)))
            }
        };
        let start = index_u32(self.plan.mesh_vertices.len());
        for i in mesh.triangles() {
            let i = i as usize;
            // `Mesh::new` validated every index against `positions` / `colors` / `uvs`.
            let Some(&[px, py]) = mesh.positions.get(i) else { continue };
            let p = xf * Point::new(f64::from(px), f64::from(py));
            let vc =
                mesh.colors.as_ref().and_then(|c| c.get(i)).map_or([1.0; 4], |&c| super::color::premul(c));
            let [r, g, b, a] = vc;
            let [tr, tg, tb, ta] = tint;
            let color = [r * tr * fold, g * tg * fold, b * tb * fold, a * ta * fold];
            let uv = match (image, mesh.uvs.as_ref().and_then(|u| u.get(i))) {
                (Some((_, w, h)), Some(&[u, v])) => [u / w as f32, v / h as f32],
                _ => [0.0, 0.0],
            };
            self.plan.mesh_vertices.push(MeshVertex { pos: [narrow_f32(p.x), narrow_f32(p.y)], uv, color });
        }
        let end = index_u32(self.plan.mesh_vertices.len());
        if end > start {
            self.push(
                DrawCmd::Vertices {
                    vertices: start..end,
                    image: image.map(|i| i.0),
                    clip: self.clips.current(),
                },
                idx,
            );
        }
    }

    pub(super) fn draw_sprites(
        &mut self,
        image: &peniko::ImageBrush,
        sprites: &[Sprite],
        xf: Affine,
        idx: u32,
    ) {
        if super::images::validate_image(&image.image).is_err() {
            return;
        }
        let (w, h) = (image.image.width as f32, image.image.height as f32);
        let fold = self.alpha() * image.sampler.alpha.clamp(0.0, 1.0);
        let key = crate::execute::resources::image_cache::ImageKey::of(&image.image);
        let sampler = super::images::sampler_key(image.sampler);
        let start = index_u32(self.plan.images.len());
        for s in sprites {
            let (sw, sh) = (s.src.width(), s.src.height());
            if !(sw > 0.0 && sh > 0.0) {
                continue;
            }
            let rs = Affine::new([
                f64::from(s.scos),
                f64::from(s.ssin),
                -f64::from(s.ssin),
                f64::from(s.scos),
                f64::from(s.tx),
                f64::from(s.ty),
            ]);
            let m = xf * rs;
            let o = m * Point::ORIGIN;
            let x = m * Point::new(sw, 0.0) - o;
            let y = m * Point::new(0.0, sh) - o;
            let tint = s.color.map_or([1.0; 4], super::color::premul).map(|v| v * fold);
            let f = narrow_f32;
            let quad = QuadInstance {
                xform: [f(x.x), f(x.y), f(y.x), f(y.y)],
                origin: [f(o.x), f(o.y)],
                uv: [f(s.src.x0) / w, f(s.src.y0) / h, f(s.src.x1) / w, f(s.src.y1) / h],
                color: tint,
                layer_kind: [super::image_nine::EDGE_ALL, 3],
            };
            self.plan.images.push(super::plan::ImageRequest {
                key,
                sampler,
                image: image.image.clone(),
                quad,
                mips: false,
                dynamic: None,
            });
        }
        let end = index_u32(self.plan.images.len());
        if end > start {
            self.push(DrawCmd::Images { images: start..end, clip: self.clips.current() }, idx);
        }
    }
}
