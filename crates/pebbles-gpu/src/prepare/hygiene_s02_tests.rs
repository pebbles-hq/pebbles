// 11 S02: colour, singular-transform, layer-alpha and coordinate-range hygiene, plus the
// seeded random-scene loop. Included into `prepare::hygiene::tests` (`hygiene_tests.rs`).

fn red_square(p: &mut Painter<'_>) {
    fill(p, Affine::IDENTITY, &Rect::new(10.0, 10.0, 50.0, 50.0));
}

#[test]
fn nan_color_skips_op_and_records_non_finite() {
    let nan = Color::new([f32::NAN, 0.0, 0.0, 1.0]);
    let stops = [Color::BLACK, Color::new([0.0, f32::INFINITY, 0.0, 1.0])];
    let gradient = peniko::Gradient::new_linear((0.0, 0.0), (10.0, 0.0)).with_stops(stops.as_slice());
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let r = Rect::new(10.0, 10.0, 50.0, 50.0);
    p.fill(Fill::NonZero, Affine::IDENTITY, nan, None, &r);
    p.stroke(&Stroke::new(2.0), Affine::IDENTITY, nan, None, &r);
    p.fill(Fill::NonZero, Affine::IDENTITY, &gradient, None, &r);
    p.draw_blurred_rounded_rect(Affine::IDENTITY, r, nan, 4.0, 2.0);
    let (plan, stats) = prepare_scene_tessellated(&scene, 100, 100);
    assert!(plan.cmds.is_empty());
    assert_eq!(stats.non_finite_skipped, 4);
    assert_eq!(stats.unsupported[Unsupported::NonFinite as usize], 4);
    // Skipped before the brush fallback: no `gradient-brush-unsupported`.
    assert_eq!(stats.fallbacks, 4);
}

#[test]
fn singular_transform_skips_silently() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    fill(&mut p, Affine::scale_non_uniform(1.0, 0.0), &Rect::new(10.0, 10.0, 50.0, 50.0));
    red_square(&mut p);
    let (plan, stats) = prepare_scene_tessellated(&scene, 100, 100);
    assert_eq!(cmd_kinds(&plan), ["triangles"]);
    assert_eq!(stats.degenerate_skipped, 1);
    assert_eq!(stats.fallbacks, 0, "no key");
}

#[test]
fn nan_layer_alpha_becomes_zero_and_keeps_stack_balanced() {
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let clip = Rect::new(0.0, 0.0, 60.0, 60.0);
    p.push_layer(Fill::NonZero, peniko::Mix::Normal, f32::NAN, Affine::IDENTITY, &clip);
    red_square(&mut p);
    p.pop_layer();
    red_square(&mut p);
    let (plan, stats) = prepare_scene_tessellated(&scene, 100, 100);
    // Alpha 0 hides the layer's content; the fill after the pop draws at full alpha, unclipped.
    assert_eq!(cmd_kinds(&plan), ["triangles"]);
    let crate::prepare::DrawCmd::Solid { clip, .. } = &plan.cmds[0] else { panic!("{:?}", plan.cmds) };
    assert_eq!(clip.scissor, crate::prepare::ScissorRect::full((100, 100)));
    assert!(plan.solid_vertices.iter().all(|v| v.color[3] == 1.0));
    assert_eq!(stats.unsupported[Unsupported::NonFinite as usize], 1);
    assert_eq!(stats.unsupported[Unsupported::LayerAlpha as usize], 0);
}

#[test]
fn huge_coordinates_record_coord_out_of_range() {
    let mut scene = Scene::new();
    fill(&mut Painter::new(&mut scene), Affine::IDENTITY, &Rect::new(10.0, 10.0, 1e7, 50.0));
    let (plan, stats) = prepare_scene_tessellated(&scene, 100, 100);
    assert_eq!(stats.unsupported[Unsupported::CoordOutOfRange as usize], 1);
    assert_eq!(cmd_kinds(&plan), ["triangles"], "still drawn");
}

/// XorShift64* (11 S02 step 5).
struct Star(u64);

impl Star {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    /// With probability 1/4 an adversarial value, else a uniform integer in `-1000..3000`.
    fn val(&mut self) -> f64 {
        const SPECIAL: [f64; 10] =
            [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -0.0, 1e-30, 1e30, f64::MAX, f64::MIN_POSITIVE, -1.0];
        if self.below(4) == 0 {
            SPECIAL[usize::try_from(self.below(10)).unwrap_or(0)]
        } else {
            self.below(4000) as f64 - 1000.0
        }
    }

    #[allow(clippy::cast_possible_truncation)] // adversarial narrowing is the point
    fn val_f32(&mut self) -> f32 {
        self.val() as f32
    }

    fn point(&mut self) -> Point {
        Point::new(self.val(), self.val())
    }

    fn affine(&mut self) -> Affine {
        match self.below(3) {
            0 => Affine::IDENTITY,
            1 => Affine::translate((self.val(), self.val())),
            _ => Affine::new([self.val(), self.val(), self.val(), self.val(), self.val(), self.val()]),
        }
    }

    fn color(&mut self) -> Color {
        if self.below(8) == 0 {
            Color::new([self.val_f32(), self.val_f32(), self.val_f32(), self.val_f32()])
        } else {
            Color::from_rgba8(200, 30, 30, 255)
        }
    }

    fn brush(&mut self) -> peniko::Brush {
        if self.below(6) == 0 {
            let stops = [self.color(), self.color()];
            let g = peniko::Gradient::new_linear(self.point(), self.point()).with_stops(stops.as_slice());
            peniko::Brush::Gradient(g)
        } else {
            peniko::Brush::Solid(self.color())
        }
    }

    fn rect(&mut self) -> Rect {
        Rect::new(self.val(), self.val(), self.val(), self.val())
    }

    fn shape(&mut self) -> Geometry {
        match self.below(4) {
            0 => Geometry::Rect(self.rect()),
            1 => Geometry::RoundedRect(kurbo::RoundedRect::from_rect(self.rect(), self.val())),
            2 => Geometry::Circle(kurbo::Circle::new(self.point(), self.val())),
            _ => {
                let mut path = BezPath::new();
                path.move_to(self.point());
                for _ in 0..self.below(5) {
                    match self.below(4) {
                        0 => path.line_to(self.point()),
                        1 => path.quad_to(self.point(), self.point()),
                        2 => path.curve_to(self.point(), self.point(), self.point()),
                        _ => path.close_path(),
                    }
                }
                Geometry::Path(path)
            }
        }
    }

    fn stroke(&mut self) -> Stroke {
        let mut s = Stroke::new(self.val()).with_miter_limit(self.val());
        if self.below(2) == 0 {
            s = s.with_join(kurbo::Join::Miter);
        }
        if self.below(3) == 0 {
            s = s.with_dashes(self.val(), [self.val(), self.val()]);
        }
        s
    }
}

/// Paint `g` (a `Geometry` is not a `Shape`; go through its path or primitive).
fn paint_geometry(p: &mut Painter<'_>, rng: &mut Star, g: &Geometry, stroke: Option<&Stroke>) {
    let (xf, brush) = (rng.affine(), rng.brush());
    match (g, stroke) {
        (Geometry::Rect(r), None) => p.fill(Fill::NonZero, xf, &brush, None, r),
        (Geometry::RoundedRect(r), None) => p.fill(Fill::EvenOdd, xf, &brush, None, r),
        (Geometry::Circle(c), None) => p.fill(Fill::NonZero, xf, &brush, None, c),
        (Geometry::Path(b), None) => p.fill(Fill::EvenOdd, xf, &brush, None, b),
        (Geometry::Rect(r), Some(s)) => p.stroke(s, xf, &brush, None, r),
        (Geometry::RoundedRect(r), Some(s)) => p.stroke(s, xf, &brush, None, r),
        (Geometry::Circle(c), Some(s)) => p.stroke(s, xf, &brush, None, c),
        (Geometry::Path(b), Some(s)) => p.stroke(s, xf, &brush, None, b),
        _ => {}
    }
}

fn random_scene(rng: &mut Star, n: usize, font: &peniko::FontData) -> Scene {
    let image = peniko::ImageBrush::new(peniko::ImageData {
        data: peniko::Blob::from(vec![255u8; 16]),
        format: peniko::ImageFormat::Rgba8,
        alpha_type: peniko::ImageAlphaType::AlphaPremultiplied,
        width: 2,
        height: 2,
    });
    let mut fragment = Scene::new();
    red_square(&mut Painter::new(&mut fragment));
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let mut depth = 0;
    for _ in 0..n {
        match rng.below(10) {
            0 | 1 => {
                let g = rng.shape();
                paint_geometry(&mut p, rng, &g, None);
            }
            2 | 3 => {
                let (g, s) = (rng.shape(), rng.stroke());
                paint_geometry(&mut p, rng, &g, Some(&s));
            }
            4 if depth < 8 => {
                let (xf, clip, alpha) = (rng.affine(), rng.rect(), rng.val_f32());
                let blend = if rng.below(4) == 0 { peniko::Mix::Multiply } else { peniko::Mix::Normal };
                p.push_layer(Fill::NonZero, blend, alpha, xf, &clip);
                depth += 1;
            }
            5 if depth > 0 => {
                p.pop_layer();
                depth -= 1;
            }
            6 => {
                let mut brush = image.clone();
                // Bypass `with_alpha`'s assert: the display list may still carry any f32.
                brush.sampler.alpha = rng.val_f32();
                p.draw_image(&brush, rng.affine());
            }
            7 => {
                let (xf, r, c, radius, sd) = (rng.affine(), rng.rect(), rng.color(), rng.val(), rng.val());
                p.draw_blurred_rounded_rect(xf, r, c, radius, sd);
            }
            8 => {
                let (size, xf, brush) = (rng.val_f32(), rng.affine(), rng.brush());
                let glyphs: Vec<_> = (0..3)
                    .map(|i| pebbles_render::paint::Glyph { id: 36 + i, x: rng.val_f32(), y: rng.val_f32() })
                    .collect();
                p.draw_glyphs(font, size, &[], &brush, xf, None, Fill::NonZero, glyphs.into_iter());
            }
            _ => {
                let xf = if rng.below(2) == 0 { None } else { Some(rng.affine()) };
                p.append(&fragment, xf);
            }
        }
    }
    for _ in 0..depth {
        p.pop_layer();
    }
    scene
}

#[test]
fn random_scenes_never_panic() {
    let mut rng = Star(0x5EED_0000_0000_0001);
    let font = crate::test_support::bundled_font();
    let scene = random_scene(&mut rng, 10_000, &font);
    let mut scratch = crate::prepare::PrepareScratch::default();
    let mut stats = crate::stats::FrameStats::default();
    let input =
        crate::prepare::PrepareInput { target_size: (1024, 768), root_transform: Affine::IDENTITY, vertex_limit: u32::MAX };
    crate::prepare::prepare(&scene, &input, &crate::test_support::webgl2_limits(), &mut scratch, &mut stats);
    let plan = &scratch.plan;
    assert!(plan.solid_vertices.iter().all(|v| v.pos.iter().all(|c| c.is_finite())), "solid vertex");
    assert!(plan.shadows.iter().all(|s| s.rect.iter().chain(&s.params).all(|c| c.is_finite())), "shadow quad");
    assert!(plan.images.iter().all(|i| i.quad.xform.iter().chain(&i.quad.origin).all(|c| c.is_finite())));
    assert!(plan.glyphs.iter().all(|g| g.origin.iter().all(|c| c.is_finite())), "glyph origin");
    assert_eq!(scratch.layers.depth(), 0, "layer stack balanced");
    assert_eq!(scratch.clips.depth(), 0, "clip stack balanced");
    assert!(stats.non_finite_skipped > 0 && stats.degenerate_skipped > 0, "{stats:?}");
}

/// Out-of-range curves are flattened at a tolerance proportional to their size, so the vertex
/// count stays bounded. At the fixed 0.1 px tolerance a radius-1e12 circle needs millions of
/// vertices, and a radius-1.8e308 one exhausts memory (found when this test file OOM-killed
/// the desktop).
#[test]
fn huge_out_of_range_circle_has_bounded_vertices() {
    for radius in [1e12, 1e30, f64::MAX] {
        let mut scene = Scene::new();
        Painter::new(&mut scene).fill(
            Fill::NonZero,
            Affine::IDENTITY,
            Color::BLACK,
            None,
            &kurbo::Circle::new((100.0, 100.0), radius),
        );
        let (plan, _) = prepare_scene_tessellated(&scene, 200, 200);
        assert!(plan.solid_vertices.len() < 20_000, "radius {radius}: {} vertices", plan.solid_vertices.len());
    }
}
