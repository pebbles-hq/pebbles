//! Every gallery page mounts and settles headless; the overlay pages open their overlays.
#![cfg(feature = "headless")]

use gallery::headless::{self, Look};

#[test]
fn every_page_mounts_in_every_look() {
    for page in gallery::PAGES {
        for look in [Look::DEFAULT, Look { dark: true, ..Look::DEFAULT }, Look { rtl: true, ..Look::DEFAULT }]
        {
            let mut h = headless::harness();
            headless::mount(&mut h, page, look);
            let _ = h.draw();
        }
    }
}

#[test]
fn toast_opens() {
    let mut h = headless::harness();
    headless::mount(&mut h, gallery::page("toasts").unwrap(), Look::DEFAULT);
    headless::click(&mut h, "With action");
    let _ = h.draw();
    assert!(h.find_text("Message archived"), "toast not shown");
}
