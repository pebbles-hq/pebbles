//! `render-check`: the pixel render-check harness of the wgpu migration
//! (documentations/wgpu-migration/05-testing-and-ci.md §6). Dev tool; never shipped.

pub mod args;
pub mod case;
pub mod corpus;
pub mod metric;
pub mod rng;
pub mod status;

pub use status::{Status, exit_code};
