//! The ambient UI services — the layers that live *above* the widget tree and
//! are driven imperatively from anywhere: the [`overlay`] host (dropdowns,
//! menus, popovers, tooltips), modal [`dialog`]s, side [`sheet`]s, [`toast`]
//! notifications, and the [`global_menu`] right-click fallback.
//!
//! Each keeps a process-global registry that a mounted `OverlayHost` renders, so
//! calling code needs no handle. Grouped for navigation only: every module here
//! is re-exported at the crate root (`pebbles_widgets::overlay`, …).

pub mod dialog;
pub mod global_menu;
pub mod overlay;
pub mod sheet;
pub mod toast;
