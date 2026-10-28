//! Deterministic headless rendering of one page through the GPU-free test harness (the one
//! `tools/render-check` drives): fixed logical size, bundled fonts only, every animation
//! settled before the frame is recorded.

use pebbles::prelude::*;
use pebbles_testing::Harness;

use crate::registry::Page;

/// Logical width of a headless page render.
pub const WIDTH: u32 = 800;
/// Logical height of a headless page render.
pub const HEIGHT: u32 = 1000;

/// The theme a page is rendered under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look {
    /// Design language.
    pub design: DesignLanguage,
    /// Dark palette instead of light.
    pub dark: bool,
    /// Right-to-left text direction.
    pub rtl: bool,
}

impl Look {
    /// Default design, light, left-to-right.
    pub const DEFAULT: Look = Look { design: DesignLanguage::Default, dark: false, rtl: false };
}

/// A fresh harness at the headless size (bundled fonts only).
pub fn harness() -> Harness {
    Harness::new().window(f64::from(WIDTH), f64::from(HEIGHT))
}

/// Apply `look`, mount `page` alone (no shell chrome) into `h` and settle it.
pub fn mount(h: &mut Harness, page: &Page, look: Look) {
    crate::ui::set_offline(true);
    set_theme(if look.dark { Theme::dark() } else { Theme::light() }.design(look.design));
    set_text_direction(if look.rtl { TextDirection::Rtl } else { TextDirection::Ltr });
    // Like the shell: the page over the theme background, inside the overlay host (so its
    // dialogs, popovers, tooltips and toasts render).
    let root = OverlayHost::wrap(component(page.build).into_widget());
    h.mount_widget(View::new(theme().colors.background, root));
    h.settle();
}

/// The center of the first semantics node labelled `label` (a button's text, a field's
/// label), if it is on screen.
pub fn find(h: &Harness, label: &str) -> Option<Offset> {
    h.semantics_tree().into_iter().find(|n| n.props.label == label).map(|n| {
        let b = n.bounds;
        Offset::new((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0)
    })
}

/// Click the control labelled `label`, then settle.
///
/// # Panics
/// If no node carries `label` (the page changed; update the caller's label).
pub fn click(h: &mut Harness, label: &str) {
    let at = find(h, label).unwrap_or_else(|| panic!("no control labelled {label:?} on the page"));
    h.click(at);
    h.settle();
}

/// Hover the control labelled `label`, then settle.
///
/// # Panics
/// If no node carries `label`.
pub fn hover(h: &mut Harness, label: &str) {
    let at = find(h, label).unwrap_or_else(|| panic!("no control labelled {label:?} on the page"));
    h.hover(at);
    // Run the clock past any hover delay (tooltips default to 0.5 s; slow ones 0.6 s).
    h.advance(1.0);
}
