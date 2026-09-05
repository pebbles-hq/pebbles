//! One file per tab.

mod feed;
mod messages;
mod notifications;
mod post_detail;
mod profile;

pub use feed::feed;
pub use messages::messages;
pub use notifications::notifications;
pub use post_detail::post_detail;
pub use profile::profile;
