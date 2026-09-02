//! [`Resizable`] — shadcn's **Resizable** panel group: two or more panels laid out
//! along an axis with draggable handles between them. Dragging a handle grows one
//! panel and shrinks its neighbor (respecting per-panel minimums). Horizontal or
//! vertical; the group owns the live sizes, so it just works once mounted.

use pebbles_foundation::{Alignment, Axis, CrossAxisAlignment, MainAxisSize};
use pebbles_render::{BorderRadius, BoxDecoration, Cursor, StackFit};

use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Positioned, center, column, row, stack};
use pebbles_core::context::{action, action_event};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{children, component_props, create_signal};

const HANDLE: f64 = 8.0;

/// A group of resizable panels. Build with [`resizable`].
pub struct Resizable {
    panels: Vec<AnyWidget>,
    axis: Axis,
    length: f64,
    sizes: Option<Vec<f64>>,
    min: f64,
    handle: f64,
}

/// Create a [`Resizable`] group over `panels` (side-by-side by default).
pub fn resizable(panels: Vec<AnyWidget>) -> Resizable {
    Resizable { panels, axis: Axis::Horizontal, length: 600.0, sizes: None, min: 60.0, handle: HANDLE }
}

impl Resizable {
    /// Stack the panels vertically instead of side-by-side.
    pub fn orientation(mut self, axis: Axis) -> Self {
        self.axis = axis;
        self
    }
    /// Total extent along the axis (width if horizontal, height if vertical). Panel
    /// sizes are pixel amounts within this; it also seeds an equal split.
    pub fn length(mut self, length: f64) -> Self {
        self.length = length;
        self
    }
    /// Initial pixel size of each panel (must match the panel count). Overrides the
    /// equal split; their sum should equal [`length`](Resizable::length).
    pub fn sizes(mut self, sizes: Vec<f64>) -> Self {
        self.sizes = Some(sizes);
        self
    }
    /// Minimum pixel size a panel can be dragged to (default `60`).
    pub fn min(mut self, min: f64) -> Self {
        self.min = min;
        self
    }
    /// Handle thickness (default `8`).
    pub fn handle(mut self, thickness: f64) -> Self {
        self.handle = thickness;
        self
    }
}

struct Props {
    panels: Vec<AnyWidget>,
    axis: Axis,
    sizes: Vec<f64>,
    min: f64,
    handle: f64,
}

impl IntoWidget for Resizable {
    fn into_widget(self) -> AnyWidget {
        let n = self.panels.len().max(1);
        let sizes = self.sizes.unwrap_or_else(|| vec![self.length / n as f64; n]);
        component_props(
            render_resizable,
            Props { panels: self.panels, axis: self.axis, sizes, min: self.min, handle: self.handle },
        )
        .into_widget()
    }
}

/// Drag state: `(handle_index, start_main, start_size_a, start_size_b)`.
type Drag = Option<(usize, f64, f64, f64)>;

fn render_resizable(p: &Props) -> AnyWidget {
    let c = theme().colors;
    let n = p.panels.len();
    let horiz = p.axis == Axis::Horizontal;
    let sizes = create_signal(p.sizes.clone());
    let drag = create_signal::<Drag>(None);
    let cur = sizes.get();
    let active = drag.get().map(|d| d.0);

    let mut kids: Vec<AnyWidget> = Vec::with_capacity(n * 2);
    for i in 0..n {
        // Panel: fixed along the axis, filled across it, clipped. The content is
        // stretched to fill the panel box (a bare child would shrink-wrap).
        let filled = stack(children![Positioned::fill(p.panels[i].clone())]).fit(StackFit::Expand);
        let mut panel = Container::new().clip().child(filled);
        panel = if horiz {
            panel.width(*cur.get(i).unwrap_or(&0.0))
        } else {
            panel.height(*cur.get(i).unwrap_or(&0.0))
        };
        kids.push(panel.into_widget());

        // Handle between panel i and i+1.
        if i + 1 < n {
            let min = p.min;
            let start = action_event(move |e| {
                let s = sizes.peek();
                let m = if horiz { e.global.x } else { e.global.y };
                drag.set(Some((i, m, s[i], s[i + 1])));
            });
            let update = action_event(move |e| {
                if let Some((hi, start_m, a0, b0)) = drag.peek() {
                    let m = if horiz { e.global.x } else { e.global.y };
                    let total = a0 + b0;
                    let a = (a0 + (m - start_m)).clamp(min, (total - min).max(min));
                    sizes.update(|v| {
                        v[hi] = a;
                        v[hi + 1] = total - a;
                    });
                }
            });
            let end = action(move || drag.set(None));

            let is_active = active == Some(i);
            let grip_color = if is_active { c.primary } else { c.border };
            // A centered grip pill inside a (mostly transparent) hit strip.
            let grip = if horiz {
                Container::new()
                    .decoration(BoxDecoration::new().color(grip_color).radius(BorderRadius::all(999.0)))
                    .width(2.0)
                    .height(24.0)
            } else {
                Container::new()
                    .decoration(BoxDecoration::new().color(grip_color).radius(BorderRadius::all(999.0)))
                    .width(24.0)
                    .height(2.0)
            };
            let mut strip = Container::new().color(c.border).alignment(Alignment::CENTER).child(center(grip));
            strip = if horiz { strip.width(1.0) } else { strip.height(1.0) };
            let hit = if horiz {
                Container::new().width(p.handle).alignment(Alignment::CENTER).child(center(strip))
            } else {
                Container::new().height(p.handle).alignment(Alignment::CENTER).child(center(strip))
            };

            kids.push(
                GestureDetector::new(hit)
                    .cursor(if horiz { Cursor::ColResize } else { Cursor::RowResize })
                    .on_pan_start(start)
                    .on_pan_update(update)
                    .on_pan_end(end)
                    .into_widget(),
            );
        }
    }

    if horiz {
        row(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min).into_widget()
    } else {
        column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min).into_widget()
    }
}
