//! Client-side window decorations: the building blocks for an app-drawn title bar
//! (VS Code's `window.titleBarStyle: "custom"`, Zed, Chrome). Turn the OS frame off with
//! `App::decorations(false)` (or [`set_window_decorations`](super::window::set_window_decorations)),
//! then:
//!
//! * wrap the title bar's empty area in [`window_drag_area`] — dragging moves the window
//!   (the window manager takes over, so snapping/tiling keep working), double-click
//!   maximises / restores, right-click opens the window manager's window menu;
//! * draw the minimise / maximise-restore / close buttons yourself and call
//!   [`minimize_window`](super::window::minimize_window),
//!   [`toggle_window_maximized`](super::window::toggle_window_maximized) and your own
//!   quit flow; pick the maximise or restore glyph from
//!   [`window_state`](super::window::window_state);
//! * wrap the whole window in [`window_resize_frame`] for the edge / corner resize
//!   borders (none while maximised or full screen, like every desktop).
//!
//! Every call goes through the window command queue, so headless tests assert on
//! [`take_window_commands`](super::window::take_window_commands).

use pebbles_core::context::{action, action_event};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_render::PointerEvent;

use super::window::{
    ResizeEdge, WindowId, drag_resize_window, drag_window, show_window_menu, toggle_window_maximized,
    window_state,
};
use crate::widgets::{GestureDetector, Positioned, stack};

/// The resize border thickness (logical px) most client-side-decorated apps use.
pub const RESIZE_BORDER: f64 = 6.0;
/// A corner's grab square, a bit larger than the border so corners are easy to hit.
pub const RESIZE_CORNER: f64 = 12.0;

/// `child` as a title-bar drag region of window `id`: a drag moves the window, a
/// double-click toggles maximised, a right-click opens the window menu. Put buttons
/// and menus *above* it (they claim their own presses first).
pub fn window_drag_area(id: WindowId, child: impl IntoWidget) -> AnyWidget {
    GestureDetector::new(child)
        // Start the move once the press becomes a drag (past the pan slop), so a click
        // or a double-click on the bar stays a click.
        .on_pan_start(action(move || drag_window(id)))
        .on_double_tap(action(move || toggle_window_maximized(id)))
        .on_secondary_tap(action_event(move |e: PointerEvent| show_window_menu(id, e.global.x, e.global.y)))
        .into_widget()
}

/// `child` with invisible resize borders on all edges and corners of window `id`
/// (`border` px thick; [`RESIZE_BORDER`] is the usual). A press on a border hands the
/// resize to the window manager; each shows the matching resize cursor. Hidden while the
/// window is maximised or full screen. Reactive: re-renders when the state changes.
pub fn window_resize_frame(id: WindowId, border: f64, child: impl IntoWidget) -> AnyWidget {
    let st = window_state(id);
    let mut kids = vec![child.into_widget()];
    if !(st.maximized || st.fullscreen) {
        let corner = border.max(RESIZE_CORNER);
        for edge in ResizeEdge::ALL {
            let grip = GestureDetector::new(crate::widgets::Container::new())
                .cursor(edge.cursor())
                .on_pointer_down(action(move || drag_resize_window(id, edge)));
            let p = Positioned::new(grip);
            let p = match edge {
                ResizeEdge::North => p.top(0.0).left(corner).right(corner).height(border),
                ResizeEdge::South => p.bottom(0.0).left(corner).right(corner).height(border),
                ResizeEdge::West => p.left(0.0).top(corner).bottom(corner).width(border),
                ResizeEdge::East => p.right(0.0).top(corner).bottom(corner).width(border),
                ResizeEdge::NorthWest => p.top(0.0).left(0.0).width(corner).height(corner),
                ResizeEdge::NorthEast => p.top(0.0).right(0.0).width(corner).height(corner),
                ResizeEdge::SouthWest => p.bottom(0.0).left(0.0).width(corner).height(corner),
                ResizeEdge::SouthEast => p.bottom(0.0).right(0.0).width(corner).height(corner),
            };
            kids.push(p.into_widget());
        }
    }
    stack(kids).fit(pebbles_render::StackFit::Expand).into_widget()
}
