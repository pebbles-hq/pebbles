//! Glyph rasterization during the transition: cosmic-text's `SwashCache` + `FontSystem`
//! (replaced by direct swash in C09).

use cosmic_text::FontSystem;

use super::super::renderer::Renderer;

/// Resolve a font's `fontdb::ID` (registering its bytes on first sight) so the
/// glyph cache keys reference the host's own font database.
pub(crate) fn font_id(renderer: &mut Renderer, font: &peniko::FontData) -> Option<fontdb::ID> {
    let key = (font.data.id(), font.index);
    if let Some(&id) = renderer.font_ids.get(&key) {
        return Some(id);
    }
    let bytes = font.data.data().to_vec();
    let source = fontdb::Source::Binary(std::sync::Arc::new(bytes));
    let ids = renderer.fonts.get_or_insert_with(FontSystem::new).db_mut().load_font_source(source);
    let id = ids.get(font.index as usize).copied().or_else(|| ids.first().copied())?;
    renderer.font_ids.insert(key, id);
    Some(id)
}
