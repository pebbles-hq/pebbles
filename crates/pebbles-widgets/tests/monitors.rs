//! F5: the widgets-side monitor accessor returns whatever snapshot the shell mirror
//! holds. Shell-side enumeration is manual; here we inject a fake and read it back.

use pebbles_widgets::{MonitorInfo, monitors, set_monitors};

#[test]
fn monitors_reflect_the_injected_snapshot() {
    assert!(monitors().is_empty(), "empty until the shell publishes");

    let fake = vec![
        MonitorInfo {
            name: "DELL U2720Q".into(),
            position: (0, 0),
            size: (3840, 2160),
            scale: 2.0,
            primary: true,
        },
        MonitorInfo {
            name: "Built-in".into(),
            position: (3840, 0),
            size: (1512, 982),
            scale: 2.0,
            primary: false,
        },
    ];
    set_monitors(fake.clone());
    assert_eq!(monitors(), fake, "accessor returns the published snapshot");

    // Re-publishing the same snapshot is a no-op (dedup), and a change replaces it.
    set_monitors(fake.clone());
    let one = vec![fake[0].clone()];
    set_monitors(one.clone());
    assert_eq!(monitors(), one);
}
