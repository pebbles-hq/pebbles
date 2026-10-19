//! `mesh/` triangle meshes and `sprite/` sprite batches (05 §7 V53, 02 §12) — Slice P4,
//! 128×96, judged by the golden model (Mesh / Image limits, 05 §8.1).

use std::sync::Arc;

use pebbles_render::image_from_rgba8;
use pebbles_render::paint::{Mesh, Painter, Sprite, VertexColorMode, VertexMode, kurbo, peniko};

use kurbo::{Affine, Rect};
use peniko::{Brush, Color};

use super::img::checker_rgba8;
use crate::case::{Case, Category::Image, Category::Mesh as MeshCat, Slice::P4, ops};

const W: u32 = 128;
const H: u32 = 96;

pub fn cases() -> Vec<Case> {
    vec![
        ops("mesh/vertex_colors", MeshCat, P4, W, H, |p| {
            let m = Mesh::new(
                VertexMode::Triangles,
                vec![[12., 8.], [116., 20.], [40., 88.]],
                Some(vec![rgb(0xE5_3B_2C), rgb(0x2C_B4_4A), rgb(0x2F_5C_E3)]),
                None,
                None,
            );
            draw(p, m.expect("valid mesh"), Brush::Solid(Color::WHITE), VertexColorMode::Replace);
        }),
        ops("mesh/textured", MeshCat, P4, W, H, |p| {
            let img = image_from_rgba8(32, 32, checker_rgba8(32, 32, 8));
            let m = Mesh::new(
                VertexMode::Triangles,
                vec![[16., 12.], [104., 16.], [96., 84.], [20., 80.]],
                Some(vec![Color::WHITE, rgb(0xF5_C0_3A), Color::WHITE, rgb(0x7D_D3_FC)]),
                Some(vec![[0., 0.], [32., 0.], [32., 32.], [0., 32.]]),
                Some(vec![0, 1, 2, 0, 2, 3]),
            );
            draw(p, m.expect("valid mesh"), Brush::Image(img), VertexColorMode::Modulate);
        }),
        ops("mesh/strip_fan", MeshCat, P4, W, H, |p| {
            let strip: Vec<[f32; 2]> =
                (0..8u8).map(|i| [8. + 7. * f32::from(i), if i % 2 == 0 { 12. } else { 40. }]).collect();
            let m = Mesh::new(VertexMode::TriangleStrip, strip, None, None, None).expect("valid mesh");
            draw(p, m, Brush::Solid(rgb(0x3B_82_F6)), VertexColorMode::Modulate);
            let fan: Vec<[f32; 2]> = std::iter::once([88., 60.])
                .chain((0..7u8).map(|i| {
                    let a = f32::from(i) * 0.7;
                    [88. + 28. * a.cos(), 60. + 28. * a.sin()]
                }))
                .collect();
            let m = Mesh::new(VertexMode::TriangleFan, fan, None, None, None).expect("valid mesh");
            draw(p, m, Brush::Solid(rgb(0xEF_44_44)), VertexColorMode::Modulate);
        }),
        ops("sprite/grid_1000", Image, P4, W, H, |p| {
            let img = image_from_rgba8(16, 16, checker_rgba8(16, 16, 4));
            let sprites: Vec<Sprite> = (0..1000u16)
                .map(|i| {
                    let (x, y) = (f32::from(i % 40), f32::from(i / 40));
                    let src = Rect::new(f64::from(i % 4) * 4., 0., f64::from(i % 4) * 4. + 4., 4.);
                    Sprite {
                        color: Some(rgb(0xFF_D0_8A)),
                        ..Sprite::from_components(src, 0., 0.75, (0., 0.), (x * 3., y * 3.75))
                    }
                })
                .collect();
            p.draw_atlas(&img, &Arc::from(sprites), Affine::IDENTITY);
        }),
        ops("sprite/rotated_scaled", Image, P4, W, H, |p| {
            let img = image_from_rgba8(32, 32, checker_rgba8(32, 32, 8));
            let sprites = [
                Sprite::from_components(Rect::new(0., 0., 32., 32.), 0.3, 1.5, (16., 16.), (36., 40.)),
                Sprite::from_components(Rect::new(8., 8., 24., 24.), -0.8, 2.25, (8., 8.), (92., 52.)),
            ];
            p.draw_atlas(&img, &Arc::from(sprites), Affine::IDENTITY);
        }),
    ]
}

fn rgb(c: u32) -> Color {
    let [_, r, g, b] = c.to_be_bytes();
    Color::from_rgb8(r, g, b)
}

fn draw(p: &mut Painter<'_>, m: Mesh, brush: Brush, mode: VertexColorMode) {
    p.draw_vertices(&Arc::new(m), &brush, mode, Affine::IDENTITY);
}
