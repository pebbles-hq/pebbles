//! Structural layout & app-chrome components — scaffold, navigation chrome,
//! split panes, panels and disclosure.

mod chrome;
mod disclosure;
mod panel;
mod split;

pub use chrome::{
    BottomNav, BottomNavItem, NavItem, Scaffold, SideNav, TopPanel, bottom_nav, bottom_nav_item,
    nav_item, scaffold, side_nav, top_panel,
};
pub use disclosure::{Accordion, Collapsible, accordion, collapsible};
pub use panel::{Panel, panel};
pub use split::{SplitView, split_view};
