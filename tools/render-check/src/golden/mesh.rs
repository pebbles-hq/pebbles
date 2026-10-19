//! Meshes and sprite batches in the golden model (02 §12, V52 step 4): meshes cover a pixel
//! when its center is inside a triangle (top-left rule, no antialiasing), colors interpolate
//! barycentrically; sprites are image rects under the `RSTransform`, sampled inside their source
//! rect inset by half a texel.

use kurbo::{Affine, Point, Rect};
use pebbles_render::paint::{Mesh, Sprite, VertexColorMode};
use peniko::{Brush, Extend, ImageQuality};

use super::paint::Texels;

fn premul(c: peniko::Color) -> [f64; 4] {
    let [r, g, b, a] = c.components.map(f64::from);
    [r * a, g * a, b * a, a]
}

fn over(dst: &mut [f64; 4], s: [f64; 4]) {
    for k in 0..4 {
        dst[k] = s[k] + dst[k] * (1.0 - s[3]);
    }
}

/// Edge function of `(a, b)` at `p` (positive inside a counter-clockwise-in-y-up triangle).
fn edge(a: Point, b: Point, p: Point) -> f64 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}

/// Whether the edge `a → b` is a top or left edge for the triangle's orientation `sign`.
fn top_left(a: Point, b: Point, sign: f64) -> bool {
    let (dx, dy) = ((b.x - a.x) * sign, (b.y - a.y) * sign);
    (dy == 0.0 && dx > 0.0) || dy < 0.0
}

pub fn draw_mesh(
    buf: &mut [[f64; 4]],
    w: u32,
    h: u32,
    mesh: &Mesh,
    brush: &Brush,
    mode: VertexColorMode,
    t: Affine,
) {
    let (tint, tex) = match (brush, mode) {
        (_, VertexColorMode::Replace) => ([1.0; 4], None),
        (Brush::Solid(c), _) => (premul(*c), None),
        (Brush::Gradient(g), _) => {
            (g.stops.first().map_or([0.0, 0.0, 0.0, 1.0], |s| premul(s.color.to_alpha_color())), None)
        }
        (Brush::Image(b), _) => {
            let a = f64::from(b.sampler.alpha.clamp(0.0, 1.0));
            ([a; 4], Some((Texels::of(&b.image), b.sampler.quality == ImageQuality::Low)))
        }
    };
    let tri = mesh.triangles();
    for c in tri.as_chunks::<3>().0 {
        let idx = [c[0] as usize, c[1] as usize, c[2] as usize];
        let p = idx.map(|i| t * Point::new(f64::from(mesh.positions[i][0]), f64::from(mesh.positions[i][1])));
        let area = edge(p[0], p[1], p[2]);
        if area == 0.0 {
            continue;
        }
        let sign = area.signum();
        let col = idx.map(|i| mesh.colors.as_ref().map_or([1.0; 4], |cs| premul(cs[i])));
        let uv =
            idx.map(|i| mesh.uvs.as_ref().map_or([0.0; 2], |u| [f64::from(u[i][0]), f64::from(u[i][1])]));
        let bb = Rect::from_points(p[0], p[1]).union_pt(p[2]);
        let (x0, y0) = (bb.x0.floor().max(0.0) as u32, bb.y0.floor().max(0.0) as u32);
        let (x1, y1) = ((bb.x1.ceil().max(0.0) as u32).min(w), (bb.y1.ceil().max(0.0) as u32).min(h));
        for y in y0..y1 {
            for x in x0..x1 {
                let q = Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                let e = [edge(p[1], p[2], q), edge(p[2], p[0], q), edge(p[0], p[1], q)].map(|v| v * sign);
                let pairs = [(p[1], p[2]), (p[2], p[0]), (p[0], p[1])];
                let inside =
                    e.iter().zip(pairs).all(|(&v, (a, b))| v > 0.0 || (v == 0.0 && top_left(a, b, sign)));
                if !inside {
                    continue;
                }
                let bc = e.map(|v| v / (area * sign));
                let vc: [f64; 4] =
                    std::array::from_fn(|k| bc[0] * col[0][k] + bc[1] * col[1][k] + bc[2] * col[2][k]);
                let texel = match &tex {
                    Some((tx, nearest)) => {
                        let u = bc[0] * uv[0][0] + bc[1] * uv[1][0] + bc[2] * uv[2][0];
                        let v = bc[0] * uv[0][1] + bc[1] * uv[1][1] + bc[2] * uv[2][1];
                        tx.sample(u, v, Extend::Pad, Extend::Pad, *nearest)
                    }
                    None => [1.0; 4],
                };
                let s: [f64; 4] = std::array::from_fn(|k| vc[k] * tint[k] * texel[k]);
                over(&mut buf[(y * w + x) as usize], s);
            }
        }
    }
}

pub fn draw_sprites(
    buf: &mut [[f64; 4]],
    w: u32,
    h: u32,
    image: &peniko::ImageBrush,
    sprites: &[Sprite],
    t: Affine,
) {
    let tex = Texels::of(&image.image);
    let nearest = image.sampler.quality == ImageQuality::Low;
    let a = f64::from(image.sampler.alpha.clamp(0.0, 1.0));
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
        let m = t * rs;
        if m.determinant().abs() < 1e-12 {
            continue;
        }
        let inv = m.inverse();
        let tint = s.color.map_or([1.0; 4], premul);
        let cov = super::shapes::fill_geometry(
            &pebbles_render::paint::display_list::Geometry::Rect(Rect::new(0.0, 0.0, sw, sh)),
            m,
            peniko::Fill::NonZero,
            super::shapes::Snap::Off,
            w,
            h,
        );
        for y in 0..h {
            for x in 0..w {
                let k = cov[(y * w + x) as usize];
                if k == 0.0 {
                    continue;
                }
                let l = inv * Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                let u = (s.src.x0 + l.x).clamp(s.src.x0 + 0.5, (s.src.x1 - 0.5).max(s.src.x0 + 0.5));
                let v = (s.src.y0 + l.y).clamp(s.src.y0 + 0.5, (s.src.y1 - 0.5).max(s.src.y0 + 0.5));
                let texel = tex.sample(u, v, Extend::Pad, Extend::Pad, nearest);
                let src: [f64; 4] = std::array::from_fn(|c| texel[c] * tint[c] * a * k);
                over(&mut buf[(y * w + x) as usize], src);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pebbles_render::paint::VertexMode;

    #[test]
    fn mesh_top_left_rule_shared_edge_covered_once() {
        // Two triangles sharing the diagonal of an 8×8 square at half alpha: double coverage
        // on the shared edge would show as alpha > 0.5.
        let m = Mesh::new(
            VertexMode::Triangles,
            vec![[0., 0.], [8., 0.], [8., 8.], [0., 0.], [8., 8.], [0., 8.]],
            None,
            None,
            None,
        )
        .expect("valid mesh");
        let mut buf = vec![[0.0; 4]; 64];
        let brush = Brush::Solid(peniko::Color::from_rgba8(255, 0, 0, 128));
        draw_mesh(&mut buf, 8, 8, &m, &brush, VertexColorMode::Modulate, Affine::IDENTITY);
        let a = 128.0 / 255.0;
        assert!(buf.iter().all(|p| (p[3] - a).abs() < 1e-6), "every pixel covered exactly once");
    }
}
