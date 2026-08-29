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
        TextEnv { fonts: parley::FontContext::new(), layout: parley::LayoutContext::new() }
    }
}

impl TextEnv {
    pub fn new() -> Self {
        Self::default()
    }
}
