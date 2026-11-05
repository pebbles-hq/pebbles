//! Gravel sweep S11: the desktop runner wakes an idle app when the background system-font
//! scan finishes (text shaped before it, e.g. CJK file names, stayed tofu until the next
//! input). The runner reads `TextEnv::font_scan_state` in `about_to_wait`.

use pebbles_render::{SystemFonts, TextEnv};

#[test]
fn a_background_scan_reports_running_then_finished_then_merged() {
    let mut env = TextEnv::with_system_fonts(SystemFonts::Background);
    assert!(env.font_scan_state().is_some(), "a scan is pending after start");
    let t0 = std::time::Instant::now();
    while env.font_scan_state() == Some(false) {
        assert!(t0.elapsed().as_secs() < 120, "the scan never finished");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(env.font_scan_state(), Some(true), "finished, not merged yet");
    env.poll_fonts();
    assert_eq!(env.font_scan_state(), None, "merged by the poll");

    let none = TextEnv::with_system_fonts(SystemFonts::None);
    assert_eq!(none.font_scan_state(), None);
}
