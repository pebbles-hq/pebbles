//! Input concern — keyboard, key identity, shortcuts, focus, and scroll input.
//!
//! Grouped for navigation; each stays reachable at its original crate-root path
//! (`pebbles_core::focus`, `pebbles_core::keyboard`, …) via re-export in `lib.rs`.

pub mod focus;
pub mod key;
pub mod keyboard;
pub mod scroll;
pub mod shortcuts;
