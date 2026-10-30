//! Text environment (03 §3.4): fonts, shaping, layout and their caches, shared by every text
//! render object.
//!
//! - **Ownership.** One `TextEnv` per runner, shared by all its windows; offscreen capture
//!   builds its own per call; tests use [`TextEnv::new`] (bundled + `App::font` faces only,
//!   deterministic on every OS).
//! - **Pipeline.** `FontRegistry` (bundled-first, zero-copy) → shaping per paragraph
//!   (cosmic-text `ShapeLine`) → [`TextLayout`] (line breaking, metrics, ellipsis). Both stages
//!   are cached by content hash and evicted by generation, only under pressure; a font-set
//!   change clears them.
//! - **No rasterization here.** Text produces glyph runs; the renderer owns the atlas (DR-07).

pub(crate) mod cache;
pub(crate) mod decoration;
pub(crate) mod fonts;
pub(crate) mod geometry;
pub(crate) mod layout;
pub(crate) mod motion;
pub(crate) mod paint;
pub(crate) mod shape;
pub(crate) mod style;

// Debug-only tally of shapes (E3): one per cosmic-text `ShapeLine::new`. Tests assert a
// stable string does not re-shape on a repeat layout.
#[cfg(debug_assertions)]
thread_local! {
    static SHAPES: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Debug-only: total shapes since [`reset_shape_count`]. Test hook (E3).
#[cfg(debug_assertions)]
pub fn shape_count() -> u64 {
    SHAPES.with(std::cell::Cell::get)
}

/// Debug-only: reset the shape tally.
#[cfg(debug_assertions)]
pub fn reset_shape_count() {
    SHAPES.with(|c| c.set(0));
}

#[cfg(debug_assertions)]
thread_local! {
    static LAYOUTS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Debug-only: total `TextLayout` constructions since [`reset_layout_count`].
#[cfg(debug_assertions)]
pub fn layout_count() -> u64 {
    LAYOUTS.with(std::cell::Cell::get)
}

/// Debug-only: reset the layout tally.
#[cfg(debug_assertions)]
pub fn reset_layout_count() {
    LAYOUTS.with(|c| c.set(0));
}

/// Count one `TextLayout` construction (no-op in release).
#[inline]
pub(crate) fn count_layout() {
    #[cfg(debug_assertions)]
    LAYOUTS.with(|c| c.set(c.get() + 1));
}

pub use geometry::Affinity;
pub use layout::TextLayout;

/// One text layout request (crate-internal: cosmic types never reach a `pub` signature).
pub(crate) struct TextRequest<'a> {
    pub(crate) text: &'a str,
    pub(crate) style: &'a style::TextStyleSpec,
    pub(crate) spans: &'a [style::SpanSpec],
    pub(crate) params: layout::LayoutParams,
}

/// The cache keys of a [`TextRequest`]: its shape and its layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextKey {
    pub(crate) shape: u64,
    pub(crate) layout: u64,
}

/// Debug census of a [`TextEnv`] (03 §3.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextCensus {
    /// Cached shaped texts.
    pub shapes: usize,
    /// Cached layouts.
    pub layouts: usize,
    /// Font faces in the registry.
    pub faces: usize,
    /// Faces with glyph-op data.
    pub face_data: usize,
    /// Memoized (face, weight) variation coordinates.
    pub coords: usize,
    /// Font generation.
    pub generation: u64,
}

/// Test seam: shape + lay out `text` exactly as `RenderParagraph` will (`width` only when
/// `style.soft_wrap`), through the caches.
#[doc(hidden)]
pub fn layout_for_test(
    env: &mut TextEnv,
    text: &str,
    style: &crate::objects::ParagraphStyle,
    spans: &[crate::objects::TextSpanStyle],
    width: Option<f32>,
) -> std::rc::Rc<TextLayout> {
    let spec = style::TextStyleSpec::from(style);
    let spans: Vec<style::SpanSpec> = spans.iter().map(style::SpanSpec::from).collect();
    let wrap = style.soft_wrap && width.is_some();
    let params = layout::LayoutParams {
        width: if wrap || style.ellipsis { width } else { None },
        wrap,
        align: Some(style::to_cosmic_align(
            style.align,
            crate::text_direction() == pebbles_foundation::TextDirection::Rtl,
        )),
        max_lines: style.max_lines,
        ellipsis: style.ellipsis,
        strut: style.strut.as_ref().map(|s| env.strut_metrics(s)),
    };
    let req = TextRequest { text, style: &spec, spans: &spans, params };
    let key = env.text_key(&req);
    env.layout_text(&req, key)
}

/// Lay out `text` with `spans` exactly as a `Text` / `RichText` widget would
/// (`RenderParagraph`'s style resolution and line breaking; wraps at `max_width` only when
/// `style.soft_wrap`), through `env`'s caches.
pub(crate) fn layout_paragraph(
    env: &mut TextEnv,
    text: &str,
    spans: &[crate::objects::TextSpanStyle],
    style: &crate::objects::ParagraphStyle,
    max_width: Option<f64>,
) -> std::rc::Rc<TextLayout> {
    let width = max_width.filter(|w| w.is_finite()).map(|w| w.max(0.0) as f32);
    layout_for_test(env, text, style, spans, width)
}

impl TextEnv {
    /// Measure `text` laid out as a `Text` widget with `style` would be (Flutter's
    /// `TextPainter.layout` + `size`): the width of the longest line and the total height.
    /// `max_width` wraps when `style.soft_wrap` (pass `None` for one unbounded line).
    pub fn measure_text(
        &mut self,
        text: &str,
        style: &crate::objects::ParagraphStyle,
        max_width: Option<f64>,
    ) -> pebbles_foundation::Size {
        let layout = layout_paragraph(self, text, &[], style, max_width);
        let (w, h) = layout.size();
        pebbles_foundation::Size::new(f64::from(w), f64::from(h))
    }

    /// The cell of a monospace grid in `family` at `font_size` and `line_height` (a
    /// multiple of the size, as in `ParagraphStyle::line_height`): the advance of one
    /// character and the height of one line — exactly what a `Text` row in that font
    /// occupies, so a terminal / code grid can position cells with `Text` or the canvas.
    pub fn monospace_cell(
        &mut self,
        family: &str,
        font_size: f32,
        line_height: f32,
    ) -> pebbles_foundation::Size {
        const N: usize = 64;
        let style = crate::objects::ParagraphStyle {
            font_size,
            line_height,
            font_family: Some(family.to_string()),
            soft_wrap: false,
            ..Default::default()
        };
        // Average over a run so sub-pixel advances don't round per glyph.
        let row = "0".repeat(N);
        let size = self.measure_text(&row, &style, None);
        pebbles_foundation::Size::new(size.width / N as f64, size.height)
    }
}

thread_local! {
    /// The measuring environment behind [`measure_text`] / [`monospace_cell`].
    static MEASURE_ENV: std::cell::RefCell<Option<TextEnv>> = const { std::cell::RefCell::new(None) };
}

fn with_measure_env<R>(f: impl FnOnce(&mut TextEnv) -> R) -> R {
    MEASURE_ENV.with(|cell| {
        let mut cell = cell.borrow_mut();
        // Not `TextEnv::new()`: that republishes the global family list, which must keep
        // reflecting the runner's (system-font) environment.
        let env = cell.get_or_insert_with(|| TextEnv {
            registry: fonts::FontRegistry::new_bundled(&crate::fonts::user_fonts_snapshot(), "en-US".into()),
            scratch: Default::default(),
            caches: Default::default(),
        });
        f(env)
    })
}

/// Measure `text` as a `Text` widget with `style` would lay it out — usable anywhere
/// (build, `layout_builder`, event handlers), no paint context needed. Measures with the
/// bundled fonts (Inter, JetBrains Mono, …) and every `App::font`; a family that exists
/// only as an installed system font falls back here, so use [`TextEnv::measure_text`] /
/// [`Canvas::measure_text`](crate::Canvas::measure_text) for those.
pub fn measure_text(
    text: &str,
    style: &crate::objects::ParagraphStyle,
    max_width: Option<f64>,
) -> pebbles_foundation::Size {
    with_measure_env(|env| env.measure_text(text, style, max_width))
}

/// The monospace cell size (advance × line height) for `family` at `font_size` /
/// `line_height` — see [`TextEnv::monospace_cell`]; same font scope as [`measure_text`].
///
/// ```no_run
/// let cell = pebbles_render::monospace_cell("JetBrains Mono", 13.0, 1.3);
/// let cols = (640.0 / cell.width).floor() as usize;
/// ```
pub fn monospace_cell(family: &str, font_size: f32, line_height: f32) -> pebbles_foundation::Size {
    with_measure_env(|env| env.monospace_cell(family, font_size, line_height))
}

/// Count one shape (no-op in release).
#[inline]
pub(crate) fn count_shape() {
    #[cfg(debug_assertions)]
    SHAPES.with(|c| c.set(c.get() + 1));
}

/// Whether a [`TextEnv`] also uses the host's installed fonts (03 §3.3.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SystemFonts {
    /// Bundled + App::font only. Deterministic. Tests, web.
    None,
    /// Scan synchronously during construction. Capture/offscreen (screenshots match the app).
    Blocking,
    /// Scan on a background thread "pebbles-font-scan"; merge on the next poll. Desktop runner.
    Background,
}

/// Owns the mutable state text needs: the font registry, the line-breaking scratch and the
/// shape / layout caches. One per runner (shared by its windows).
pub struct TextEnv {
    /// The cosmic-text font registry (03 §3.3).
    registry: fonts::FontRegistry,
    /// cosmic-text line-breaking scratch, reused by every layout (03 §3.8).
    scratch: cosmic_text::ShapeBuffer,
    /// Shape + layout caches for the cosmic-text path (03 §3.8).
    caches: cache::TextCaches,
}

/// Entries kept beyond this trigger an eviction sweep at frame end. Sized for
/// editor-scale documents: a megabyte source holds tens of thousands of per-line
/// entries whose owner (the field's line table) rebuilds from them on remount —
/// evicting those between screen hops turns a warm remount into a full reshape.
pub(crate) const SHAPE_CACHE_PRESSURE: usize = 32_768;

/// How many generations (frames) an entry may sit unused before a pressure sweep
/// may drop it. Minutes, not seconds: an editor's working set (per-line layouts
/// of the open document) must survive View/Edit remounts and idle pauses —
/// re-shaping a megabyte because the user read the preview for a while is the
/// exact jank this cache exists to kill. Reclaimed after sustained disuse.
pub(crate) const SHAPE_CACHE_STALE: u32 = 4096;

impl Default for TextEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEnv {
    /// A text environment with the bundled + `App::font` faces only (deterministic on every
    /// OS; tests). Apps use [`with_system_fonts`](Self::with_system_fonts).
    pub fn new() -> Self {
        Self::with_registry(fonts::FontRegistry::new_bundled(
            &crate::fonts::user_fonts_snapshot(),
            "en-US".into(),
        ))
    }

    fn with_registry(registry: fonts::FontRegistry) -> Self {
        // The global families snapshot is refreshed ONLY from the registry (03 §3.3.7).
        crate::fonts::refresh_families_from(registry.families());
        TextEnv { registry, scratch: Default::default(), caches: Default::default() }
    }

    /// Former test-only hermetic constructor; [`new`](Self::new) is hermetic now.
    #[deprecated(note = "`TextEnv::new()` uses bundled + App::font faces only")]
    #[doc(hidden)]
    pub fn new_hermetic() -> Self {
        Self::new()
    }

    /// A text environment that also uses the host's fonts per `mode` (always bundled-only on
    /// the web). Locale from the OS (`sys-locale`), falling back to `en-US`.
    pub fn with_system_fonts(mode: SystemFonts) -> Self {
        let locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".into());
        let mut registry = fonts::FontRegistry::new_bundled(&crate::fonts::user_fonts_snapshot(), locale);
        match mode {
            SystemFonts::None => {}
            SystemFonts::Blocking => registry.scan_blocking(),
            SystemFonts::Background => registry.start_background(),
        }
        Self::with_registry(registry)
    }

    /// Merge a finished background font scan; `true` when the font set changed. Never blocks.
    pub fn poll_fonts(&mut self) -> bool {
        let changed = self.registry.poll();
        if changed {
            self.caches.clear();
        }
        changed
    }

    /// End-of-frame hook (the shell calls it once per window draw): advance the
    /// cache generation and, under pressure, evict entries not used in the last
    /// two generations. Idle windows keep their entries (eviction only under
    /// pressure), so re-presenting a long-idle window never re-shapes the world.
    pub fn finish_frame(&mut self) {
        self.caches.finish_frame();
        // cosmic-text's shaped-word cache (feature `shape-run-cache`): one age per 256 frames,
        // dropping runs unused for 16 ages — the same ~4 096-frame horizon as the layout caches.
        if self.caches.generation.is_multiple_of(256) {
            self.registry.font_system().shape_run_cache.trim(SHAPE_CACHE_STALE as u64 / 256);
        }
    }

    /// Number of cached shaped layouts (census / tests).
    pub fn shape_cache_len(&self) -> usize {
        self.caches.shapes.len()
    }

    /// The cache keys of `req` (shape key includes the font generation).
    pub(crate) fn text_key(&self, req: &TextRequest<'_>) -> TextKey {
        let shape = style::shape_hash(req.text, req.style, req.spans, self.registry.generation());
        TextKey { shape, layout: layout::layout_hash(shape, &req.params) }
    }

    /// The layout of `req`: a layout-cache hit, or a layout of the cached (or fresh) shape.
    pub(crate) fn layout_text(&mut self, req: &TextRequest<'_>, key: TextKey) -> std::rc::Rc<TextLayout> {
        let generation = self.caches.generation;
        if let Some(hit) = self.caches.layouts.get(key.layout, generation) {
            return hit;
        }
        let shaped = match self.caches.shapes.get(key.shape, generation) {
            Some(shaped) => shaped,
            None => {
                let shaped =
                    std::rc::Rc::new(shape::shape_text(&mut self.registry, req.text, req.style, req.spans));
                self.caches.shapes.insert(key.shape, shaped.clone(), generation);
                shaped
            }
        };
        let layout =
            std::rc::Rc::new(TextLayout::new(shaped, req.params, &mut self.scratch, &mut self.registry));
        self.caches.layouts.insert(key.layout, layout.clone(), generation);
        layout
    }

    /// Face data (Blob + normalized `wght` coords) for a glyph op.
    #[cfg(test)]
    pub(crate) fn face(&mut self, id: cosmic_text::fontdb::ID, weight: u16) -> Option<fonts::FaceRef> {
        self.registry.face(id, weight)
    }

    /// (min, max) intrinsic widths of a layout's shaped text (memoized, no reshape).
    pub(crate) fn intrinsic_widths(&mut self, layout: &TextLayout) -> (f32, f32) {
        layout::intrinsic_widths(&layout.shaped, &mut self.scratch, &mut self.registry)
    }

    /// A strut resolved to px: the strut font's ascent / descent at its size (rescaled to
    /// `height` × size when set), plus half the leading above and below.
    pub(crate) fn strut_metrics(&mut self, strut: &crate::objects::StrutStyle) -> layout::StrutMetrics {
        let size = style::sanitize_size(strut.font_size);
        let family = strut.font_family.as_deref().unwrap_or(crate::fonts::DEFAULT_FAMILY);
        let (mut ascent, mut descent) =
            self.registry.vertical_metrics(family, size).unwrap_or((size * 0.8, size * 0.2));
        if let Some(h) = strut.height.filter(|h| h.is_finite() && *h >= 0.0) {
            let total = ascent + descent;
            if total > 0.0 {
                let k = h * size / total;
                ascent *= k;
                descent *= k;
            }
        }
        let half_leading = strut.leading.filter(|l| l.is_finite()).map_or(0.0, |l| l * size / 2.0);
        layout::StrutMetrics {
            above: ascent + half_leading,
            below: descent + half_leading,
            force: strut.force_strut_height,
        }
    }

    /// [`face`](Self::face) with extra variation axes.
    pub(crate) fn face_with(
        &mut self,
        id: cosmic_text::fontdb::ID,
        weight: u16,
        variations: &[crate::objects::FontVariation],
    ) -> Option<fonts::FaceRef> {
        self.registry.face_with(id, weight, variations)
    }

    /// The font generation (bumps on every face addition).
    pub fn font_generation(&self) -> u64 {
        self.registry.generation()
    }

    /// Debug census: cache and font-registry sizes.
    #[cfg(debug_assertions)]
    pub fn census(&self) -> TextCensus {
        TextCensus {
            shapes: self.caches.shapes.len(),
            layouts: self.caches.layouts.len(),
            faces: self.registry.faces_len(),
            face_data: self.registry.face_data_len(),
            coords: self.registry.coords_len(),
            generation: self.registry.generation(),
        }
    }

    /// Number of cached cosmic-text layouts (census / tests).
    pub fn layout_cache_len(&self) -> usize {
        self.caches.layouts.len()
    }
    /// Registers font bytes (one or more faces) and refreshes discovery.
    /// Returns the number of faces registered.
    pub fn register_font(&mut self, bytes: Vec<u8>) -> usize {
        let added = self.registry.register(std::sync::Arc::new(bytes));
        if added > 0 {
            self.caches.clear();
        }
        added
    }
    /// Loads and registers a font file from disk (ttf/otf/ttc).
    pub fn register_font_file(&mut self, path: impl AsRef<std::path::Path>) -> std::io::Result<()> {
        let data = std::fs::read(path)?;
        self.register_font(data);
        Ok(())
    }
    /// Reloads the OS font set (e.g. after the user installs a font) and
    /// refreshes discovery. No-op on the web, which has no system-font discovery —
    /// the bundled families (+ any `App::font`) are the guaranteed set there.
    pub fn register_system_fonts(&mut self) {
        #[cfg(not(target_family = "wasm"))]
        {
            let before = self.registry.generation();
            self.registry.scan_blocking();
            if self.registry.generation() != before {
                self.caches.clear();
            }
        }
    }
}

#[cfg(all(test, debug_assertions))] // the shape/layout counters and census are debug-only
mod tests {
    use super::*;
    use crate::objects::ParagraphStyle;

    fn style() -> ParagraphStyle {
        ParagraphStyle::default()
    }

    #[test]
    fn request_hits_layout_cache() {
        let mut env = TextEnv::new();
        let (shapes, layouts) = (shape_count(), layout_count());
        layout_for_test(&mut env, "hello world", &style(), &[], Some(200.0));
        layout_for_test(&mut env, "hello world", &style(), &[], Some(200.0));
        assert_eq!((shape_count() - shapes, layout_count() - layouts), (1, 1));
    }

    #[test]
    fn width_change_relayouts_without_reshape() {
        let mut env = TextEnv::new();
        let (shapes, layouts) = (shape_count(), layout_count());
        layout_for_test(&mut env, "hello world", &style(), &[], Some(200.0));
        layout_for_test(&mut env, "hello world", &style(), &[], Some(120.0));
        assert_eq!((shape_count() - shapes, layout_count() - layouts), (1, 2));
    }

    #[test]
    fn census_counts() {
        let mut env = TextEnv::new();
        layout_for_test(&mut env, "hello", &style(), &[], None);
        let c = env.census();
        assert_eq!((c.shapes, c.layouts, c.faces), (1, 1, 6));
    }
}
