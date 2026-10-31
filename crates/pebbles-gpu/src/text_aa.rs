//! Text antialiasing mode: grayscale or subpixel (LCD) coverage (Skia / FreeType / Chromium
//! practice). LCD text is drawn only where its per-channel coverage can be composited exactly:
//!
//! - the device has `Features::DUAL_SOURCE_BLENDING` (absent on WebGL and some GL drivers);
//! - the target is opaque (`FrameParams::base_color` alpha 1) and not a GL sRGB-decode surface;
//! - the run is on the atlas route (axis-aligned, uniform positive scale; rotation, skew,
//!   non-uniform scale and per-glyph transforms take the outline route, which stays grayscale);
//! - the run is not inside an offscreen layer (opacity groups, blend modes, clips rendered to
//!   a transparent intermediate) — as Chromium, which drops LCD text in non-opaque layers.
//!
//! Everything else falls back to the grayscale atlas, which is always kept.

use std::sync::atomic::{AtomicU8, Ordering};

/// How glyph edges are antialiased.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextAntialiasing {
    /// Follow the system setting (GNOME `font-antialiasing` / `font-rgba-order`; Windows:
    /// ClearType RGB; elsewhere grayscale), overridable by `PEBBLES_TEXT_AA`.
    #[default]
    Auto,
    /// Subpixel (LCD) coverage where supported (see the module rules), else grayscale.
    Subpixel,
    /// Grayscale coverage everywhere.
    Grayscale,
}

impl TextAntialiasing {
    /// Parse `auto` / `subpixel` (`lcd`, `rgb`, `bgr`) / `grayscale` (`gray`, `antialiased`).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "auto" | "default" => Some(Self::Auto),
            "subpixel" | "lcd" | "rgb" | "bgr" | "subpixel-antialiased" => Some(Self::Subpixel),
            "grayscale" | "gray" | "greyscale" | "antialiased" | "none" => Some(Self::Grayscale),
            _ => None,
        }
    }
}

/// Physical order of a pixel's subpixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SubpixelOrder {
    /// Horizontal R, G, B stripes (most panels).
    Rgb,
    /// Horizontal B, G, R stripes.
    Bgr,
    /// Vertical stripes: not supported (grayscale), as most toolkits.
    VRgb,
    /// Vertical B-over-R stripes: grayscale.
    VBgr,
    /// No subpixel structure (OLED pentile, projectors): grayscale.
    None,
}

/// A resolved horizontal LCD order (the only orders the atlas encodes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LcdOrder {
    Rgb,
    Bgr,
}

impl LcdOrder {
    /// `GlyphKey::lcd` / `RasterRequest::lcd`: 1 = RGB, 2 = BGR (0 = grayscale).
    pub(crate) fn code(self) -> u8 {
        match self {
            Self::Rgb => 1,
            Self::Bgr => 2,
        }
    }
}

/// The system's text antialiasing as `(subpixel wanted, order)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SystemTextAa {
    pub subpixel: bool,
    pub order: SubpixelOrder,
}

/// GNOME `font-antialiasing` (`'rgba'` | `'grayscale'` | `'none'`) and `font-rgba-order`
/// (`'rgb'` | `'bgr'` | `'vrgb'` | `'vbgr'`), as printed by `gsettings get`.
pub(crate) fn parse_gnome(antialiasing: &str, order: &str) -> SystemTextAa {
    let clean = |s: &str| s.trim().trim_matches('\'').to_ascii_lowercase();
    let order = match clean(order).as_str() {
        "bgr" => SubpixelOrder::Bgr,
        "vrgb" => SubpixelOrder::VRgb,
        "vbgr" => SubpixelOrder::VBgr,
        "none" => SubpixelOrder::None,
        _ => SubpixelOrder::Rgb,
    };
    SystemTextAa { subpixel: clean(antialiasing) == "rgba", order }
}

/// The system setting, read once. Linux: `gsettings` (GNOME and the GNOME settings portal's
/// backing store); unavailable → grayscale. Windows: ClearType RGB. Others: grayscale (macOS
/// dropped subpixel AA in 10.14).
pub(crate) fn system_text_aa() -> SystemTextAa {
    static S: std::sync::OnceLock<SystemTextAa> = std::sync::OnceLock::new();
    *S.get_or_init(|| {
        let gray = SystemTextAa { subpixel: false, order: SubpixelOrder::Rgb };
        if cfg!(target_os = "windows") {
            return SystemTextAa { subpixel: true, order: SubpixelOrder::Rgb };
        }
        #[cfg(all(unix, not(target_os = "macos"), not(target_family = "wasm")))]
        {
            let get = |key: &str| {
                std::process::Command::new("gsettings")
                    .args(["get", "org.gnome.desktop.interface", key])
                    .stderr(std::process::Stdio::null())
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            };
            if let (Some(aa), Some(order)) = (get("font-antialiasing"), get("font-rgba-order")) {
                return parse_gnome(&aa, &order);
            }
        }
        gray
    })
}

/// 0 = unset, else `TextAntialiasing` + 1.
static APP_DEFAULT: AtomicU8 = AtomicU8::new(0);

fn encode(aa: TextAntialiasing) -> u8 {
    match aa {
        TextAntialiasing::Auto => 1,
        TextAntialiasing::Subpixel => 2,
        TextAntialiasing::Grayscale => 3,
    }
}

/// Set the process-wide text antialiasing that window frames use (an app setting, e.g. Gravel
/// `workbench.fontAliasing`). `PEBBLES_TEXT_AA` still wins, as the other `PEBBLES_*` overrides.
pub fn set_text_antialiasing(aa: TextAntialiasing) {
    APP_DEFAULT.store(encode(aa), Ordering::Relaxed);
}

/// The text antialiasing for window frames: `PEBBLES_TEXT_AA`, else the app's
/// [`set_text_antialiasing`], else `Auto`.
pub fn text_antialiasing() -> TextAntialiasing {
    static ENV: std::sync::OnceLock<Option<TextAntialiasing>> = std::sync::OnceLock::new();
    if let Some(aa) =
        *ENV.get_or_init(|| std::env::var("PEBBLES_TEXT_AA").ok().and_then(|v| TextAntialiasing::parse(&v)))
    {
        return aa;
    }
    match APP_DEFAULT.load(Ordering::Relaxed) {
        2 => TextAntialiasing::Subpixel,
        3 => TextAntialiasing::Grayscale,
        _ => TextAntialiasing::Auto,
    }
}

/// `PEBBLES_TEXT_AA=bgr` forces the BGR order (testing a BGR panel).
fn env_order() -> Option<SubpixelOrder> {
    static O: std::sync::OnceLock<Option<SubpixelOrder>> = std::sync::OnceLock::new();
    *O.get_or_init(|| match std::env::var("PEBBLES_TEXT_AA").ok()?.trim().to_ascii_lowercase().as_str() {
        "bgr" => Some(SubpixelOrder::Bgr),
        "rgb" => Some(SubpixelOrder::Rgb),
        _ => None,
    })
}

/// The frame-level inputs of the LCD decision.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LcdInputs {
    pub mode: TextAntialiasing,
    /// `FrameParams::subpixel_order`; `None` = the system's.
    pub order: Option<SubpixelOrder>,
    pub system: SystemTextAa,
    pub dual_source: bool,
    pub opaque_target: bool,
    pub decode_srgb_output: bool,
}

/// The frame's LCD order, or `None` for grayscale (module rules; per-run rules apply in prepare).
pub(crate) fn resolve_lcd(i: LcdInputs) -> Option<LcdOrder> {
    let wanted = match i.mode {
        TextAntialiasing::Grayscale => false,
        TextAntialiasing::Subpixel => true,
        TextAntialiasing::Auto => i.system.subpixel,
    };
    if !wanted || !i.dual_source || !i.opaque_target || i.decode_srgb_output {
        return None;
    }
    match i.order.or(env_order()).unwrap_or(i.system.order) {
        SubpixelOrder::Rgb => Some(LcdOrder::Rgb),
        SubpixelOrder::Bgr => Some(LcdOrder::Bgr),
        SubpixelOrder::VRgb | SubpixelOrder::VBgr | SubpixelOrder::None => None,
    }
}

/// CPU reference of the LCD blend (`glyph_lcd.wgsl` + `{One, OneMinusSrc1}`), 8-bit in/out:
/// `dst' = color·m + dst·(1 − a·m)` per channel, `m` = the text-curve coverage of each subpixel.
/// `color` is premultiplied RGBA, `cov` the RGB subpixel coverage in panel order.
pub(crate) fn lcd_blend_reference(
    dst: [u8; 3],
    color: [u8; 4],
    cov: [u8; 3],
    gamma: f32,
    contrast: f32,
) -> [u8; 3] {
    let a = f32::from(color[3]) / 255.0;
    let straight = if a > 0.0 { [0, 1, 2].map(|k| f32::from(color[k]) / 255.0 / a) } else { [0.0; 3] };
    let ratios = crate::execute::resources::globals::text_gamma_ratios(gamma);
    [0, 1, 2].map(|k| {
        let m = crate::execute::resources::globals::text_coverage(
            f32::from(cov[k]) / 255.0,
            straight,
            contrast,
            ratios,
        );
        let src = f32::from(color[k]) / 255.0 * m;
        let src = (src * 255.0).round() / 255.0; // `store_unorm8` in finish_color
        let out = src + f32::from(dst[k]) / 255.0 * (1.0 - a * m);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = (out.clamp(0.0, 1.0) * 255.0).round() as u8;
        v
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(mode: TextAntialiasing) -> LcdInputs {
        LcdInputs {
            mode,
            order: Some(SubpixelOrder::Rgb),
            system: SystemTextAa { subpixel: true, order: SubpixelOrder::Rgb },
            dual_source: true,
            opaque_target: true,
            decode_srgb_output: false,
        }
    }

    #[test]
    fn gnome_settings_parse() {
        assert_eq!(
            parse_gnome("'rgba'\n", "'rgb'\n"),
            SystemTextAa { subpixel: true, order: SubpixelOrder::Rgb }
        );
        assert_eq!(
            parse_gnome("'grayscale'", "'bgr'"),
            SystemTextAa { subpixel: false, order: SubpixelOrder::Bgr }
        );
        assert_eq!(parse_gnome("'rgba'", "'vrgb'").order, SubpixelOrder::VRgb);
    }

    #[test]
    fn modes_resolve() {
        assert_eq!(resolve_lcd(inputs(TextAntialiasing::Subpixel)), Some(LcdOrder::Rgb));
        assert_eq!(resolve_lcd(inputs(TextAntialiasing::Auto)), Some(LcdOrder::Rgb));
        assert_eq!(resolve_lcd(inputs(TextAntialiasing::Grayscale)), None);
        let mut i = inputs(TextAntialiasing::Auto);
        i.system.subpixel = false;
        assert_eq!(resolve_lcd(i), None, "Auto follows a grayscale system");
        i.mode = TextAntialiasing::Subpixel;
        i.order = Some(SubpixelOrder::Bgr);
        assert_eq!(resolve_lcd(i), Some(LcdOrder::Bgr));
    }

    #[test]
    fn fallbacks_choose_grayscale() {
        let base = inputs(TextAntialiasing::Subpixel);
        assert_eq!(resolve_lcd(LcdInputs { dual_source: false, ..base }), None, "no dual-source blending");
        assert_eq!(resolve_lcd(LcdInputs { opaque_target: false, ..base }), None, "transparent target");
        assert_eq!(resolve_lcd(LcdInputs { decode_srgb_output: true, ..base }), None, "GL sRGB decode");
        for o in [SubpixelOrder::VRgb, SubpixelOrder::VBgr, SubpixelOrder::None] {
            assert_eq!(resolve_lcd(LcdInputs { order: Some(o), ..base }), None, "{o:?}");
        }
    }

    #[test]
    fn parse_modes() {
        assert_eq!(TextAntialiasing::parse("default"), Some(TextAntialiasing::Auto));
        assert_eq!(TextAntialiasing::parse("antialiased"), Some(TextAntialiasing::Grayscale));
        assert_eq!(TextAntialiasing::parse("subpixel-antialiased"), Some(TextAntialiasing::Subpixel));
        assert_eq!(TextAntialiasing::parse("x"), None);
    }

    #[test]
    fn reference_blend_is_per_channel() {
        // Black text, full red-subpixel coverage only, over white: red darkens, others stay.
        let out = lcd_blend_reference([255; 3], [0, 0, 0, 255], [255, 0, 0], 1.0, 0.0);
        assert_eq!(out, [0, 255, 255]);
        // Equal coverage reduces to the grayscale over.
        let g = lcd_blend_reference([255; 3], [0, 0, 0, 255], [128; 3], 1.0, 0.0);
        assert_eq!(g[0], g[1]);
        assert_eq!(g[1], g[2]);
        assert!((126..=128).contains(&g[0]), "{g:?}");
    }
}
