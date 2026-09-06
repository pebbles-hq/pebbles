//! Local persistence, backend-split by platform: a bundled-SQLite store on native
//! (`native`) and an in-memory twin on the web (`web`), behind one identical API.
//! `store.rs` calls these functions without caring which is compiled.

#[cfg(not(target_family = "wasm"))]
mod native;
#[cfg(not(target_family = "wasm"))]
pub use native::*;

#[cfg(target_family = "wasm")]
mod web;
#[cfg(target_family = "wasm")]
pub use web::*;
