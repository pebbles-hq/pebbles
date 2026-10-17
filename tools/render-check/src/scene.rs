//! Case → logical Scene → frame Scene (§6.6).

use pebbles_render::paint::{Painter, Scene, kurbo};

use crate::case::{Build, Case};

/// Record the case's logical-px Scene for `frame` (0-based).
pub fn logical_scene(case: &Case, frame: u32) -> Scene {
    match case.build {
        Build::Ops(f) => {
            let mut s = Scene::new();
            f(&mut Painter::new(&mut s));
            s
        }
        Build::FrameOps(f) => {
            let mut s = Scene::new();
            f(&mut Painter::new(&mut s), frame);
            s
        }
        // Widget cases are wired by V15.
        Build::Harness(_) => Scene::new(),
    }
}

/// Mirror the runner (`app/runner/render.rs`): the logical scene appended under the DPI scale.
/// The candidate and the smoke reference both render THIS Scene value (candidate scale 1.0).
pub fn frame_scene(logical: &Scene, scale: f64) -> Scene {
    let mut frame = Scene::new();
    frame.append(logical, Some(kurbo::Affine::scale(scale)));
    frame
}
