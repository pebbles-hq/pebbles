//! [`View`] — the root widget the shell wraps around user content. Backs
//! [`pebbles_render::RenderView`]: fills the window with a background color and
//! sizes its child to the window.

use pebbles_foundation::Color;
use pebbles_render::{RenderObject, RenderView};

use pebbles_core::widget::{AnyWidget, RenderWidget};

/// The framework-provided root. You rarely construct this directly — `App::run`
/// wraps your widget in one.
#[derive(Clone)]
pub struct View {
    pub background: Color,
    child: Option<AnyWidget>,
}

impl View {
    pub fn new(background: Color, child: impl pebbles_core::IntoWidget) -> Self {
        View { background, child: Some(child.into_widget()) }
    }
}

pebbles_core::render_widget!(View);

impl RenderWidget for View {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderView::new(self.background))
    }

    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(view) = object.downcast_mut::<RenderView>() {
            view.background = self.background;
        }
    }

    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
