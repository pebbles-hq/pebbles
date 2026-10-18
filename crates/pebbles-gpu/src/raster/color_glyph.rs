//! COLR v0/v1 color glyphs (02 R29, §7.7, DR-23): skrifa walks the paint graph and this
//! painter records it as a display-list fragment (fills, gradients, glyph/box clips, composite
//! layers) that prepare splices into the frame — drawn by our pipeline under any transform,
//! never rasterized on the CPU.
//!
//! Fragment space is **font units, y-up** (the caller maps it with
//! `M · translate(pen) · G · scale(k, −k)`, `k = font_size / upem`): COLR transforms and
//! gradient geometry are in font units, so no per-callback flip is needed. Ops are recorded
//! through the public `Painter` (`Op` variants are `#[non_exhaustive]`).

use std::collections::HashMap;
use std::sync::Arc;

use kurbo::{Affine, BezPath, Rect, Shape as _};
use pebbles_render::paint::{Painter, Scene};
use peniko::{BlendMode, Brush, Color, Compose, Extend, Fill, FontData, Gradient, Mix};
use skrifa::color::{Brush as ColrBrush, ColorPainter, ColorStop, CompositeMode, Transform};
use skrifa::raw::types::BoundingBox;
use skrifa::{GlyphId, MetadataProvider};

/// Most cached fragments kept past a frame (R29 step 3).
pub(crate) const COLOR_GLYPH_CACHE_MAX: usize = 512;

/// One COLR glyph as display-list ops in font units (y-up) and its bounds there.
#[derive(Clone)]
pub(crate) struct ColorGlyphFragment {
    pub scene: Arc<Scene>,
    pub bounds: Rect,
}

/// `CompositeMode` → peniko `BlendMode` (09 §4 vocabulary).
pub(crate) fn blend_of(mode: CompositeMode) -> BlendMode {
    use CompositeMode as C;
    let compose = |c| BlendMode::new(Mix::Normal, c);
    let mix = |m| BlendMode::new(m, Compose::SrcOver);
    match mode {
        C::Clear => compose(Compose::Clear),
        C::Src => compose(Compose::Copy),
        C::Dest => compose(Compose::Dest),
        C::DestOver => compose(Compose::DestOver),
        C::SrcIn => compose(Compose::SrcIn),
        C::DestIn => compose(Compose::DestIn),
        C::SrcOut => compose(Compose::SrcOut),
        C::DestOut => compose(Compose::DestOut),
        C::SrcAtop => compose(Compose::SrcAtop),
        C::DestAtop => compose(Compose::DestAtop),
        C::Xor => compose(Compose::Xor),
        C::Plus => compose(Compose::Plus),
        C::Screen => mix(Mix::Screen),
        C::Overlay => mix(Mix::Overlay),
        C::Darken => mix(Mix::Darken),
        C::Lighten => mix(Mix::Lighten),
        C::ColorDodge => mix(Mix::ColorDodge),
        C::ColorBurn => mix(Mix::ColorBurn),
        C::HardLight => mix(Mix::HardLight),
        C::SoftLight => mix(Mix::SoftLight),
        C::Difference => mix(Mix::Difference),
        C::Exclusion => mix(Mix::Exclusion),
        C::Multiply => mix(Mix::Multiply),
        C::HslHue => mix(Mix::Hue),
        C::HslSaturation => mix(Mix::Saturation),
        C::HslColor => mix(Mix::Color),
        C::HslLuminosity => mix(Mix::Luminosity),
        // SrcOver and unknown modes.
        _ => BlendMode::default(),
    }
}

/// Cache identity (R29 step 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FragmentKey {
    font_blob: u64,
    font_index: u32,
    gid: u16,
    coords_hash: u64,
    palette: u16,
    foreground: [u8; 4],
}

/// Per-font: whether it has a COLR table (checked once per font).
#[derive(Default)]
pub(crate) struct ColorGlyphCache {
    map: HashMap<FragmentKey, (Option<ColorGlyphFragment>, u64)>,
    has_colr: HashMap<(u64, u32), bool>,
    outlines: crate::prepare::glyph_outline::OutlineSource,
    frame: u64,
    /// Fragment builds (cache misses), for tests.
    #[cfg(test)]
    pub(crate) built: usize,
}

impl ColorGlyphCache {
    /// Whether `font` has a COLR table (cached per font).
    pub(crate) fn font_has_colr(&mut self, font: &FontData) -> bool {
        *self.has_colr.entry((font.data.id(), font.index)).or_insert_with(|| {
            skrifa::FontRef::from_index(font.data.data(), font.index)
                .is_ok_and(|f| f.table_data(skrifa::raw::types::Tag::new(b"COLR")).is_some())
        })
    }

    /// The fragment of COLR glyph `gid` (palette 0; index `0xFFFF` = `foreground`), or `None`
    /// when the glyph has no COLR representation.
    pub(crate) fn fragment(
        &mut self,
        font: &FontData,
        gid: u16,
        coords: &[i16],
        foreground: Color,
    ) -> Option<ColorGlyphFragment> {
        let key = FragmentKey {
            font_blob: font.data.id(),
            font_index: font.index,
            gid,
            coords_hash: if coords.is_empty() {
                0
            } else {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                coords.hash(&mut h);
                h.finish().max(1)
            },
            palette: 0,
            foreground: foreground.to_rgba8().to_u8_array(),
        };
        let frame = self.frame;
        if let Some((frag, used)) = self.map.get_mut(&key) {
            *used = frame;
            return frag.clone();
        }
        #[cfg(test)]
        {
            self.built += 1;
        }
        let frag = build_fragment(&mut self.outlines, font, gid, coords, 0, foreground);
        self.map.insert(key, (frag.clone(), frame));
        frag
    }

    /// End of frame: above the cap, drop entries not used this frame, oldest first.
    pub(crate) fn end_frame(&mut self) {
        if self.map.len() > COLOR_GLYPH_CACHE_MAX {
            let frame = self.frame;
            let mut stale: Vec<(u64, FragmentKey)> =
                self.map.iter().filter(|(_, (_, u))| *u < frame).map(|(k, (_, u))| (*u, *k)).collect();
            stale.sort_unstable_by_key(|s| s.0);
            let excess = self.map.len() - COLOR_GLYPH_CACHE_MAX;
            for (_, k) in stale.into_iter().take(excess) {
                self.map.remove(&k);
            }
        }
        // One bool per font seen; reset when many fonts came and went (bounded, AP-8).
        if self.has_colr.len() > 256 {
            self.has_colr.clear();
        }
        self.frame += 1;
    }

    /// Cached fragment count (census `color_glyph_fragments`).
    #[cfg(any(test, debug_assertions))]
    pub(crate) fn len(&self) -> usize {
        self.map.len()
    }
}

/// skrifa's paint graph for `gid` → a fragment, or `None` without a COLR glyph.
pub(crate) fn build_fragment(
    outlines: &mut crate::prepare::glyph_outline::OutlineSource,
    font: &FontData,
    gid: u16,
    coords: &[i16],
    palette_index: u16,
    foreground: Color,
) -> Option<ColorGlyphFragment> {
    let face = skrifa::FontRef::from_index(font.data.data(), font.index).ok()?;
    let glyph = face.color_glyphs().get(GlyphId::new(u32::from(gid)))?;
    let palette: Vec<Color> = face
        .color_palettes()
        .get(palette_index)
        .map(|p| p.colors().iter().map(|c| Color::from_rgba8(c.red, c.green, c.blue, c.alpha)).collect())
        .unwrap_or_default();
    let upem =
        f32::from(skrifa::raw::TableProvider::head(&face).map(|h| h.units_per_em()).unwrap_or(1000).max(1));
    let mut scene = Scene::new();
    let mut painter = FragmentPainter {
        p: Painter::new(&mut scene),
        font,
        outlines,
        coords,
        upem,
        palette,
        foreground,
        xf: vec![Affine::IDENTITY],
        clip_bounds: Vec::new(),
        bounds: None,
    };
    let location: Vec<skrifa::instance::NormalizedCoord> =
        coords.iter().map(|&c| skrifa::instance::NormalizedCoord::from_bits(c)).collect();
    glyph.paint(skrifa::instance::LocationRef::new(&location), &mut painter).ok()?;
    let bounds = painter.bounds.unwrap_or(Rect::ZERO);
    Some(ColorGlyphFragment { scene: Arc::new(scene), bounds })
}

/// Records skrifa's callbacks as display-list ops (R29 step 2).
struct FragmentPainter<'a, 's> {
    p: Painter<'s>,
    font: &'a FontData,
    outlines: &'a mut crate::prepare::glyph_outline::OutlineSource,
    coords: &'a [i16],
    upem: f32,
    palette: Vec<Color>,
    foreground: Color,
    /// Font-space transform stack (`push_transform` concatenates).
    xf: Vec<Affine>,
    /// Bounds of the open clips in fragment space (the area a `fill` paints).
    clip_bounds: Vec<Rect>,
    /// Union of every drawn area (fragment space).
    bounds: Option<Rect>,
}

impl FragmentPainter<'_, '_> {
    fn top(&self) -> Affine {
        self.xf.last().copied().unwrap_or(Affine::IDENTITY)
    }

    fn color(&self, index: u16, alpha: f32) -> Color {
        let c = if index == 0xFFFF {
            self.foreground
        } else {
            self.palette.get(usize::from(index)).copied().unwrap_or(Color::TRANSPARENT)
        };
        c.multiply_alpha(alpha)
    }

    fn stops(&self, stops: &[ColorStop]) -> Vec<peniko::ColorStop> {
        stops
            .iter()
            .map(|s| peniko::ColorStop::from((s.offset, self.color(s.palette_index, s.alpha))))
            .collect()
    }

    /// The COLR brush as a peniko brush in the current paint space.
    fn brush(&self, b: &ColrBrush<'_>) -> Brush {
        let ext = |e: skrifa::color::Extend| match e {
            skrifa::color::Extend::Repeat => Extend::Repeat,
            skrifa::color::Extend::Reflect => Extend::Reflect,
            _ => Extend::Pad,
        };
        let pt = |p: skrifa::raw::types::Point<f32>| (f64::from(p.x), f64::from(p.y));
        match b {
            ColrBrush::Solid { palette_index, alpha } => Brush::Solid(self.color(*palette_index, *alpha)),
            ColrBrush::LinearGradient { p0, p1, color_stops, extend } => Brush::Gradient(
                Gradient::new_linear(pt(*p0), pt(*p1))
                    .with_extend(ext(*extend))
                    .with_stops(self.stops(color_stops).as_slice()),
            ),
            ColrBrush::RadialGradient { c0, r0, c1, r1, color_stops, extend } => Brush::Gradient(
                Gradient::new_two_point_radial(pt(*c0), *r0, pt(*c1), *r1)
                    .with_extend(ext(*extend))
                    .with_stops(self.stops(color_stops).as_slice()),
            ),
            ColrBrush::SweepGradient { c0, start_angle, end_angle, color_stops, extend } => Brush::Gradient(
                Gradient::new_sweep(pt(*c0), start_angle.to_radians(), end_angle.to_radians())
                    .with_extend(ext(*extend))
                    .with_stops(self.stops(color_stops).as_slice()),
            ),
        }
    }

    /// Glyph `gid`'s outline in font units (y-up), from R20's outline source at `size = upem`.
    fn outline(&mut self, gid: GlyphId) -> BezPath {
        let mut path = BezPath::new();
        if let Ok(id) = u16::try_from(gid.to_u32()) {
            self.outlines.outline(self.font, id, self.upem, self.coords, &mut path);
        }
        path
    }

    fn grow(&mut self, r: Rect) {
        self.bounds = Some(self.bounds.map_or(r, |b| b.union(r)));
    }
}

impl ColorPainter for FragmentPainter<'_, '_> {
    fn push_transform(&mut self, t: Transform) {
        let m = Affine::new([t.xx, t.yx, t.xy, t.yy, t.dx, t.dy].map(f64::from));
        self.xf.push(self.top() * m);
    }

    fn pop_transform(&mut self) {
        if self.xf.len() > 1 {
            self.xf.pop();
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        let path = self.outline(glyph_id);
        let t = self.top();
        self.clip_bounds.push(t.transform_rect_bbox(path.bounding_box()));
        self.p.push_layer(Fill::NonZero, BlendMode::default(), 1.0, t, &path);
    }

    fn push_clip_box(&mut self, b: BoundingBox<f32>) {
        let r = Rect::new(f64::from(b.x_min), f64::from(b.y_min), f64::from(b.x_max), f64::from(b.y_max));
        let t = self.top();
        self.clip_bounds.push(t.transform_rect_bbox(r));
        self.p.push_layer(Fill::NonZero, BlendMode::default(), 1.0, t, &r);
    }

    fn pop_clip(&mut self) {
        self.clip_bounds.pop();
        self.p.pop_layer();
    }

    fn fill(&mut self, brush: ColrBrush<'_>) {
        // Fill the current clip's bounds (fragment space); the clip layer cuts the shape.
        let Some(&area) = self.clip_bounds.last() else { return };
        let b = self.brush(&brush);
        let t = self.top();
        // The rect is in fragment space; the brush lives in the current paint space.
        self.p.fill(Fill::NonZero, Affine::IDENTITY, &b, Some(t), &area);
        self.grow(area);
    }

    fn fill_glyph(&mut self, glyph_id: GlyphId, brush_transform: Option<Transform>, brush: ColrBrush<'_>) {
        let path = self.outline(glyph_id);
        let t = self.top();
        let bt = brush_transform.map(|m| Affine::new([m.xx, m.yx, m.xy, m.yy, m.dx, m.dy].map(f64::from)));
        let b = self.brush(&brush);
        self.p.fill(Fill::NonZero, t, &b, bt, &path);
        self.grow(t.transform_rect_bbox(path.bounding_box()));
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        // A composite layer over the glyph's whole area (the clips inside cut it).
        let area = self.clip_bounds.last().copied().unwrap_or_else(|| {
            let e = f64::from(self.upem) * 4.0;
            Rect::new(-e, -e, e, e)
        });
        self.p.push_layer(Fill::NonZero, blend_of(mode), 1.0, Affine::IDENTITY, &area);
    }

    fn pop_layer(&mut self) {
        self.p.pop_layer();
    }
}

#[cfg(test)]
#[path = "color_glyph_tests.rs"]
pub(crate) mod tests;
