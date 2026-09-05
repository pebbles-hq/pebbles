//! Reusable pieces. `bits` are shared atoms; `post_card` renders a feed post;
//! `compose` is the new-post bottom sheet; `post_menu` is the ⋯ action sheet.

pub mod bits;
pub mod compose;
mod post_card;
pub mod post_menu;

pub use post_card::post_card;
