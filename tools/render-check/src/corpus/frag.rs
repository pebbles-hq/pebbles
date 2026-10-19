//! `frag/` fragments (09 §7, §8.0) — Slice P4, 128×128: appended sub-scenes compose their
//! transforms and keep their layers balanced.

use pebbles_render::paint::{Painter, Scene, kurbo, peniko};

use kurbo::{Affine, Rect, RoundedRect};
use peniko::{Color, Fill, Mix};

use crate::case::{Case, Category::Layer, Slice::P4, ops};

pub fn cases() -> Vec<Case> {
    vec![
        ops("frag/append_twice", Layer, P4, 128, 128, |p| {
            let f = tile();
            p.append(&f, Some(Affine::translate((8., 8.))));
            p.append(&f, Some(Affine::translate((64., 56.)) * Affine::rotate(0.3)));
        }),
        ops("frag/nested_transforms", Layer, P4, 128, 128, |p| {
            let leaf = tile();
            let mut mid = Scene::new();
            Painter::new(&mut mid).append(&leaf, Some(Affine::rotate_about(0.4, (24., 20.))));
            Painter::new(&mut mid).append(&leaf, Some(Affine::translate((40., 44.)) * Affine::scale(0.75)));
            p.append(&mid, Some(Affine::translate((16., 12.)) * Affine::scale(1.25)));
        }),
    ]
}

/// A clipped, translucent group of two overlapping fills (48×40 local).
pub fn tile() -> Scene {
    let mut s = Scene::new();
    let mut p = Painter::new(&mut s);
    p.push_layer(Fill::NonZero, Mix::Normal, 0.7, Affine::IDENTITY, &RoundedRect::new(0., 0., 48., 40., 8.));
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0xEF, 0x45, 0x45, 0xFF),
        None,
        &Rect::new(0., 0., 32., 28.),
    );
    p.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0x3B, 0x83, 0xF7, 0xFF),
        None,
        &Rect::new(16., 12., 48., 40.),
    );
    p.pop_layer();
    s
}
