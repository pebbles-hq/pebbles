//! One file per tab.

mod feed;
mod messages;
mod notifications;
mod profile;

pub use feed::feed;
pub use messages::messages;
pub use notifications::notifications;
pub use profile::profile;
