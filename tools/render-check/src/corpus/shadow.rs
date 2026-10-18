//! shadow/ — Slice P3 (V23), §7.2: blurred rounded rects (02 §8.2) and widget box shadows.

use pebbles_core::IntoWidget as _;
use pebbles_foundation::{Alignment, Color, EdgeInsets, Offset};
use pebbles_render::paint::{Painter, kurbo};
use pebbles_render::{BorderRadius, BoxShadow};
use pebbles_testing::Harness;
use pebbles_widgets::{container, list_view, stack};

use kurbo::{Affine, Rect};

use crate::case::{Case, Category::Analytic, Category::Shadow, Slice::P3, harness, ops, with_scale_variants};
use crate::metric::Rgba8;
use crate::probe;

const W: u32 = 128;
const H: u32 = 128;
const SHADOW: Color = Color::from_rgba8(0, 0, 0, 0x99);
const BG: Color = Color::from_rgba8(0xF0, 0xF0, 0xF0, 0xFF);

pub fn cases() -> Vec<Case> {
    // The landed shader is a 1-D erf-of-SDF approximation; the 02 §8.2 closed form (judged by
    // the golden model's 2-D Gaussian) lands with R40.
    let r40 = |c: Case| c.xfail("R40", "shadow shader is not the 02 §8.2 closed form until R40");
    let mut v = vec![
        r40(ops("shadow/blurred_rect_basic", Shadow, P3, W, H, blurred_rect_basic)),
        ops("shadow/blur_min", Shadow, P3, W, H, blur_min),
        r40(ops("shadow/blur_64", Shadow, P3, 256, 256, blur_64)),
        r40(ops("shadow/radius_gt_half", Shadow, P3, W, H, radius_gt_half)),
        r40(ops("shadow/translated_scaled", Shadow, P3, W, H, translated_scaled)),
        ops("shadow/rotated", Analytic, P3, W, H, rotated)
            .analytic()
            .probe("rotated_shadow_bounds", rotated_shadow_bounds),
    ];
    v.extend(
        with_scale_variants(
            harness("shadow/elevation_card", Shadow, P3, 200, 200, elevation_card)
                .bg(BG)
                .probe("card_interior_white", probe::card_interior_white)
                .probe("shadow_below_card", probe::shadow_below_card),
        )
        .into_iter()
        .map(r40),
    );
    v.extend([
        r40(harness("shadow/overlapping_cards", Shadow, P3, 200, 200, overlapping_cards).bg(BG)),
        r40(harness("shadow/inside_scroll_clip", Shadow, P3, 200, 200, inside_scroll_clip).bg(BG)),
    ]);
    v
}

fn blurred(p: &mut Painter<'_>, t: Affine, rect: Rect, radius: f64, std_dev: f64) {
    p.draw_blurred_rounded_rect(t, rect, SHADOW, radius, std_dev);
}

fn blurred_rect_basic(p: &mut Painter<'_>) {
    blurred(p, Affine::IDENTITY, Rect::new(24.0, 24.0, 104.0, 104.0), 8.0, 6.0);
}

fn blur_min(p: &mut Painter<'_>) {
    blurred(p, Affine::IDENTITY, Rect::new(24.0, 24.0, 104.0, 104.0), 8.0, 0.01);
}

fn blur_64(p: &mut Painter<'_>) {
    blurred(p, Affine::IDENTITY, Rect::new(64.0, 64.0, 192.0, 192.0), 8.0, 64.0);
}

fn radius_gt_half(p: &mut Painter<'_>) {
    blurred(p, Affine::IDENTITY, Rect::new(24.0, 34.0, 104.0, 94.0), 80.0, 6.0);
}

fn translated_scaled(p: &mut Painter<'_>) {
    blurred(
        p,
        Affine::translate((10.0, 5.0)) * Affine::scale(1.5),
        Rect::new(10.0, 10.0, 60.0, 60.0),
        8.0,
        6.0,
    );
}

const ROT_RECT: Rect = Rect::new(34.0, 44.0, 94.0, 84.0);
const ROT_ANGLE: f64 = 0.3;
const ROT_SIGMA: f64 = 4.0;

fn rotated(p: &mut Painter<'_>) {
    blurred(p, Affine::rotate_about(ROT_ANGLE, kurbo::Point::new(64.0, 64.0)), ROT_RECT, 6.0, ROT_SIGMA);
}

/// `shadow/rotated`: ink inside the rotated rect's bounds + 3σ, none beyond; the shadow center
/// is drawn (02 op matrix allows the AABB approximation, which stays inside the AABB + 3σ).
fn rotated_shadow_bounds(img: &Rgba8, scale: f64) -> Result<(), String> {
    use kurbo::Shape as _;
    let t = Affine::scale(scale) * Affine::rotate_about(ROT_ANGLE, kurbo::Point::new(64.0, 64.0));
    let bb = (t * ROT_RECT.to_path(0.1))
        .bounding_box()
        .inflate(3.0 * ROT_SIGMA * scale * 2.0, 3.0 * ROT_SIGMA * scale * 2.0);
    let c = probe::px(img, probe::dev(64.0, scale), probe::dev(64.0, scale));
    if probe::luma(c) + 40 > 255 {
        return Err(format!("center {c:?} has no shadow"));
    }
    for y in 0..img.height {
        for x in 0..img.width {
            let inside = bb.contains(kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5));
            let p = img.px(x, y);
            if !inside && !probe::near(p, [255; 4], 1) {
                return Err(format!("ink {p:?} at ({x},{y}) outside bounds + 3σ"));
            }
        }
    }
    Ok(())
}

/// The V06 card: white 80×80 at logical (40,40), radius 12, elevation shadow, on `#F0F0F0`.
fn card() -> pebbles_widgets::Container {
    container()
        .width(80.0)
        .height(80.0)
        .color(Color::WHITE)
        .radius(BorderRadius::all(12.0))
        .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 0x40), Offset::new(0.0, 4.0), 12.0, 0.0))
}

fn elevation_card(h: &mut Harness) {
    h.mount_widget(container().padding(EdgeInsets::all(40.0)).alignment(Alignment::TOP_LEFT).child(card()));
}

fn overlapping_cards(h: &mut Harness) {
    let a = container()
        .padding(EdgeInsets::only(30.0, 30.0, 0.0, 0.0))
        .alignment(Alignment::TOP_LEFT)
        .child(card());
    let b = container()
        .padding(EdgeInsets::only(80.0, 80.0, 0.0, 0.0))
        .alignment(Alignment::TOP_LEFT)
        .child(card());
    h.mount_widget(stack(vec![a.into_widget(), b.into_widget()]));
}

fn inside_scroll_clip(h: &mut Harness) {
    let rows: Vec<_> = (0..6)
        .map(|_| container().padding(EdgeInsets::all(16.0)).child(card().width(140.0)).into_widget())
        .collect();
    h.mount_widget(container().height(200.0).child(list_view(rows)));
    h.scroll(Offset::new(100.0, 100.0), 60.0);
}
