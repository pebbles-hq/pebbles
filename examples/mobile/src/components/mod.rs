//! Reusable pieces. `bits` are shared atoms; `post_card` renders a feed post;
//! `compose` and `comments` are bottom-sheet flows opened from the UI.

pub mod bits;
pub mod comments;
pub mod compose;
mod post_card;
pub mod post_menu;

pub use post_card::post_card;
