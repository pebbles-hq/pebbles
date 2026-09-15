//! Control-flow widgets — SolidJS-style `<For>` / `<Suspense>` / `<ErrorBoundary>`.
//!
//! (Solid's `<Show>` / `<Switch>` have no entry here: Rust's own `if` / `match` in a
//! component body already are them.)

mod error_boundary;
mod for_each;
mod suspense;

pub use error_boundary::error_boundary;
pub use for_each::for_each;
pub use suspense::suspense;
