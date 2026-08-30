//! The component catalog — higher-level, themed components in the spirit of
//! shadcn/ui, built from the primitives in [`widgets`](crate::widgets) and styled
//! from the current [`Theme`](crate::Theme). Grouped by role so the catalog scales:
//!
//! * [`input`] — buttons, toggles (checkbox / radio / switch / toggle)
//! * [`display`] — surfaces, data, typography, icons, progress
//! * [`layout`] — scaffold & nav chrome, split panes, panels, disclosure
//! * [`navigation`] — breadcrumb, pagination, toolbars, tabs, routing
//!
//! Every component is also re-exported flat at this module's root, so callers can
//! `use pebbles::prelude::*` (or `pebbles_widgets::components::*`) without caring
//! which group a component lives in.

pub mod display;
pub mod input;
pub mod layout;
pub mod navigation;

pub use display::*;
pub use input::*;
pub use layout::*;
pub use navigation::*;

/// The icon model (`IconData`/`IconPrim`), the named [`IconKind`] handles, and
/// the bundled [`lucide`] icon set.
pub use pebbles_render::{IconData, IconKind, IconPrim, lucide};
