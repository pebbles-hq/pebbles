//! [`InteractiveViewer`] — pan and zoom a child (Flutter's `InteractiveViewer`).
//!
//! Drag to pan; double-tap to toggle between fit (1×) and a zoomed-in scale. The
//! child is transformed (translate × scale) and clipped to the viewer's bounds, so
//! panned/zoomed content never spills outside. Scale is clamped to `[min, max]`.
//!
//! ```ignore
//! interactive_viewer(big_diagram()).min_scale(0.5).max_scale(5.0)
//! ```

use pebbles_render::{Affine, BorderRadius};

use crate::widgets::{GestureDetector, clip_rrect, transform};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Element, action, action_event, animate_to, component_props, create_signal};

/// A pan-and-zoom viewport over a child. Built by [`interactive_viewer`].
#[derive(Clone)]
pub struct InteractiveViewer {
    child: AnyWidget,
    min_scale: f64,
    max_scale: f64,
    zoomed_scale: f64,
    pannable: bool,
}

/// A viewer that lets the user drag to pan and double-tap to zoom `child`.
pub fn interactive_viewer(child: impl IntoWidget) -> InteractiveViewer {
    InteractiveViewer {
        child: child.into_widget(),
        min_scale: 0.8,
        max_scale: 4.0,
        zoomed_scale: 2.5,
        pannable: true,
    }
}

impl InteractiveViewer {
    /// Smallest allowed scale (default `0.8`).
    pub fn min_scale(mut self, s: f64) -> Self {
        self.min_scale = s.max(0.05);
        self
    }
    /// Largest allowed scale (default `4.0`).
    pub fn max_scale(mut self, s: f64) -> Self {
        self.max_scale = s;
        self
    }
    /// Scale that a double-tap zooms to (default `2.5`).
    pub fn zoomed_scale(mut self, s: f64) -> Self {
        self.zoomed_scale = s;
        self
    }
    /// Whether dragging pans the child (default `true`).
    pub fn pannable(mut self, pannable: bool) -> Self {
        self.pannable = pannable;
        self
    }
}

impl IntoWidget for InteractiveViewer {
    fn into_widget(self) -> AnyWidget {
        component_props(render_interactive_viewer, self).into_widget()
    }
}

fn render_interactive_viewer(v: &InteractiveViewer) -> Element {
    let scale = create_signal(1.0_f64);
    let tx = create_signal(0.0_f64);
    let ty = create_signal(0.0_f64);

    let s = scale.get().clamp(v.min_scale, v.max_scale);
    let matrix = Affine::translate((tx.get(), ty.get())) * Affine::scale(s);
    let content = transform(matrix, v.child.clone());

    let zoomed = v.zoomed_scale.clamp(v.min_scale, v.max_scale);
    let pannable = v.pannable;

    let gd = GestureDetector::new(content)
        .on_pan_update(action_event(move |e: pebbles_render::PointerEvent| {
            if pannable {
                tx.update(|x| *x += e.delta.x);
                ty.update(|y| *y += e.delta.y);
            }
        }))
        .on_double_tap(action(move || {
            // Toggle between fit (1×) and zoomed; reset translation on the way back.
            if scale.peek() > 1.01 {
                animate_to(scale, 1.0, 0.2);
                animate_to(tx, 0.0, 0.2);
                animate_to(ty, 0.0, 0.2);
            } else {
                animate_to(scale, zoomed, 0.2);
            }
        }));

    // Clip so scaled/panned content never spills past the viewport.
    clip_rrect(BorderRadius::ZERO, gd).into_widget()
}
