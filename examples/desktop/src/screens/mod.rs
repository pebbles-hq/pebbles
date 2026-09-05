//! One module per section of the app.

mod customers;
mod dashboard;
mod orders;
mod products;
mod settings;

pub use customers::customers;
pub use dashboard::dashboard;
pub use orders::orders;
pub use products::products;
pub use settings::settings;
