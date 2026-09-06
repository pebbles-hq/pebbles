//! Reusable UI components composed from the framework — badges, cards, a responsive
//! grid, media atoms, the notifications popover, and data-table helpers (search field,
//! pagination footer, empty state). The screens are mostly just arrangements of these,
//! which is the point: an app is small reusable pieces on top of the widget library.

mod badges;
mod cards;
mod data_table;
mod format;
mod layout;
mod media;
mod notifications;

pub use badges::{order_badge, stock_badge};
pub use cards::{kpi_card, panel, table_card};
pub use data_table::{search_field, table_empty, table_pager};
pub use format::{mix, price};
pub use layout::responsive_grid;
pub use media::{stars, thumb};
pub use notifications::notifications_button;
