//! [`DecoratedBox`] — paints a [`BoxDecoration`] (color, border, radius, shadows)
//! around its child. Backs [`pebbles_render::RenderDecoratedBox`].

use pebbles_render::{BoxDecoration, RenderDecoratedBox, RenderObject};

use pebbles_core::widget::{AnyWidget, RenderWidget};

/// A box with a rich visual decoration.
#[derive(Clone)]
pub struct DecoratedBox {
    pub decoration: BoxDecoration,
    pub foreground: Option<BoxDecoration>,
    child: Option<AnyWidget>,
}

impl DecoratedBox {
    pub fn new(decoration: BoxDecoration, child: impl pebbles_core::IntoWidget) -> Self {
        DecoratedBox { decoration, foreground: None, child: Some(child.into_widget()) }
    }
    /// A childless decorated box: paints its decoration and fills the
    /// constraints it is given (the render-level fill primitive).
    pub fn childless(decoration: BoxDecoration) -> Self {
        DecoratedBox { decoration, foreground: None, child: None }
    }
    /// A decoration painted OVER the child (Flutter's `foregroundDecoration`).
    pub fn foreground(mut self, decoration: BoxDecoration) -> Self {
        self.foreground = Some(decoration);
        self
    }
}

pebbles_core::render_widget!(DecoratedBox);

impl RenderWidget for DecoratedBox {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut r = RenderDecoratedBox::new(self.decoration.clone());
        r.foreground = self.foreground.clone();
        Box::new(r)
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderDecoratedBox>() {
            r.decoration = self.decoration.clone();
            r.foreground = self.foreground.clone();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
