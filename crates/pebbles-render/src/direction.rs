//! The ambient [`TextDirection`] read during layout (D2). A process-/thread-local
//! global (LTR by default) so `RenderFlex` (Row order) and `RenderParagraph` (bidi
//! base direction) can consult it without threading it through every constraint.
//!
//! The widgets layer owns the *reactive* mirror (`pebbles_widgets::text_direction`)
//! and writes here through `set_text_direction`; layout reads the plain global.

use std::cell::Cell;

use pebbles_foundation::TextDirection;

thread_local! {
    static DIR: Cell<TextDirection> = const { Cell::new(TextDirection::Ltr) };
}

/// The current ambient text direction (LTR until set).
pub fn text_direction() -> TextDirection {
    DIR.with(Cell::get)
}

/// Set the ambient text direction. Called by the widgets layer's reactive setter.
pub fn set_text_direction(dir: TextDirection) {
    DIR.with(|d| d.set(dir));
}
