//! Responsive layout helpers.

use pebbles::prelude::*;

/// A **responsive grid**: lay `cards` out in `desktop` / `tablet` / `mobile` columns
/// depending on the window's [`Breakpoint`], each card stretching to fill its column
/// and wrapping to new rows. Reads `breakpoint()` (reactive on the window size), so it
/// re-flows the moment the window crosses a breakpoint — no polling, no frame lag.
pub fn responsive_grid(
    spacing: f64,
    mobile: usize,
    tablet: usize,
    desktop: usize,
    cards: Vec<AnyWidget>,
) -> impl IntoWidget {
    let n = cards.len().max(1);
    let cols = breakpoint().select(mobile, tablet, desktop).clamp(1, n);

    let mut rows: Vec<AnyWidget> = Vec::new();
    let mut it = cards.into_iter();
    let mut remaining = n;
    let mut first = true;
    while remaining > 0 {
        if !first {
            rows.push(gap_h(spacing).into_widget());
        }
        first = false;
        let take = cols.min(remaining);
        let mut cells: Vec<AnyWidget> = Vec::new();
        for col in 0..cols {
            if col > 0 {
                cells.push(gap_w(spacing).into_widget());
            }
            if col < take {
                // Each card fills an equal share of the row width.
                cells.push(Expanded::new(it.next().unwrap()).into_widget());
            } else {
                // Empty slots on the last row keep the card widths consistent.
                cells.push(Expanded::new(gap_h(0.0)).into_widget());
            }
        }
        remaining -= take;
        rows.push(row(cells).cross_axis_alignment(CrossAxisAlignment::Stretch).into_widget());
    }
    column(rows).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min)
}
