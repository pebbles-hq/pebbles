//! Design languages (§2 blocker): `DesignLanguage::{Default,Material,Tailwind}` is
//! applied across the whole catalog via `set_design`, yet nothing tested that the
//! switch actually reshapes the tokens. A token regression would silently restyle
//! every catalog widget. One test per language pins the seeded radius / spacing /
//! control padding + height / density (and the density-scaled `pad_all`), then
//! mounts a widget under that language to prove the catalog re-skins without panic.

use pebbles_foundation::EdgeInsets;
use pebbles_testing::Harness;
use pebbles_widgets::button;
use pebbles_widgets::theme::{DesignLanguage, Theme, set_design, set_theme, theme};

/// Mounting a real control under `lang` must lay out + paint (the "applied
/// catalog-wide" smoke).
fn skinned_button() -> impl pebbles_core::IntoWidget {
    button("Go").on_pressed(|| {})
}

fn draw_a_widget() {
    let mut h = Harness::new();
    h.mount(skinned_button);
    h.draw();
}

#[test]
fn default_seeds_sharp_dense_tokens() {
    set_design(DesignLanguage::Default);
    let t = theme();
    assert_eq!(t.design, DesignLanguage::Default);
    assert_eq!(t.radius, 0.0, "Default is truly square");
    assert_eq!(t.spacing, 6.0);
    assert_eq!(t.control_pad_x, 10.0);
    assert_eq!(t.control_pad_y, 4.0);
    assert_eq!(t.control_height, 28.0);
    assert_eq!(t.density, 0.82);
    assert_eq!(t.pill(), t.radius, "Default tags are square, not pills");
    assert_eq!(t.pad_all(8.0), EdgeInsets::all(8.0 * 0.82), "padding scales by density");
    draw_a_widget();
    set_theme(Theme::light());
}

#[test]
fn tailwind_seeds_the_original_rounded_tokens() {
    set_design(DesignLanguage::Tailwind);
    let t = theme();
    assert_eq!(t.design, DesignLanguage::Tailwind);
    assert_eq!(t.radius, 8.0);
    assert_eq!(t.spacing, 8.0);
    assert_eq!(t.control_pad_x, 16.0);
    assert_eq!(t.control_pad_y, 9.0);
    assert_eq!(t.control_height, 36.0);
    assert_eq!(t.density, 1.0);
    assert_eq!(t.pill(), 999.0, "Tailwind tags are fully rounded");
    assert_eq!(t.pad_all(8.0), EdgeInsets::all(8.0), "density 1.0 = unscaled padding");
    draw_a_widget();
    set_theme(Theme::light());
}

#[test]
fn material_seeds_roomy_elevated_tokens() {
    set_design(DesignLanguage::Material);
    let t = theme();
    assert_eq!(t.design, DesignLanguage::Material);
    assert_eq!(t.radius, 18.0);
    assert_eq!(t.spacing, 8.0);
    assert_eq!(t.control_pad_x, 24.0);
    assert_eq!(t.control_pad_y, 10.0);
    assert_eq!(t.control_height, 40.0);
    assert_eq!(t.density, 1.2);
    assert_eq!(t.pill(), 999.0, "Material tags are fully rounded");
    assert_eq!(t.pad_all(10.0), EdgeInsets::all(10.0 * 1.2), "padding scales by density");
    draw_a_widget();
    set_theme(Theme::light());
}
