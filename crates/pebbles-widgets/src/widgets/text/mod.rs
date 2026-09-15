//! Text rendering, rich text, selection, and editing widgets.

mod default_text_style;
mod editable;
#[allow(clippy::module_inception)] // the group's core text widgets (Text, RichText)
mod text;

pub use default_text_style::{DefaultTextStyle, animated_default_text_style, default_text_style};
pub use editable::{EditableText, editable};
pub use text::{
    RichText, SelectionGroup, Text, TextSpan, selection_group, span, text, text_rich, text_signal,
};
