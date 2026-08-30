//! [`EditableText`] — a leaf widget that paints editable text, a selection and a
//! caret. Backs [`pebbles_render::RenderTextField`]. It is display-only: the owning
//! component holds the string + selection and passes them in each render. The themed
//! [`TextField`](crate::components::TextField) wraps this with a border, padding,
//! focus ring and the keyboard/mouse editing logic.

use pebbles_render::{RenderObject, RenderTextField, TextFieldStyle};

use pebbles_core::widget::RenderWidget;

/// A leaf editable-text display. Configure fluently; the component drives it.
#[derive(Clone)]
pub struct EditableText {
    pub text: String,
    pub placeholder: String,
    pub anchor: usize,
    pub focus: usize,
    pub preedit: String,
    pub focused: bool,
    pub obscure: Option<char>,
    pub multiline: bool,
    pub field_id: u64,
    pub style: TextFieldStyle,
}

/// Create an [`EditableText`] showing `text`.
pub fn editable(text: impl Into<String>) -> EditableText {
    EditableText {
        text: text.into(),
        placeholder: String::new(),
        anchor: 0,
        focus: 0,
        preedit: String::new(),
        focused: false,
        obscure: None,
        multiline: false,
        field_id: 0,
        style: TextFieldStyle::default(),
    }
}

impl EditableText {
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }
    /// Set the selection (anchor, focus) as byte offsets. A collapsed selection
    /// (anchor == focus) is a plain caret.
    pub fn selection(mut self, anchor: usize, focus: usize) -> Self {
        self.anchor = anchor;
        self.focus = focus;
        self
    }
    /// The IME preedit (composition) text, shown underlined at the caret. Empty means
    /// not composing.
    pub fn preedit(mut self, preedit: impl Into<String>) -> Self {
        self.preedit = preedit.into();
        self
    }
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }
    pub fn obscure(mut self, ch: Option<char>) -> Self {
        self.obscure = ch;
        self
    }
    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }
    pub fn field_id(mut self, id: u64) -> Self {
        self.field_id = id;
        self
    }
    pub fn style(mut self, style: TextFieldStyle) -> Self {
        self.style = style;
        self
    }

    fn make(&self) -> RenderTextField {
        let mut r = RenderTextField::new(self.text.clone(), self.style.clone());
        r.placeholder = self.placeholder.clone();
        r.anchor = self.anchor;
        r.focus = self.focus;
        r.preedit = self.preedit.clone();
        r.focused = self.focused;
        r.obscure = self.obscure;
        r.multiline = self.multiline;
        r.field_id = self.field_id;
        r
    }
}

pebbles_core::render_widget!(EditableText);

impl RenderWidget for EditableText {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(self.make())
    }

    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderTextField>() {
            *r = self.make();
        }
    }
}
