//! Text environment: the parley font/layout contexts shared across all text
//! render objects for a window. Held by the shell and threaded into layout.

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
const SHAPE_CACHE_PRESSURE: usize = 32_768;

/// How many generations (frames) an entry may sit unused before a pressure sweep
/// may drop it. Minutes, not seconds: an editor's working set (per-line layouts
/// of the open document) must survive View/Edit remounts and idle pauses —
/// re-shaping a megabyte because the user read the preview for a while is the
/// exact jank this cache exists to kill. Reclaimed after sustained disuse.
const SHAPE_CACHE_STALE: u32 = 4096;

impl Default for TextEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl TextEnv {
    /// Creates a text environment with the bundled families registered and
    /// the host's system fonts discovered.
    pub fn new() -> Self {
        let mut fonts = parley::FontContext::new();
        crate::fonts::apply_builtins(&mut fonts);
        crate::fonts::apply_user_fonts(&mut fonts); // F4: user fonts from App::font
        crate::fonts::refresh_families(&mut fonts);
        TextEnv { fonts, layout: parley::LayoutContext::new(), cache: ShapeCache::default() }
    }

    /// End-of-frame hook (the shell calls it once per window draw): advance the
    /// cache generation and, under pressure, evict entries not used in the last
    /// two generations. Idle windows keep their entries (eviction only under
    /// pressure), so re-presenting a long-idle window never re-shapes the world.
    pub fn finish_frame(&mut self) {
        self.cache.generation = self.cache.generation.wrapping_add(1);
        // Sweep only occasionally: a huge document legitimately holds thousands
        // of per-line entries whose owners (the field's line table) keep them
        // useful between rebuilds — scanning them every frame is wasted work.
        if self.cache.map.len() > SHAPE_CACHE_PRESSURE && self.cache.generation % 256 == 0 {
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
    /// Registers font bytes (one or more faces) and refreshes discovery.
    /// Returns the number of faces registered.
    pub fn register_font(&mut self, bytes: Vec<u8>) -> usize {
        let n = self
            .fonts
            .collection
            .register_fonts(crate::fonts::FontBlob::new(std::sync::Arc::new(bytes)), None)
            .len();
        crate::fonts::refresh_families(&mut self.fonts);
        n
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
            self.fonts.collection.load_system_fonts();
            crate::fonts::refresh_families(&mut self.fonts);
        }
    }
}
