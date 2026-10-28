//! H2 — the custom-paint `canvas` widget: immediate-mode drawing over the scene.

use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn canvas_screen() -> Element {
    // Read theme colors in the COMPONENT (subscribes) and capture them into the
    // painters — signals belong in the component, not the painter (H2 note).
    let colors = theme().colors;

    screen("Canvas").body(children![
        doc("Shapes").body(
            canvas(move |c: &mut Canvas<'_>| {
                let s = c.size();
                // Card background.
                c.fill_rrect(Rect::new(0.0, 0.0, s.width, s.height), 12.0, colors.card);
                // A row of dots.
                for i in 0..6 {
                    let x = 28.0 + i as f64 * 48.0;
                    c.fill_circle(Offset::new(x, 40.0), 14.0, colors.primary);
                }
                // A rounded bar + a plain bar.
                c.fill_rrect(Rect::new(24.0, 78.0, s.width - 24.0, 100.0), 8.0, colors.secondary);
                c.fill_rect(Rect::new(24.0, 116.0, s.width * 0.5, 132.0), colors.primary);
                // A diagonal hairline.
                c.stroke_line(
                    Offset::new(24.0, 156.0),
                    Offset::new(s.width - 24.0, 168.0),
                    2.0,
                    colors.border,
                );
            })
            .width(340.0)
            .height(190.0),
        ),
        doc("Fills its constraints").body(container().width(340.0).height(72.0).child(canvas(
            move |c: &mut Canvas<'_>| {
                let s = c.size();
                c.fill_rrect(Rect::new(0.0, 0.0, s.width, s.height), 8.0, colors.card);
                c.fill_rrect(Rect::new(0.0, 0.0, s.width * 0.66, s.height), 8.0, colors.primary);
            }
        )),),
    ])
}
