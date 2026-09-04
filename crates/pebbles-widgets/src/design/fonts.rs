//! Font discovery: the bundled families plus everything installed on the
//! host. Rendering is handled by the render-layer [`TextEnv`](pebbles_render::TextEnv);
//! apply a family to text with `Style::font_family(...)` or
//! `ParagraphStyle::font_family(...)`.

/// Every known family (built-ins first, then system families sorted).
/// Mirrors the most recently constructed render-layer `TextEnv`.
pub fn families() -> Vec<String> {
    pebbles_render::fonts::available_families()
}

/// The families bundled into the binary (SIL OFL): Inter, JetBrains Mono,
/// Space Grotesk, Lora.
pub fn builtins() -> &'static [&'static str] {
    pebbles_render::fonts::BUILTIN_FAMILIES
}

/// Whether `family` is one of the bundled families (case-insensitive).
pub fn is_builtin(family: &str) -> bool {
    pebbles_render::fonts::is_builtin(family)
}

/// Whether `family` is known — bundled or installed on the host
/// (case-insensitive).
pub fn has(family: &str) -> bool {
    pebbles_render::fonts::has_family(family)
}
