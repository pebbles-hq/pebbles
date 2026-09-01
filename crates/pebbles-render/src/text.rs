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
}

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
        crate::fonts::refresh_families(&mut fonts);
        TextEnv { fonts, layout: parley::LayoutContext::new() }
    }
    /// Registers font bytes (one or more faces) and refreshes discovery.
    /// Returns the number of faces registered.
    pub fn register_font(&mut self, bytes: Vec<u8>) -> usize {
        let n = self.fonts.collection.register_fonts(crate::fonts::FontBlob::new(std::sync::Arc::new(bytes)), None).len();
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
    /// refreshes discovery.
    pub fn register_system_fonts(&mut self) {
        self.fonts.collection.load_system_fonts();
        crate::fonts::refresh_families(&mut self.fonts);
    }
}
