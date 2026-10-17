//! `xform/` — transforms (§7.2) — Slice P1 — 128×128.

use pebbles_render::paint::{Painter, Scene, kurbo, peniko};

use kurbo::{Affine, Circle, Line, Rect, RoundedRect, Stroke};
use peniko::{Color, Fill};

use super::prim::{BLUE, star};
use crate::case::{Case, Category::Geometry, Slice::P1, ops};

const INK: Color = Color::from_rgba8(0x11, 0x18, 0x27, 0xFF);

/// Every `xform/` case.
pub fn cases() -> Vec<Case> {
    vec![
        ops("xform/rotate_15", Geometry, P1, 128, 128, rotate_15),
        ops("xform/rotate_45", Geometry, P1, 128, 128, rotate_45),
        ops("xform/rotate_90", Geometry, P1, 128, 128, rotate_90),
        ops("xform/skew_x_0_3", Geometry, P1, 128, 128, skew_x_0_3),
        ops("xform/mirror_x", Geometry, P1, 128, 128, mirror_x),
        ops("xform/scale_10x_circle", Geometry, P1, 128, 128, scale_10x_circle),
        ops("xform/nested_append", Geometry, P1, 128, 128, nested_append),
    ]
}

/// Filled rrect + stroked circle under `rotate_about(theta, (64,64))`.
fn rotated(p: &mut Painter<'_>, theta: f64) {
    let t = Affine::rotate_about(theta, kurbo::Point::new(64., 64.));
    p.fill(Fill::NonZero, t, BLUE, None, &RoundedRect::new(32., 40., 96., 88., 10.));
    p.stroke(&Stroke::new(3.), t, INK, None, &Circle::new((64., 64.), 40.));
}

fn rotate_15(p: &mut Painter<'_>) {
    rotated(p, 15f64.to_radians());
}

fn rotate_45(p: &mut Painter<'_>) {
    rotated(p, 45f64.to_radians());
}

fn rotate_90(p: &mut Painter<'_>) {
    rotated(p, 90f64.to_radians());
}

fn skew_x_0_3(p: &mut Painter<'_>) {
    let t = Affine::skew(0.3, 0.0);
    p.fill(Fill::NonZero, t, BLUE, None, &Rect::new(8., 8., 64., 64.));
    p.fill(Fill::NonZero, t, INK, None, &Circle::new((64., 96.), 20.));
}

/// `FLIP_X` then translate(128, 0), on an off-center star so the mirror is visible.
fn mirror_x(p: &mut Painter<'_>) {
    let t = Affine::translate((128., 0.)) * Affine::FLIP_X;
    p.fill(Fill::NonZero, t, BLUE, None, &star((44., 56.), 36.));
}

/// `Circle r10` at (6.4, 6.4) under `scale(10)`: flattening tolerance must scale.
fn scale_10x_circle(p: &mut Painter<'_>) {
    p.fill(Fill::NonZero, Affine::scale(10.), BLUE, None, &Circle::new((6.4, 6.4), 10.));
}

fn sub_scene() -> Scene {
    let mut s = Scene::new();
    let mut q = Painter::new(&mut s);
    q.fill(Fill::NonZero, Affine::IDENTITY, BLUE, None, &RoundedRect::new(0., 0., 40., 24., 6.));
    q.stroke(&Stroke::new(2.), Affine::IDENTITY, INK, None, &Line::new((0., 30.), (40., 30.)));
    s
}

/// The sub-scene appended at translate, rotate 30°, scale 0.5, and a depth-3 append-in-append.
fn nested_append(p: &mut Painter<'_>) {
    let sub = sub_scene();
    p.append(&sub, Some(Affine::translate((8., 8.))));
    p.append(&sub, Some(Affine::translate((64., 8.)) * Affine::rotate(30f64.to_radians())));
    p.append(&sub, Some(Affine::translate((8., 64.)) * Affine::scale(0.5)));
    let mut d1 = Scene::new();
    Painter::new(&mut d1).append(&sub, Some(Affine::translate((4., 4.))));
    let mut d2 = Scene::new();
    Painter::new(&mut d2).append(&d1, Some(Affine::rotate(10f64.to_radians())));
    let mut d3 = Scene::new();
    Painter::new(&mut d3).append(&d2, Some(Affine::scale(0.75)));
    p.append(&d3, Some(Affine::translate((64., 64.))));
}
