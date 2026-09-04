//! The design system — everything that decides how the catalog *looks*, rather
//! than what it is: the [`theme`] tokens, the general [`style`] system, the
//! [`modifiers`] extension trait, font discovery ([`fonts`]) and the ambient
//! [`text_direction`].
//!
//! Grouped for navigation only: every module here is re-exported at the crate
//! root, so the public paths stay `pebbles_widgets::theme`,
//! `pebbles_widgets::style`, and so on.

pub mod fonts;
pub mod modifiers;
pub mod style;
pub mod text_direction;
pub mod theme;
