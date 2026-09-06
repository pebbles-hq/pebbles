//! Bundled font families and family discovery.
//!
//! Every [`TextEnv`](crate::TextEnv) registers a small set of OFL-licensed
//! families embedded in the binary (see `assets/fonts/`) **plus** the fonts
//! installed on the host, discovered through parley/fontique. Query them with
//! [`available_families`]/[`has_family`], and apply one to text with
//! `Style::font_family(...)` / `ParagraphStyle::font_family(...)`.

use std::sync::{OnceLock, RwLock};

use parley::fontique::{Blob, FontInfoOverride};

/// The families embedded in the binary (SIL OFL 1.1 — licenses in
/// `assets/fonts/*-OFL.txt`). Registered in this order, so they take
/// precedence over same-named system fonts.
pub const BUILTIN_FAMILIES: &[&str] = &["Inter", "JetBrains Mono", "Space Grotesk", "Lora"];

/// The default UI font when text specifies no family. It MUST be a bundled family
/// (not a generic like "sans-serif") so text renders on every platform — on web
/// and mobile there is no system-font fallback, so an unresolved generic family
/// produces NO glyphs (the "icons render but text is missing" web bug). This is
/// the same choice Flutter makes by bundling a default font.
pub const DEFAULT_FAMILY: &str = "Inter";

/// Built-in font files: `(family name, bytes)`.
pub fn builtin_fonts() -> &'static [(&'static str, &'static [u8])] {
    &[
        ("Inter", include_bytes!("../assets/fonts/InterVariable.ttf")),
        ("Inter", include_bytes!("../assets/fonts/InterVariable-Italic.ttf")),
        ("JetBrains Mono", include_bytes!("../assets/fonts/JetBrainsMono[wght].ttf")),
        ("JetBrains Mono", include_bytes!("../assets/fonts/JetBrainsMono-Italic[wght].ttf")),
        ("Space Grotesk", include_bytes!("../assets/fonts/SpaceGrotesk[wght].ttf")),
        ("Lora", include_bytes!("../assets/fonts/Lora[wght].ttf")),
    ]
}

/// Registers every built-in face into the given font context, overriding the
/// metadata family name so lookups by [`BUILTIN_FAMILIES`] always resolve.
pub(crate) fn apply_builtins(fonts: &mut parley::FontContext) {
    for (family, bytes) in builtin_fonts() {
        fonts.collection.register_fonts(
            Blob::new(std::sync::Arc::new(bytes.to_vec())),
            Some(FontInfoOverride { family_name: Some(family), ..Default::default() }),
        );
    }
}

/// User fonts registered via `App::font` (F4): raw `'static` bytes, applied by every
/// [`TextEnv`](crate::TextEnv) so all windows (each own a `TextEnv`) see the family.
static USER_FONTS: OnceLock<RwLock<Vec<&'static [u8]>>> = OnceLock::new();

fn user_fonts() -> &'static RwLock<Vec<&'static [u8]>> {
    USER_FONTS.get_or_init(|| RwLock::new(Vec::new()))
}

/// Register user-supplied font bytes globally (F4). The bytes are `&'static` so they
/// outlive every window's collection. Call before `App::run` (via `App::font`); the
/// font's own metadata family name is what `style().font_family("…")` then matches.
/// Runtime (post-startup) loading is v2 (§J).
pub fn register_user_font(bytes: &'static [u8]) {
    user_fonts().write().unwrap().push(bytes);
}

/// Apply every registered user font into a fresh font context.
pub(crate) fn apply_user_fonts(fonts: &mut parley::FontContext) {
    for bytes in user_fonts().read().unwrap().iter() {
        fonts.collection.register_fonts(Blob::new(std::sync::Arc::new(bytes.to_vec())), None);
    }
}

/// Last known family snapshot (built-ins first, then system families sorted).
static FAMILIES: OnceLock<RwLock<Vec<String>>> = OnceLock::new();

fn registry() -> &'static RwLock<Vec<String>> {
    FAMILIES.get_or_init(|| RwLock::new(Vec::new()))
}

/// Refreshes the global family snapshot from a font context. Called whenever
/// fonts are registered into a [`TextEnv`](crate::TextEnv).
pub(crate) fn refresh_families(fonts: &mut parley::FontContext) {
    let mut others: Vec<String> = Vec::new();
    for name in fonts.collection.family_names() {
        if BUILTIN_FAMILIES.contains(&name) {
            continue;
        }
        others.push(name.to_string());
    }
    others.sort_by_key(|a| a.to_lowercase());
    let mut all: Vec<String> = BUILTIN_FAMILIES
        .iter()
        .filter(|b| fonts.collection.family_by_name(b).is_some())
        .map(|s| s.to_string())
        .collect();
    all.append(&mut others);
    if let Ok(mut w) = registry().write() {
        *w = all;
    }
}

/// All family names known to the most recently constructed
/// [`TextEnv`](crate::TextEnv): built-ins first (declaration order), then
/// system families (case-insensitive sort). Empty until a `TextEnv` exists.
pub fn available_families() -> Vec<String> {
    registry().read().map(|r| r.clone()).unwrap_or_default()
}

/// The bundled families actually registered (all of [`BUILTIN_FAMILIES`] once
/// a `TextEnv` has been constructed).
pub fn builtin_families() -> Vec<String> {
    registry()
        .read()
        .map(|r| r.iter().filter(|f| BUILTIN_FAMILIES.contains(&f.as_str())).cloned().collect())
        .unwrap_or_default()
}

/// Whether `name` is a known family (built-in or system), case-insensitive.
pub fn has_family(name: &str) -> bool {
    registry().read().map(|r| r.iter().any(|f| f.eq_ignore_ascii_case(name))).unwrap_or(false)
}

/// Whether `name` is one of the bundled families, case-insensitive.
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_FAMILIES.iter().any(|f| f.eq_ignore_ascii_case(name))
}

/// Convenience re-export so [`TextEnv`](crate::TextEnv) callers don't need a
/// direct parley dependency to register fonts.
pub use parley::fontique::Blob as FontBlob;
