//! H2 — the custom-paint `canvas` widget: immediate-mode drawing over the scene.

use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn canvas_screen() -> Element {
    // Read theme colors in the COMPONENT (subscribes) and capture them into the
    // painters — signals belong in the component, not the painter (H2 note).
    let colors = theme().colors;

    screen("Canvas")
        .description(
            "An immediate-mode drawing surface. The painter runs each paint in the widget's local \
             coordinates; signals read in the component (not the painter) drive re-renders. \
             Unblocks charts + Gravel.",
        )
        .body(children![
            doc("Shapes")
                .description("fill_rrect / fill_circle / fill_rect / stroke_line in one painter.")
                .body(
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
            doc("Fills its constraints")
                .description("Omit width/height and the canvas fills the space its parent gives it.")
                .body(
                    Container::new().width(340.0).height(72.0).child(canvas(move |c: &mut Canvas<'_>| {
                        let s = c.size();
                        c.fill_rrect(Rect::new(0.0, 0.0, s.width, s.height), 8.0, colors.card);
                        c.fill_rrect(Rect::new(0.0, 0.0, s.width * 0.66, s.height), 8.0, colors.primary);
                    })),
                ),
        ])
}
