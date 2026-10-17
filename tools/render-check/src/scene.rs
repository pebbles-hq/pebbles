//! Case → logical Scene → frame Scene (§6.6).

use pebbles_render::paint::{Painter, Scene, kurbo};
use pebbles_testing::Harness;

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
        Build::Harness(f) => {
            // Theme is thread-local global state: reset it so no case leaks its theme.
            pebbles_widgets::theme::set_theme(pebbles_widgets::theme::Theme::light());
            let mut h = Harness::new()
                .window(f64::from(case.logical.0), f64::from(case.logical.1))
                .background(case.background);
            f(&mut h); // mounts (h.mount / h.mount_widget) and interacts
            h.settle();
            h.draw()
        }
    }
}

/// Mirror the runner (`app/runner/render.rs`): the logical scene appended under the DPI scale.
/// The candidate and the smoke reference both render THIS Scene value (candidate scale 1.0).
pub fn frame_scene(logical: &Scene, scale: f64) -> Scene {
    let mut frame = Scene::new();
    frame.append(logical, Some(kurbo::Affine::scale(scale)));
    frame
}

#[cfg(test)]
mod tests {
    use pebbles_core::IntoWidget as _;
    use pebbles_widgets::theme::{DesignLanguage, set_design};
    use pebbles_widgets::{button, column, text};

    use super::*;
    use crate::case::{Category, Slice, harness};

    /// Op kinds in order, with full geometry/paint for everything but glyph runs and images
    /// (their font/image handles carry identities that differ between runs).
    fn fingerprint(s: &Scene) -> (usize, Vec<String>) {
        let ops = s
            .ops()
            .iter()
            .map(|op| {
                let d = format!("{op:?}");
                if d.starts_with("Glyphs") || d.starts_with("Image") {
                    d.split([' ', '{']).next().unwrap_or_default().to_string()
                } else {
                    d
                }
            })
            .collect();
        (s.len(), ops)
    }

    fn hello(h: &mut Harness) {
        h.mount_widget(text("hello"));
    }

    fn material(h: &mut Harness) {
        set_design(DesignLanguage::Material);
        h.mount_widget(button("A"));
    }

    fn themed(h: &mut Harness) {
        h.mount_widget(column(vec![button("B").into_widget(), text("b").into_widget()]));
    }

    #[test]
    fn widget_case_scene_is_nonempty() {
        let c = harness("widget/hello", Category::Screen, Slice::P2, 480, 320, hello);
        assert!(!logical_scene(&c, 0).is_empty());
    }

    #[test]
    fn frame_scene_is_a_single_fragment() {
        let mut s = Scene::new();
        crate::corpus::solid_rect(&mut Painter::new(&mut s));
        assert_eq!(frame_scene(&s, 2.0).len(), 1);
    }

    #[test]
    fn theme_is_reset_between_cases() {
        let b = harness("widget/b", Category::Screen, Slice::P2, 240, 120, themed);
        let fresh = fingerprint(&logical_scene(&b, 0));
        let a = harness("widget/a", Category::Screen, Slice::P2, 240, 120, material);
        let _ = logical_scene(&a, 0);
        // Without the reset, B would now render with Material's radii and padding.
        assert_eq!(fingerprint(&logical_scene(&b, 0)), fresh);
        // And the switch is real: under Material, B's scene differs.
        set_design(DesignLanguage::Material);
        let mut h = Harness::new().window(240.0, 120.0);
        themed(&mut h);
        h.settle();
        assert_ne!(fingerprint(&h.draw()), fresh, "Material must change B's scene");
        pebbles_widgets::theme::set_theme(pebbles_widgets::theme::Theme::light());
    }
}
