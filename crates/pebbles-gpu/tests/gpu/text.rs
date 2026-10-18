//! Glyph rendering (C09, C04).

use super::*;

/// C09: a 40-glyph label whose last glyph changes every frame for 500 frames keeps its first
/// glyph (no disappearing text), and steady-state frames rasterize at most 2 glyphs.
#[test]
#[ignore = "gpu"]
fn glyphs_survive_500_frames() {
    let Some((device, queue, mut renderer)) = setup("glyphs_survive_500_frames") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    let (w, h) = (640u32, 48u32);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("pebbles.gpu.test.target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let params = FrameParams::new(Color::WHITE);
    let mut targets = FrameTargets::new();
    // Ink of the first glyph's cell (x in 0..16).
    let first_ink = |rgba: &[u8]| {
        (0..h)
            .flat_map(|y| (0..16).map(move |x| (x, y)))
            .filter(|&(x, y)| !is_white(pixel(rgba, w, x, y)))
            .count()
    };
    let mut first = 0usize;
    for frame in 1..=500u32 {
        let mut scene = Scene::new();
        let ids = (0..39u32).map(|i| 36 + i % 26).chain(std::iter::once(36 + frame % 26));
        Painter::new(&mut scene).draw_glyphs(
            &font,
            16.0,
            &[],
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            ids.enumerate().map(|(i, id)| Glyph { id, x: 2.0 + 15.5 * i as f32, y: 30.0 }),
        );
        let stats = renderer
            .render(RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, &mut targets), &scene, &params)
            .expect("render");
        if frame == 1 || frame == 500 {
            queue.submit([]);
            let ink = first_ink(&readback(&device, &queue, &texture));
            if frame == 1 {
                assert!(ink > 10, "first glyph has no ink in frame 1");
                first = ink;
            } else {
                let diff = (ink as f64 - first as f64).abs() / first as f64;
                assert!(diff <= 0.02, "first glyph ink {first} -> {ink}");
                assert!(stats.glyphs_rasterized <= 2, "frame 500 rasterized {}", stats.glyphs_rasterized);
            }
        }
    }
}

/// C04: weight travels only in normalized coords — "HHHH" at 32 px renders bolder at wght 700
/// than at wght 400 (bundled Inter Variable). Inter's 'H' advance barely varies with weight
/// (23.78 → 23.91 px at 32 px, per swash and skrifa), so the run is not > 2 px wider as the card
/// assumed; the assertion is on ink (thicker stems) and a non-narrower run instead.
#[test]
#[ignore = "gpu"]
fn bold_run_is_wider_than_regular() {
    let Some((device, queue, mut renderer)) = setup("bold_run_is_wider_than_regular") else { return };
    let bytes = pebbles_render::fonts::builtin_fonts()[0].1;
    let font = peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0);
    let f = swash::FontRef::from_index(bytes, 0).expect("Inter parses");
    let h = f.charmap().map('H');
    let ink = |renderer: &mut Renderer, weight: f32| -> (u32, u64) {
        let coords: Vec<i16> =
            f.variations().normalized_coords([(swash::tag_from_str_lossy("wght"), weight)]).collect();
        let advance = f.glyph_metrics(&coords).scale(32.0).advance_width(h);
        let mut scene = Scene::new();
        Painter::new(&mut scene).draw_glyphs(
            &font,
            32.0,
            &coords,
            &Brush::Solid(Color::BLACK),
            Affine::IDENTITY,
            None,
            Fill::NonZero,
            (0..4).map(|i| Glyph { id: u32::from(h), x: 4.0 + advance * i as f32, y: 40.0 }),
        );
        let rgba = render_scene(&device, &queue, renderer, &scene, (200, 56), Color::WHITE);
        let cols: Vec<u32> =
            (0..200).filter(|&x| (0..56).any(|y| !is_white(pixel(&rgba, 200, x, y)))).collect();
        let width = cols.last().map_or(0, |last| last - cols[0] + 1);
        let coverage = rgba.as_chunks::<4>().0.iter().map(|p| u64::from(255 - p[0])).sum();
        (width, coverage)
    };
    let (regular_w, regular_ink) = ink(&mut renderer, 400.0);
    let (bold_w, bold_ink) = ink(&mut renderer, 700.0);
    assert!(bold_w >= regular_w, "bold run {bold_w} px narrower than regular {regular_w} px");
    assert!(bold_ink * 10 > regular_ink * 13, "bold ink {bold_ink} not > 1.3 × regular {regular_ink}");
}

// ---- R20: the glyph outline route ----

fn inter() -> peniko::FontData {
    peniko::FontData::new(peniko::Blob::from(pebbles_render::fonts::builtin_fonts()[0].1.to_vec()), 0)
}

fn glyph_id(ch: char) -> u32 {
    let f = inter();
    u32::from(swash::FontRef::from_index(f.data.data(), 0).expect("Inter").charmap().map(ch))
}

/// Ink bounding box `(x0, y0, x1, y1)` (inclusive) of non-white pixels.
fn ink_bbox(rgba: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for y in 0..h {
        for x in 0..w {
            if !is_white(pixel(rgba, w, x, y)) {
                b = Some(b.map_or((x, y, x, y), |(a, c, e, f)| (a.min(x), c.min(y), e.max(x), f.max(y))));
            }
        }
    }
    b
}

fn draw_run(scene: &mut Scene, size: f32, t: Affine, text: &str) {
    let glyphs: Vec<Glyph> = text
        .chars()
        .enumerate()
        .map(|(i, c)| Glyph { id: glyph_id(c), x: i as f32 * size * 0.3, y: 0.0 })
        .collect();
    let brush = Brush::Solid(Color::BLACK);
    Painter::new(scene).draw_glyphs(&inter(), size, &[], &brush, t, None, Fill::NonZero, glyphs.into_iter());
}

#[test]
#[ignore = "gpu"]
fn rotated_text_is_rotated() {
    let Some((device, queue, mut r)) = setup("rotated_text_is_rotated") else { return };
    let mut s = Scene::new();
    draw_run(
        &mut s,
        32.0,
        Affine::translate((80.0, 20.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2),
        "IIII",
    );
    let px = render_scene(&device, &queue, &mut r, &s, (160, 160), Color::WHITE);
    let (x0, y0, x1, y1) = ink_bbox(&px, 160, 160).expect("ink");
    assert!(y1 - y0 > x1 - x0, "ink bbox {x0},{y0}–{x1},{y1} must be taller than wide");
}

#[test]
#[ignore = "gpu"]
fn rotated_text_is_not_upscaled_bitmap() {
    let Some((device, queue, mut r)) = setup("rotated_text_is_not_upscaled_bitmap") else { return };
    let mut s = Scene::new();
    draw_run(&mut s, 32.0, Affine::translate((10.0, 150.0)) * Affine::scale(4.0) * Affine::rotate(0.01), "I");
    let px = render_scene(&device, &queue, &mut r, &s, (200, 200), Color::WHITE);
    let opaque = (0..200)
        .flat_map(|y| (0..200).map(move |x| (x, y)))
        .any(|(x, y)| pixel(&px, 200, x, y)[..3] == [0, 0, 0]);
    assert!(opaque, "a crisp stem has fully opaque ink pixels");
}

#[test]
#[ignore = "gpu"]
fn huge_glyph_uses_outline() {
    let Some((device, queue, mut r)) = setup("huge_glyph_uses_outline") else { return };
    let mut s = Scene::new();
    draw_run(&mut s, 2000.0, Affine::translate((-200.0, 1500.0)), "W");
    let _ = render_scene(&device, &queue, &mut r, &s, (256, 256), Color::WHITE);
    let stats = r.last_frame_stats();
    assert_eq!(stats.glyphs_rasterized, 0, "no atlas raster for a 2000-px glyph");
    assert!(stats.tier_b_draws + stats.tier_c_draws >= 1, "{stats:?}");
}

// ---- R26: text quality (physical-size raster, subpixel bins, baseline snap) ----

/// `text` as a run, pen x advanced 10 px per glyph from `x`, baseline `y`.
fn pen_run(scene: &mut Scene, size: f32, t: Affine, coords: &[i16], text: &str, x: f32, y: f32) {
    let glyphs: Vec<Glyph> =
        text.chars().enumerate().map(|(i, c)| Glyph { id: glyph_id(c), x: x + 10.0 * i as f32, y }).collect();
    let brush = Brush::Solid(Color::BLACK);
    Painter::new(scene).draw_glyphs(
        &inter(),
        size,
        coords,
        &brush,
        t,
        None,
        Fill::NonZero,
        glyphs.into_iter(),
    );
}

fn render_run(
    r: &mut (wgpu::Device, wgpu::Queue, Renderer),
    size: (u32, u32),
    f: impl FnOnce(&mut Scene),
) -> Vec<u8> {
    let mut s = Scene::new();
    f(&mut s);
    render_scene(&r.0, &r.1, &mut r.2, &s, size, Color::WHITE)
}

/// R26: "Hamburg" at 16 px under `scale(2)` is rasterized at 32 px device size: twice the ink
/// height, with solid stems (an up-scaled 16-px bitmap would have no near-black pixels).
#[test]
#[ignore = "gpu"]
fn hidpi_glyphs_are_rasterized_at_device_size() {
    let Some(mut r) = setup("hidpi_glyphs_are_rasterized_at_device_size") else { return };
    let a = render_run(&mut r, (200, 40), |s| pen_run(s, 16.0, Affine::IDENTITY, &[], "Hamburg", 4.0, 28.0));
    let b =
        render_run(&mut r, (400, 80), |s| pen_run(s, 16.0, Affine::scale(2.0), &[], "Hamburg", 4.0, 28.0));
    let (_, ay0, _, ay1) = ink_bbox(&a, 200, 40).expect("ink at 1x");
    let (_, by0, _, by1) = ink_bbox(&b, 400, 80).expect("ink at 2x");
    let (ha, hb) = (f64::from(ay1 - ay0 + 1), f64::from(by1 - by0 + 1));
    assert!((hb / ha - 2.0).abs() <= 0.3, "ink heights {ha} -> {hb}");
    let dark = b.as_chunks::<4>().0.iter().filter(|p| p[0] <= 10 && p[1] <= 10 && p[2] <= 10).count();
    assert!(dark >= 20, "only {dark} near-black pixels at 2x");
}

/// R26: pen x 10.0 vs 10.25 land in different subpixel bins; 10.0 vs 10.05 in the same one.
#[test]
#[ignore = "gpu"]
fn subpixel_bins_differ() {
    let Some(mut r) = setup("subpixel_bins_differ") else { return };
    let at = |r: &mut _, x: f32| {
        render_run(r, (40, 32), |s| pen_run(s, 16.0, Affine::IDENTITY, &[], "i", x, 24.0))
    };
    let base = at(&mut r, 10.0);
    assert_ne!(base, at(&mut r, 10.25), "quarter-pixel shift must change the raster");
    assert_eq!(base, at(&mut r, 10.05), "same bin must render identically");
}

/// R26: baselines snap to whole pixels: y 20.3 renders like 20.0; 20.6 like 21.0 (one row down).
#[test]
#[ignore = "gpu"]
fn baseline_snaps_to_whole_pixels() {
    let Some(mut r) = setup("baseline_snaps_to_whole_pixels") else { return };
    let (w, h) = (40u32, 40u32);
    let at =
        |r: &mut _, y: f32| render_run(r, (w, h), |s| pen_run(s, 16.0, Affine::IDENTITY, &[], "H", 10.0, y));
    let base = at(&mut r, 20.0);
    assert_eq!(base, at(&mut r, 20.3));
    let down = at(&mut r, 20.6);
    let row = (w * 4) as usize;
    assert_eq!(&down[row..], &base[..base.len() - row], "20.6 must be 20.0 shifted down one row");
    assert!(ink_bbox(&base, w, h).is_some());
}

/// R26: Inter 'H' at wght 700 has ≥ 15% more ink pixels than at wght 400.
#[test]
#[ignore = "gpu"]
fn variable_weight_changes_ink() {
    let Some(mut r) = setup("variable_weight_changes_ink") else { return };
    let font = inter();
    let f = swash::FontRef::from_index(font.data.data(), 0).expect("Inter");
    let coords = |w: f32| -> Vec<i16> {
        f.variations().normalized_coords([(swash::tag_from_str_lossy("wght"), w)]).collect()
    };
    let ink = |r: &mut _, c: &[i16]| {
        let px = render_run(r, (64, 48), |s| pen_run(s, 32.0, Affine::IDENTITY, c, "H", 8.0, 38.0));
        px.as_chunks::<4>().0.iter().filter(|p| !is_white(**p)).count()
    };
    let (regular, bold) = (ink(&mut r, &coords(400.0)), ink(&mut r, &coords(700.0)));
    assert!(bold * 100 >= regular * 115, "bold {bold} vs regular {regular}");
}

/// R26: one Renderer renders a 200-glyph scene 300 times; frame 300's ink equals frame 1's.
#[test]
#[ignore = "gpu"]
fn text_survives_300_frames() {
    let Some(mut r) = setup("text_survives_300_frames") else { return };
    let text: String = "The quick brown fox jumps over the lazy dog ".chars().cycle().take(200).collect();
    let scene = |s: &mut Scene| {
        for (line, chunk) in text.as_bytes().chunks(40).enumerate() {
            let chunk = std::str::from_utf8(chunk).expect("ascii");
            pen_run(s, 14.0, Affine::IDENTITY, &[], chunk, 4.0, 20.0 + 20.0 * line as f32);
        }
    };
    let count = |px: &[u8]| px.as_chunks::<4>().0.iter().filter(|p| !is_white(**p)).count();
    let first = count(&render_run(&mut r, (420, 110), scene));
    assert!(first > 500, "frame 1 ink {first}");
    for _ in 2..300 {
        render_run(&mut r, (420, 110), scene);
    }
    assert_eq!(count(&render_run(&mut r, (420, 110), scene)), first);
}

// ---- R23: color glyphs (system color font; never bundled) ----

/// The first font under `/usr/share/fonts` with a `COLR` or `CBDT` table, and its glyph for 😀.
fn system_color_font() -> Option<(peniko::FontData, u32)> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "ttf" || x == "otf") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(std::path::Path::new("/usr/share/fonts"), &mut files);
    files.sort();
    for path in files {
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Some(f) = swash::FontRef::from_index(&bytes, 0) else { continue };
        let color = [b"COLR", b"CBDT"].iter().any(|t| f.table(swash::tag_from_bytes(t)).is_some());
        let id = f.charmap().map('😀');
        if color && id != 0 {
            return Some((peniko::FontData::new(peniko::Blob::from(bytes), 0), u32::from(id)));
        }
    }
    None
}

fn emoji_scene(font: &peniko::FontData, id: u32, alpha: f32, t: Affine) -> Scene {
    let mut s = Scene::new();
    let brush = Brush::Solid(Color::BLACK.multiply_alpha(alpha));
    let g = [Glyph { id, x: 8.0, y: 56.0 }];
    Painter::new(&mut s).draw_glyphs(font, 48.0, &[], &brush, t, None, Fill::NonZero, g.into_iter());
    s
}

/// R23: color glyphs are uploaded premultiplied (no RGB above alpha over a transparent base)
/// and keep their colors.
#[test]
#[ignore = "requires a GPU and a system color-emoji font"]
fn color_glyph_is_premultiplied() {
    let Some((font, id)) = system_color_font() else {
        eprintln!("color_glyph_is_premultiplied: no system color font, skipped");
        return;
    };
    let Some((device, queue, mut r)) = setup("color_glyph_is_premultiplied") else { return };
    let px = render_scene(
        &device,
        &queue,
        &mut r,
        &emoji_scene(&font, id, 1.0, Affine::IDENTITY),
        (72, 72),
        Color::TRANSPARENT,
    );
    let px = px.as_chunks::<4>().0;
    assert!(px.iter().all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]), "a channel exceeds alpha");
    let saturated = px
        .iter()
        .filter(|p| {
            let (mx, mn) = (p[0].max(p[1]).max(p[2]), p[0].min(p[1]).min(p[2]));
            mx > 0 && f32::from(mx - mn) / f32::from(mx) > 0.3
        })
        .count();
    assert!(saturated >= 50, "only {saturated} saturated pixels");
}

/// R23: a run at alpha 0.5 draws its color glyphs at half opacity.
#[test]
#[ignore = "requires a GPU and a system color-emoji font"]
fn color_glyph_follows_run_alpha() {
    let Some((font, id)) = system_color_font() else {
        eprintln!("color_glyph_follows_run_alpha: no system color font, skipped");
        return;
    };
    let Some((device, queue, mut r)) = setup("color_glyph_follows_run_alpha") else { return };
    let px = render_scene(
        &device,
        &queue,
        &mut r,
        &emoji_scene(&font, id, 0.5, Affine::IDENTITY),
        (72, 72),
        Color::TRANSPARENT,
    );
    let max_a = px.as_chunks::<4>().0.iter().map(|p| p[3]).max().unwrap_or(0);
    assert!((120..=136).contains(&max_a), "max alpha {max_a}");
}

/// R23 2a: a rotated bitmap-only color glyph draws as a transformed quad (no fallback).
#[test]
#[ignore = "requires a GPU and a system color-emoji font"]
fn rotated_bitmap_glyph_draws_as_quad() {
    let Some((font, id)) = system_color_font() else {
        eprintln!("rotated_bitmap_glyph_draws_as_quad: no system color font, skipped");
        return;
    };
    let Some((device, queue, mut r)) = setup("rotated_bitmap_glyph_draws_as_quad") else { return };
    let t = Affine::rotate_about(0.5, kurbo::Point::new(36.0, 36.0));
    let px =
        render_scene(&device, &queue, &mut r, &emoji_scene(&font, id, 1.0, t), (72, 72), Color::TRANSPARENT);
    let stats = r.last_frame_stats();
    assert_eq!(stats.unsupported[pebbles_gpu::Unsupported::ColorGlyphTransform as usize], 0);
    let ink = px.as_chunks::<4>().0.iter().filter(|p| p[3] > 128).count();
    assert!(ink > 500, "rotated emoji ink {ink}");
}

// ---- R29: COLR glyphs drawn by our pipeline ----

fn colr_font() -> peniko::FontData {
    let bytes = include_bytes!("../../../../tools/render-check/fonts/colr-test.ttf");
    peniko::FontData::new(peniko::Blob::from(bytes.to_vec()), 0)
}

/// R29: glyph `A` of the COLR test font (red square under a blue circle) rotated 30° about
/// the canvas center: both colors are drawn, and every inked pixel lies inside the rotated
/// glyph box (font units 0..800 → 0..80 px at 100 px).
#[test]
#[ignore = "gpu"]
fn color_glyph_renders_under_rotation() {
    let Some((device, queue, mut r)) = setup("color_glyph_renders_under_rotation") else { return };
    let font = colr_font();
    let id = {
        let f = swash::FontRef::from_index(font.data.data(), 0).expect("colr-test");
        u32::from(f.charmap().map('A'))
    };
    let t = Affine::rotate_about(std::f64::consts::FRAC_PI_6, kurbo::Point::new(80.0, 80.0));
    let mut s = Scene::new();
    let g = [Glyph { id, x: 40.0, y: 120.0 }];
    Painter::new(&mut s).draw_glyphs(
        &font,
        100.0,
        &[],
        &Brush::Solid(Color::BLACK),
        t,
        None,
        Fill::NonZero,
        g.into_iter(),
    );
    let px = render_scene(&device, &queue, &mut r, &s, (160, 160), Color::WHITE);
    let stats = r.last_frame_stats();
    assert_eq!(stats.unsupported[pebbles_gpu::Unsupported::ColorGlyphTransform as usize], 0);
    let (mut red, mut blue) = (0, 0);
    // The glyph box: pen (40, 120), font units 0..800 at k = 0.1 → x 40..120, y 40..120 (y-down).
    let inv = t.inverse();
    for y in 0..160u32 {
        for x in 0..160u32 {
            let p = pixel(&px, 160, x, y);
            if is_white(p) {
                continue;
            }
            red += u32::from(p[0] > 200 && p[1] < 60 && p[2] < 60);
            blue += u32::from(p[2] > 200 && p[0] < 60 && p[1] < 60);
            let q = inv * kurbo::Point::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            assert!(
                (39.0..=121.0).contains(&q.x) && (39.0..=121.0).contains(&q.y),
                "ink {p:?} at ({x},{y}) outside the rotated glyph box"
            );
        }
    }
    assert!(red > 100 && blue > 100, "red {red} blue {blue}");
}

// ---- R39: COLR glyphs cached in the color atlas ----

fn colr_label(font: &peniko::FontData, ch: char, x: f32) -> Scene {
    let id = {
        let f = swash::FontRef::from_index(font.data.data(), 0).expect("colr-test");
        u32::from(f.charmap().map(ch))
    };
    let mut s = Scene::new();
    let g = [Glyph { id, x, y: 90.0 }];
    Painter::new(&mut s).draw_glyphs(
        font,
        64.0,
        &[],
        &Brush::Solid(Color::BLACK),
        Affine::IDENTITY,
        None,
        Fill::NonZero,
        g.into_iter(),
    );
    s
}

/// R39: frame 1 renders the COLR glyph into the color atlas once; frame 2 of the same label
/// only draws a quad (no extra pass, nothing rasterized).
#[test]
#[ignore = "gpu"]
fn color_glyph_atlas_hit_has_no_extra_pass() {
    let Some((device, queue, mut r)) = setup("color_glyph_atlas_hit_has_no_extra_pass") else { return };
    let font = colr_font();
    let s = colr_label(&font, 'A', 12.25);
    let first = render_scene(&device, &queue, &mut r, &s, (96, 96), Color::WHITE);
    assert_eq!(r.last_frame_stats().color_glyph_renders, 1);
    let second = render_scene(&device, &queue, &mut r, &s, (96, 96), Color::WHITE);
    let stats = r.last_frame_stats();
    assert_eq!(stats.color_glyph_renders, 0, "frame 2 hits the atlas");
    assert_eq!(stats.render_passes, 1, "no extra pass on a hit");
    assert_eq!(first, second);
    assert!(first.as_chunks::<4>().0.iter().any(|p| p[2] > 200 && p[0] < 60), "blue layer drawn");
}

/// R39: the atlas route matches the vector route of the same glyph at the same position within
/// the Text limits (max channel delta ≤ 3 on ≥ 99 % of pixels).
#[test]
#[ignore = "gpu"]
fn atlas_colr_matches_vector() {
    let Some((device, queue, mut r)) = setup("atlas_colr_matches_vector") else { return };
    let font = colr_font();
    for (ch, x) in [('A', 12.0), ('A', 12.5), ('D', 13.25)] {
        let s = colr_label(&font, ch, x);
        r.set_colr_atlas_enabled(true);
        let atlas = render_scene(&device, &queue, &mut r, &s, (96, 96), Color::WHITE);
        r.set_colr_atlas_enabled(false);
        let vector = render_scene(&device, &queue, &mut r, &s, (96, 96), Color::WHITE);
        let (a, v) = (atlas.as_chunks::<4>().0, vector.as_chunks::<4>().0);
        let over =
            a.iter().zip(v).filter(|(p, q)| p.iter().zip(q.iter()).any(|(x, y)| x.abs_diff(*y) > 3)).count();
        let max = a
            .iter()
            .zip(v)
            .flat_map(|(p, q)| p.iter().zip(q.iter()).map(|(x, y)| x.abs_diff(*y)))
            .max()
            .unwrap_or(0);
        assert!(over * 100 <= a.len(), "{ch} at x {x}: {over} pixels over 3 (max Δ {max})");
    }
}
