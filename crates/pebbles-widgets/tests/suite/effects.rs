//! `image_filtered` / `backdrop_filter` (DR-24, Flutter `ImageFiltered` / `BackdropFilter`):
//! the recorded ops, the child painted inside the layer, and the backdrop's clip shape.
//! Headless through a real Ui + a CPU scene (no GPU).

use pebbles_core::{IntoWidget, Ui};
use pebbles_foundation::{Color, Size, palette};
use pebbles_render::paint::display_list::{Geometry, Op};
use pebbles_render::{LayerEffect, TextEnv};
use pebbles_widgets::{SizedBox, View, backdrop_filter, center, colored_box, image_filtered};

const BLUR: LayerEffect =
    LayerEffect::Blur { sigma_x: 4.0, sigma_y: 4.0, edge: pebbles_render::BlurEdge::Decal };

fn ops_of(root: impl IntoWidget) -> Vec<Op> {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, root).into_widget());
    ui.layout(&mut env, Size::new(200.0, 200.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);
    let mut out = Vec::new();
    flatten(scene.ops(), &mut out);
    out
}

fn flatten(ops: &[Op], out: &mut Vec<Op>) {
    for op in ops {
        match op {
            Op::Fragment { ops, .. } => flatten(ops, out),
            _ => out.push(op.clone()),
        }
    }
}

fn red_box() -> impl IntoWidget {
    colored_box(Color::from_rgb8(255, 0, 0), SizedBox::new(Some(80.0), Some(40.0), None))
}

/// Index of the first op matching `f`, panicking with the op list otherwise.
fn find(ops: &[Op], what: &str, f: impl Fn(&Op) -> bool) -> usize {
    ops.iter().position(f).unwrap_or_else(|| panic!("no {what} in {ops:#?}"))
}

fn is_red_fill(op: &Op) -> bool {
    matches!(op, Op::Fill { brush: pebbles_render::paint::peniko::Brush::Solid(c), .. } if c.to_rgba8().r == 255 && c.to_rgba8().g == 0)
}

#[test]
fn image_filtered_draws_the_child_inside_an_effect_layer() {
    let ops = ops_of(center(image_filtered(BLUR, red_box())));
    let push = find(
        &ops,
        "PushEffectLayer",
        |o| matches!(o, Op::PushEffectLayer { effect: LayerEffect::Blur { sigma_x, .. }, clip: None, .. } if *sigma_x == 4.0),
    );
    let fill = find(&ops, "the child's fill", is_red_fill);
    let pop = push + ops[push..].iter().position(|o| matches!(o, Op::PopLayer)).expect("PopLayer");
    assert!(push < fill && fill < pop, "child must paint inside the effect layer: {push} < {fill} < {pop}");
}

#[test]
fn backdrop_filter_clips_to_its_box_and_draws_the_child_on_top() {
    let ops = ops_of(center(backdrop_filter(BLUR, red_box()).opacity(0.5)));
    let push = find(&ops, "PushBackdropFilter", |o| matches!(o, Op::PushBackdropFilter { .. }));
    let Op::PushBackdropFilter { clip, alpha, .. } = &ops[push] else { unreachable!() };
    assert_eq!(*alpha, 0.5);
    // Square corners: the clip is the box itself, 80×40 centered in 200×200.
    let Geometry::Rect(r) = clip else { panic!("square backdrop clips to a rect, got {clip:?}") };
    assert_eq!((r.x0, r.y0, r.width(), r.height()), (60.0, 80.0, 80.0, 40.0));
    let fill = find(&ops, "the child's fill", is_red_fill);
    let pop = push + ops[push..].iter().position(|o| matches!(o, Op::PopLayer)).expect("PopLayer");
    assert!(push < fill && fill < pop, "child must paint on the filtered backdrop: {push} < {fill} < {pop}");
}

#[test]
fn backdrop_filter_radius_rounds_the_clip() {
    let ops = ops_of(center(backdrop_filter(BLUR, red_box()).radius(12.0)));
    let push = find(&ops, "PushBackdropFilter", |o| matches!(o, Op::PushBackdropFilter { .. }));
    let Op::PushBackdropFilter { clip, alpha, .. } = &ops[push] else { unreachable!() };
    assert_eq!(*alpha, 1.0);
    assert!(matches!(clip, Geometry::RoundedRect(rr) if rr.radii().top_left == 12.0), "{clip:?}");
}
