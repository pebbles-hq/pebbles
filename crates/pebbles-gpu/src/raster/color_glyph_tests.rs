use super::*;
use pebbles_render::paint::display_list::{Geometry, Op};

/// The committed COLR test font (02 R29 step 6; built by tools/render-check/fonts/make_colr_test.py).
pub(crate) fn colr_font() -> FontData {
    let bytes = include_bytes!("../../../../tools/render-check/fonts/colr-test.ttf");
    FontData::new(peniko::Blob::from(bytes.to_vec()), 0)
}

pub(crate) fn gid(font: &FontData, ch: char) -> u16 {
    let f = skrifa::FontRef::from_index(font.data.data(), 0).unwrap();
    u16::try_from(f.charmap().map(ch).unwrap().to_u32()).unwrap()
}

fn fills(frag: &ColorGlyphFragment) -> Vec<Brush> {
    frag.scene
        .ops()
        .iter()
        .filter_map(|op| if let Op::Fill { brush, .. } = op { Some(brush.clone()) } else { None })
        .collect()
}

fn build(ch: char, fg: Color) -> ColorGlyphFragment {
    let font = colr_font();
    let mut outlines = crate::prepare::glyph_outline::OutlineSource::default();
    build_fragment(&mut outlines, &font, gid(&font, ch), &[], 0, fg).expect("COLR glyph")
}

#[test]
fn colr_v0_layers_become_fills() {
    let frag = build('A', Color::BLACK);
    let f = fills(&frag);
    assert_eq!(f.len(), 2, "two layers");
    assert_eq!(f[0], Brush::Solid(Color::from_rgba8(255, 0, 0, 255)), "red square first");
    assert_eq!(f[1], Brush::Solid(Color::from_rgba8(0, 0, 255, 255)), "then the blue circle");
    // Glyph-shaped fills, not rects.
    assert!(frag.scene.ops().iter().all(|op| !matches!(op, Op::Fill { shape: Geometry::Rect(_), .. })));
    assert!(frag.bounds.width() > 500.0, "{:?}", frag.bounds);
}

#[test]
fn foreground_index_uses_run_color() {
    let green = Color::from_rgba8(0, 160, 64, 255);
    assert_eq!(fills(&build('D', green)), vec![Brush::Solid(green)]);
}

#[test]
fn colr_v1_linear_gradient_maps_stops() {
    let f = fills(&build('B', Color::BLACK));
    assert_eq!(f.len(), 1);
    let Brush::Gradient(g) = &f[0] else { panic!("gradient expected, got {:?}", f[0]) };
    let colors: Vec<_> = g
        .stops
        .iter()
        .map(|s| (s.offset, s.color.to_alpha_color::<peniko::color::Srgb>().to_rgba8()))
        .collect();
    assert_eq!(colors.len(), 2);
    assert_eq!(colors[0].0, 0.0);
    assert_eq!(colors[1].0, 1.0);
    assert_eq!(colors[0].1.to_u8_array(), [255, 0, 0, 255]);
    assert_eq!(colors[1].1.to_u8_array(), [0, 0, 255, 255]);
    assert!(matches!(g.kind, peniko::GradientKind::Linear(_)));
}

#[test]
fn composite_mode_maps_to_blend() {
    use CompositeMode as C;
    let cases = [
        (C::Clear, Mix::Normal, Compose::Clear),
        (C::Src, Mix::Normal, Compose::Copy),
        (C::Dest, Mix::Normal, Compose::Dest),
        (C::SrcOver, Mix::Normal, Compose::SrcOver),
        (C::DestOver, Mix::Normal, Compose::DestOver),
        (C::SrcIn, Mix::Normal, Compose::SrcIn),
        (C::DestIn, Mix::Normal, Compose::DestIn),
        (C::SrcOut, Mix::Normal, Compose::SrcOut),
        (C::DestOut, Mix::Normal, Compose::DestOut),
        (C::SrcAtop, Mix::Normal, Compose::SrcAtop),
        (C::DestAtop, Mix::Normal, Compose::DestAtop),
        (C::Xor, Mix::Normal, Compose::Xor),
        (C::Plus, Mix::Normal, Compose::Plus),
        (C::Screen, Mix::Screen, Compose::SrcOver),
        (C::Overlay, Mix::Overlay, Compose::SrcOver),
        (C::Darken, Mix::Darken, Compose::SrcOver),
        (C::Lighten, Mix::Lighten, Compose::SrcOver),
        (C::ColorDodge, Mix::ColorDodge, Compose::SrcOver),
        (C::ColorBurn, Mix::ColorBurn, Compose::SrcOver),
        (C::HardLight, Mix::HardLight, Compose::SrcOver),
        (C::SoftLight, Mix::SoftLight, Compose::SrcOver),
        (C::Difference, Mix::Difference, Compose::SrcOver),
        (C::Exclusion, Mix::Exclusion, Compose::SrcOver),
        (C::Multiply, Mix::Multiply, Compose::SrcOver),
        (C::HslHue, Mix::Hue, Compose::SrcOver),
        (C::HslSaturation, Mix::Saturation, Compose::SrcOver),
        (C::HslColor, Mix::Color, Compose::SrcOver),
        (C::HslLuminosity, Mix::Luminosity, Compose::SrcOver),
    ];
    for (mode, mix, compose) in cases {
        assert_eq!(blend_of(mode), BlendMode::new(mix, compose), "{mode:?}");
    }
    // Glyph C: a Multiply composite layer.
    let frag = build('C', Color::BLACK);
    assert!(
        frag.scene
            .ops()
            .iter()
            .any(|op| matches!(op, Op::PushLayer { blend, .. } if blend.mix == Mix::Multiply))
    );
}

#[test]
fn fragment_cache_is_bounded() {
    let font = colr_font();
    let mut cache = ColorGlyphCache::default();
    let a = gid(&font, 'A');
    // Distinct foreground colors make distinct keys.
    for i in 0..(COLOR_GLYPH_CACHE_MAX + 100) {
        let c = Color::from_rgba8(u8::try_from(i % 256).unwrap(), u8::try_from(i / 256).unwrap(), 0, 255);
        assert!(cache.fragment(&font, a, &[], c).is_some());
        cache.end_frame();
    }
    assert!(cache.len() <= COLOR_GLYPH_CACHE_MAX, "{}", cache.len());
    // A hit does not rebuild.
    let built = cache.built;
    cache.fragment(&font, a, &[], Color::from_rgba8(1, 2, 0, 255));
    cache.fragment(&font, a, &[], Color::from_rgba8(1, 2, 0, 255));
    assert!(cache.built <= built + 1);
}
