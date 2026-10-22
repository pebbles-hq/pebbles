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
//!
//! The Parley `fonts`/`layout` fields remain for not-yet-ported consumers until T40.

pub(crate) mod cache;
pub(crate) mod fonts;
pub(crate) mod geometry;
pub(crate) mod layout;
pub(crate) mod motion;
pub(crate) mod paint;
pub(crate) mod shape;
pub(crate) mod style;

// Debug-only tally of shapes (E3): one per Parley layout that shapes, and one per cosmic-text
// `ShapeLine::new`. Tests assert a stable string doesn't re-shape on a repeat layout.
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
        width: if wrap { width } else { None },
        wrap,
        align: Some(style::to_cosmic_align(
            style.align,
            crate::text_direction() == pebbles_foundation::TextDirection::Rtl,
        )),
        max_lines: style.max_lines,
        ellipsis: style.ellipsis,
    };
    let req = TextRequest { text, style: &spec, spans: &spans, params };
    let key = env.text_key(&req);
    env.layout_text(&req, key)
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

/// Owns the mutable state parley needs to shape and lay out text: the font
/// database/cache and the layout scratch context. One per window.
pub struct TextEnv {
    /// Font collection + cache. Discovers system fonts on construction.
    pub fonts: parley::FontContext,
    /// Reusable layout builder scratch space, parameterized by the brush type
    /// used for glyph styling (we use [`peniko::Brush`]).
    pub layout: parley::LayoutContext<peniko::Brush>,
    /// Shaped-layout cache shared by every paragraph in the window: content-keyed
    /// (`text + style + spans + wrap width`), generation-evicted. A paragraph
    /// REBUILT by the widget layer (a virtual-list window sliding, a corrective
    /// pass, a toggle elsewhere) hits this instead of re-shaping — shaping cost
    /// tracks NEW content, not rebuild traffic.
    cache: ShapeCache,
    /// The cosmic-text font registry (03 §3.3); shaping moves onto it in Track T, while the
    /// Parley fields above stay for not-yet-ported consumers until T40.
    registry: fonts::FontRegistry,
    /// cosmic-text line-breaking scratch, reused by every layout (03 §3.8).
    scratch: cosmic_text::ShapeBuffer,
    /// Shape + layout caches for the cosmic-text path (03 §3.8).
    caches: cache::TextCaches,
}

#[derive(Default)]
struct ShapeCache {
    map: std::collections::HashMap<u64, CacheEntry>,
    generation: u32,
}

struct CacheEntry {
    layout: std::rc::Rc<parley::Layout<peniko::Brush>>,
    size: (f64, f64),
    last_used: u32,
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
    /// Creates a text environment with the bundled families registered and
    /// the host's system fonts discovered.
    pub fn new() -> Self {
        Self::with_registry(fonts::FontRegistry::new_bundled(
            &crate::fonts::user_fonts_snapshot(),
            "en-US".into(),
        ))
    }

    fn with_registry(registry: fonts::FontRegistry) -> Self {
        let mut fonts = parley::FontContext::new();
        crate::fonts::apply_builtins(&mut fonts);
        crate::fonts::apply_user_fonts(&mut fonts); // F4: user fonts from App::font
        // The global families snapshot is refreshed ONLY from the registry (03 §3.3.7).
        crate::fonts::refresh_families_from(registry.families());
        TextEnv {
            fonts,
            layout: parley::LayoutContext::new(),
            cache: ShapeCache::default(),
            registry,
            scratch: Default::default(),
            caches: Default::default(),
        }
    }

    /// Test-only: a TextEnv with ONLY the bundled + App::font faces (no system fonts).
    #[doc(hidden)]
    pub fn new_hermetic() -> Self {
        let mut fonts = parley::FontContext {
            collection: parley::fontique::Collection::new(parley::fontique::CollectionOptions {
                shared: false,
                system_fonts: false,
            }),
            source_cache: Default::default(),
        };
        crate::fonts::apply_builtins(&mut fonts);
        crate::fonts::apply_user_fonts(&mut fonts);
        let registry = fonts::FontRegistry::new_bundled(&crate::fonts::user_fonts_snapshot(), "en-US".into());
        TextEnv {
            fonts,
            layout: parley::LayoutContext::new(),
            cache: ShapeCache::default(),
            registry,
            scratch: Default::default(),
            caches: Default::default(),
        }
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
        self.cache.generation = self.cache.generation.wrapping_add(1);
        // Sweep only occasionally: a huge document legitimately holds thousands
        // of per-line entries whose owners (the field's line table) keep them
        // useful between rebuilds — scanning them every frame is wasted work.
        if self.cache.map.len() > SHAPE_CACHE_PRESSURE && self.cache.generation.is_multiple_of(256) {
            let current = self.cache.generation;
            self.cache.map.retain(|_, e| current.wrapping_sub(e.last_used) <= SHAPE_CACHE_STALE);
        }
    }

    /// Cached shaped layout for `key`, refreshing its generation on hit.
    pub(crate) fn cached_layout(
        &mut self,
        key: u64,
    ) -> Option<(std::rc::Rc<parley::Layout<peniko::Brush>>, f64, f64)> {
        let generation = self.cache.generation;
        self.cache.map.get_mut(&key).map(|e| {
            e.last_used = generation;
            (e.layout.clone(), e.size.0, e.size.1)
        })
    }

    /// Store a freshly shaped layout under `key`.
    pub(crate) fn store_layout(
        &mut self,
        key: u64,
        layout: std::rc::Rc<parley::Layout<peniko::Brush>>,
        w: f64,
        h: f64,
    ) {
        let last_used = self.cache.generation;
        self.cache.map.insert(key, CacheEntry { layout, size: (w, h), last_used });
    }

    /// Number of cached shaped layouts (census / tests).
    pub fn shape_cache_len(&self) -> usize {
        self.cache.map.len()
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
    pub(crate) fn face(&mut self, id: cosmic_text::fontdb::ID, weight: u16) -> Option<fonts::FaceRef> {
        self.registry.face(id, weight)
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
        let data: fonts::FontBytes = std::sync::Arc::new(bytes);
        // Parley keeps a registration (sharing the same bytes) until its consumers are ported (T40).
        self.fonts.collection.register_fonts(crate::fonts::FontBlob::new(data.clone()), None);
        let added = self.registry.register(data);
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
            self.fonts.collection.load_system_fonts(); // Parley, until T40
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
