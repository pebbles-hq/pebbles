//! The scene corpus (§7): `all()` concatenates one module per family.

use pebbles_render::paint::{Painter, kurbo, peniko};

use crate::case::Case;

pub mod adversarial;
pub mod brush;
pub mod clip;
pub mod effects;
pub mod frag;
pub mod frames;
pub mod gallery;
pub mod grad;
pub mod img;
pub mod layer;
pub mod mesh;
pub mod prim;
pub mod screen;
pub mod shadow;
pub mod stress;
pub mod stroke;
pub mod text;
pub mod widget;
pub mod xform;

/// Every case, in family order.
pub fn all() -> Vec<Case> {
    let mut out = Vec::new();
    out.extend(prim::cases());
    out.extend(stroke::cases());
    out.extend(xform::cases());
    out.extend(clip::cases());
    out.extend(text::cases());
    out.extend(text::international());
    out.extend(img::cases());
    out.extend(shadow::cases());
    out.extend(layer::cases());
    out.extend(grad::cases());
    out.extend(frag::cases());
    out.extend(brush::cases());
    out.extend(effects::cases());
    out.extend(mesh::cases());
    out.extend(widget::cases());
    out.extend(screen::cases());
    out.extend(stress::cases());
    out.extend(adversarial::cases());
    out.extend(frames::cases());
    out.extend(gallery::cases());
    out
}

/// The V01 rect (`#3366FF` over (8,8)–(56,56), 64×64 on white). No longer a registered
/// case (V20 removed the `smoke` family); the GPU smoke tests of V13/V14 still draw it.
pub fn solid_rect(p: &mut Painter<'_>) {
    p.fill(
        peniko::Fill::NonZero,
        kurbo::Affine::IDENTITY,
        peniko::Color::from_rgba8(0x33, 0x66, 0xFF, 0xFF),
        None,
        &kurbo::Rect::new(8.0, 8.0, 56.0, 56.0),
    );
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::case::Expect;

    fn valid_card(id: &str) -> bool {
        let b = id.as_bytes();
        b.len() == 3 && b"CRLTMKPSV".contains(&b[0]) && b[1].is_ascii_digit() && b[2].is_ascii_digit()
    }

    #[test]
    fn corpus_names_unique_and_valid() {
        let families = [
            "prim",
            "stroke",
            "xform",
            "clip",
            "text",
            "img",
            "shadow",
            "layer",
            "grad",
            "frag",
            "brush",
            "effect",
            "mesh",
            "sprite",
            "widget",
            "screen",
            "gallery",
            "stress",
            "adversarial",
            "frames",
        ];
        let mut seen = HashSet::new();
        for c in all() {
            assert!(seen.insert(c.name.clone()), "duplicate {}", c.name);
            assert!(
                c.name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'/'),
                "bad chars in {}",
                c.name
            );
            assert_eq!(c.name.matches('/').count(), 1, "{}", c.name);
            let family = c.name.split('/').next().unwrap_or_default();
            assert!(families.contains(&family), "{}: family is not a corpus module", c.name);
            if let Expect::Xfail { until, .. } = c.expect {
                assert!(valid_card(until), "{}: xfail card `{until}`", c.name);
            }
        }
    }

    #[test]
    fn corpus_v1_counts() {
        let all = all();
        let n = |f: &str| all.iter().filter(|c| c.name.starts_with(f)).count();
        assert!(n("prim/") >= 15, "prim/ {}", n("prim/"));
        assert!(n("stroke/") >= 50, "stroke/ {}", n("stroke/"));
        assert!(n("xform/") >= 7, "xform/ {}", n("xform/"));
        assert!(n("clip/") >= 7, "clip/ {}", n("clip/"));
        assert!(!all.iter().any(|c| c.name.starts_with("smoke/")), "the smoke family is removed");
    }

    #[test]
    fn blend_and_compose_families_complete() {
        let all = all();
        let n = |prefix: &str| all.iter().filter(|c| c.name.starts_with(prefix)).count();
        assert_eq!(n("layer/blend_") - n("layer/blend_inside_clip") - n("layer/blend_in_sdf_clip"), 16);
        assert_eq!(n("layer/compose_"), 14);
    }

    /// V28: a scene-level census of the corpus. Every case's logical Scene (every frame of a
    /// frame-indexed case) is walked, recursing into fragments, and every op kind, blend mode,
    /// brush/extend combination, stroke style and image mode the renderer supports must occur.
    #[test]
    fn corpus_covers_every_op_and_mode() {
        use std::collections::BTreeSet;

        use pebbles_render::paint::display_list::Op;
        use peniko::{Brush, GradientKind};

        #[derive(Default)]
        struct Census {
            ops: BTreeSet<&'static str>,
            unknown: usize,
            fill_rules: BTreeSet<String>,
            clip_rules: BTreeSet<String>,
            mix: BTreeSet<String>,
            compose: BTreeSet<String>,
            extend: BTreeSet<String>,
            caps: BTreeSet<String>,
            joins: BTreeSet<String>,
            dashes: bool,
            quality: BTreeSet<String>,
            format: BTreeSet<String>,
            alpha: BTreeSet<String>,
            image_brush_on: BTreeSet<&'static str>,
            variable_glyphs: bool,
            fragment_depth: usize,
            skewed: bool,
        }

        impl Census {
            fn transform(&mut self, t: &kurbo::Affine) {
                let [_, b, c, ..] = t.as_coeffs();
                self.skewed |= b != 0.0 || c != 0.0;
            }
            fn image(&mut self, i: &peniko::ImageBrush) {
                self.quality.insert(format!("{:?}", i.sampler.quality));
                self.format.insert(format!("{:?}", i.image.format));
                self.alpha.insert(format!("{:?}", i.image.alpha_type));
            }
            fn brush(&mut self, b: &Brush, on: &'static str) {
                match b {
                    Brush::Gradient(g) => {
                        let kind = match &g.kind {
                            GradientKind::Linear(_) => "linear",
                            GradientKind::Radial(r)
                                if r.start_center == r.end_center && r.start_radius == 0.0 =>
                            {
                                "radial"
                            }
                            GradientKind::Radial(_) => "two-point",
                            GradientKind::Sweep(_) => "sweep",
                        };
                        self.extend.insert(format!("{kind}/{:?}", g.extend));
                    }
                    Brush::Image(i) => {
                        self.image(i);
                        self.image_brush_on.insert(on);
                    }
                    Brush::Solid(_) => {}
                }
            }
            fn blend(&mut self, b: peniko::BlendMode) {
                self.mix.insert(format!("{:?}", b.mix));
                self.compose.insert(format!("{:?}", b.compose));
            }
            fn walk(&mut self, ops: &[Op], depth: usize) {
                self.fragment_depth = self.fragment_depth.max(depth);
                for op in ops {
                    let kind = match op {
                        Op::Fill { style, transform, brush, .. } => {
                            self.fill_rules.insert(format!("{style:?}"));
                            self.transform(transform);
                            self.brush(brush, "fill");
                            "Fill"
                        }
                        Op::Stroke { style, transform, brush, .. } => {
                            self.caps.insert(format!("{:?}", style.start_cap));
                            self.caps.insert(format!("{:?}", style.end_cap));
                            self.joins.insert(format!("{:?}", style.join));
                            self.dashes |= !style.dash_pattern.is_empty();
                            self.transform(transform);
                            self.brush(brush, "stroke");
                            "Stroke"
                        }
                        Op::PushLayer { clip_fill, blend, transform, .. } => {
                            self.clip_rules.insert(format!("{clip_fill:?}"));
                            self.blend(*blend);
                            self.transform(transform);
                            "PushLayer"
                        }
                        Op::PopLayer => "PopLayer",
                        Op::PushEffectLayer { transform, .. } => {
                            self.transform(transform);
                            "PushEffectLayer"
                        }
                        Op::PushBackdropFilter { blend, transform, .. } => {
                            self.blend(*blend);
                            self.transform(transform);
                            "PushBackdropFilter"
                        }
                        Op::PushProjectedLayer { .. } => "PushProjectedLayer",
                        Op::Custom { transform, .. } => {
                            self.transform(transform);
                            "Custom"
                        }
                        Op::ShaderFill { transform, .. } => {
                            self.transform(transform);
                            "ShaderFill"
                        }
                        Op::GpuImage { transform, .. } => {
                            self.transform(transform);
                            "GpuImage"
                        }
                        Op::Vertices { brush, transform, .. } => {
                            self.brush(brush, "vertices");
                            self.transform(transform);
                            "Vertices"
                        }
                        Op::Atlas { image, transform, .. } => {
                            self.image(image);
                            self.transform(transform);
                            "Atlas"
                        }
                        Op::Image { image, transform, .. } => {
                            self.image(image);
                            self.transform(transform);
                            "Image"
                        }
                        Op::ImageNine { image, transform, .. } => {
                            self.image(image);
                            self.transform(transform);
                            "ImageNine"
                        }
                        Op::DynamicImage { .. } => "DynamicImage",
                        Op::BlurredFill { style, transform, .. } => {
                            self.fill_rules.insert(format!("{style:?}"));
                            self.transform(transform);
                            "BlurredFill"
                        }
                        Op::BlurredGlyphs { .. } => "BlurredGlyphs",
                        Op::BlurredRect { transform, .. } => {
                            self.transform(transform);
                            "BlurredRect"
                        }
                        Op::Glyphs { normalized_coords, brush, transform, .. } => {
                            self.variable_glyphs |= !normalized_coords.is_empty();
                            self.brush(brush, "glyphs");
                            self.transform(transform);
                            "Glyphs"
                        }
                        Op::Fragment { ops, transform, .. } => {
                            if let Some(t) = transform {
                                self.transform(t);
                            }
                            self.walk(ops, depth + 1);
                            "Fragment"
                        }
                        _ => {
                            self.unknown += 1;
                            continue;
                        }
                    };
                    self.ops.insert(kind);
                }
            }
        }

        let mut c = Census::default();
        for case in all() {
            for frame in 0..case.frames.max(1) {
                let scene = crate::scene::logical_scene(&case, frame);
                c.walk(scene.ops(), 0);
            }
        }

        let strs = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<BTreeSet<String>>();
        let mut missing: Vec<String> = Vec::new();
        let mut need = |what: &str, have: &BTreeSet<String>, want: BTreeSet<String>| {
            for w in want.difference(have) {
                missing.push(format!("{what}: {w}"));
            }
        };
        let mut ops: BTreeSet<String> = c.ops.iter().map(|s| (*s).to_owned()).collect();
        // Ops that draw app-owned GPU state (a registered `CustomRenderer`, an app texture): a
        // logical Scene cannot carry it and the CPU golden model cannot judge it, so they are
        // pinned by pixel-asserting GPU tests instead (pebbles-gpu tests/gpu/custom.rs).
        const GPU_TESTED: [(&str, &str); 2] = [
            ("Custom", "custom_renders_in_painter_order, custom_inside_translucent_layer_gets_group_opacity"),
            ("GpuImage", "gpu_image_composites_with_alpha, gpu_image_unknown_and_unregistered_are_skipped"),
        ];
        for (op, _tests) in GPU_TESTED {
            assert!(!ops.contains(op), "the corpus now draws {op}: drop it from GPU_TESTED");
            ops.insert(op.to_owned());
        }
        need(
            "Op",
            &ops,
            strs(&[
                "Fill",
                "Stroke",
                "PushEffectLayer",
                "PushBackdropFilter",
                "PushProjectedLayer",
                "Custom",
                "ShaderFill",
                "GpuImage",
                "Vertices",
                "Atlas",
                "PushLayer",
                "PopLayer",
                "Image",
                "BlurredFill",
                "BlurredGlyphs",
                "DynamicImage",
                "ImageNine",
                "BlurredRect",
                "Glyphs",
                "Fragment",
            ]),
        );
        need("Fill rule (fills)", &c.fill_rules, strs(&["NonZero", "EvenOdd"]));
        need("Fill rule (clips)", &c.clip_rules, strs(&["NonZero", "EvenOdd"]));
        let mix = [
            "Normal",
            "Multiply",
            "Screen",
            "Overlay",
            "Darken",
            "Lighten",
            "ColorDodge",
            "ColorBurn",
            "HardLight",
            "SoftLight",
            "Difference",
            "Exclusion",
            "Hue",
            "Saturation",
            "Color",
            "Luminosity",
        ];
        need("Mix", &c.mix, strs(&mix));
        let compose = [
            "Clear",
            "Copy",
            "Dest",
            "SrcOver",
            "DestOver",
            "SrcIn",
            "DestIn",
            "SrcOut",
            "DestOut",
            "SrcAtop",
            "DestAtop",
            "Xor",
            "Plus",
            "PlusLighter",
        ];
        need("Compose", &c.compose, strs(&compose));
        let extend: BTreeSet<String> = ["linear", "radial", "two-point", "sweep"]
            .iter()
            .flat_map(|k| ["Pad", "Repeat", "Reflect"].map(|e| format!("{k}/{e}")))
            .collect();
        need("Extend", &c.extend, extend);
        need("Cap", &c.caps, strs(&["Butt", "Square", "Round"]));
        need("Join", &c.joins, strs(&["Bevel", "Miter", "Round"]));
        need("ImageQuality", &c.quality, strs(&["Low", "Medium"]));
        need("ImageFormat", &c.format, strs(&["Rgba8", "Bgra8"]));
        need("ImageAlphaType", &c.alpha, strs(&["Alpha", "AlphaPremultiplied"]));
        let on: BTreeSet<String> = c.image_brush_on.iter().map(|s| (*s).to_owned()).collect();
        need("image brush on", &on, strs(&["fill", "stroke"]));
        if !c.dashes {
            missing.push("dashed stroke".to_owned());
        }
        if !c.variable_glyphs {
            missing.push("glyph run with normalized_coords".to_owned());
        }
        if c.fragment_depth < 3 {
            missing.push(format!("append depth >= 3 (deepest {})", c.fragment_depth));
        }
        if !c.skewed {
            missing.push("non-axis-aligned transform".to_owned());
        }
        assert_eq!(c.unknown, 0, "ops the census does not know (extend the match)");
        assert!(missing.is_empty(), "the corpus does not exercise:\n  {}", missing.join("\n  "));
    }
}
