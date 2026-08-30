//! [`Icon`] — a themed vector icon widget over [`pebbles_render::RenderIcon`].

use pebbles_foundation::Color;
use pebbles_render::{IconData, RenderIcon, RenderObject};

use crate::theme::theme;
use pebbles_core::widget::RenderWidget;

/// A vector icon.
#[derive(Clone)]
pub struct Icon {
    data: IconData,
    size: f64,
    color: Color,
}

/// Create an [`Icon`] (default 20px, current theme foreground). Accepts any
/// icon source — a named [`IconKind`](pebbles_render::IconKind), a bundled
/// [`lucide`](pebbles_render::lucide) glyph, or your own [`IconData`].
pub fn icon(icon: impl Into<IconData>) -> Icon {
    Icon { data: icon.into(), size: 20.0, color: theme().colors.foreground }
}

impl Icon {
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

pebbles_core::render_widget!(Icon);

impl RenderWidget for Icon {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderIcon::new(self.data, self.size, self.color))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderIcon>() {
            r.data = self.data;
            r.size = self.size;
            r.color = self.color;
        }
    }
}
