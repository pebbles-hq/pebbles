//! [`Theme`] — design tokens shared across the component catalog, in the spirit of
//! shadcn/ui's CSS variables. The "current" theme is a **global reactive signal**:
//! [`theme`] reads it (subscribing the calling component), and [`set_theme`]/
//! [`toggle_theme`] swap it and re-render every component that read it — so a live
//! light/dark toggle just works, no restart, no provider plumbing.
//!
//! Light vs dark follows a [`ThemeMode`] (like Flutter's `ThemeMode`): the default,
//! [`ThemeMode::System`], tracks the platform [`Brightness`] the shell reports
//! ([`set_platform_brightness`]); [`set_theme`] / [`toggle_theme`] /
//! [`set_theme_mode`] pin an explicit `Light` or `Dark`.

use std::cell::RefCell;

use pebbles_core::widget::IntoWidget as _;
use pebbles_core::{Signal, create_root_signal};
use pebbles_foundation::{Color, EdgeInsets, Offset};
use pebbles_render::BoxShadow;

/// The semantic color roles a component can reference.
#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub background: Color,
    pub foreground: Color,
    pub card: Color,
    pub card_foreground: Color,
    pub popover: Color,
    pub popover_foreground: Color,
    pub primary: Color,
    pub primary_foreground: Color,
    pub secondary: Color,
    pub secondary_foreground: Color,
    pub muted: Color,
    pub muted_foreground: Color,
    pub accent: Color,
    pub accent_foreground: Color,
    pub destructive: Color,
    pub destructive_foreground: Color,
    pub success: Color,
    pub warning: Color,
    pub border: Color,
    pub input: Color,
    pub ring: Color,
}

/// The **base design language** — the single switch that reshapes the whole
/// component catalog (like Flutter's Material vs Cupertino). It seeds the
/// shape / density / elevation tokens; colors stay orthogonal, and every token
/// (and every per-component setter) remains overridable on top. Pick a base,
/// then customize as far as you like.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DesignLanguage {
    /// Dense, flat, near-sharp corners, minimal padding — the desktop /
    /// productivity / power-user default (no decorative chrome).
    #[default]
    Default,
    /// Google **Material 3**: large radii, generous touch targets, elevation —
    /// the closest base for a mobile app.
    Material,
    /// Rounded corners, roomy padding, subtle borders. (Pebbles' original default.)
    Tailwind,
}

/// The shape / density / elevation values a [`DesignLanguage`] seeds a [`Theme`]
/// with. Not public — themes expose the resolved fields, which stay overridable.
#[derive(Clone, Copy)]
struct DesignTokens {
    radius: f64,
    spacing: f64,
    control_pad_x: f64,
    control_pad_y: f64,
    control_height: f64,
    border_width: f64,
    elevation: f64,
    density: f64,
    font_size: f32,
    sizes: SizeScale,
}

/// The pixel metrics of one control size: everything a sized component (button,
/// icon button, toggle, input, select, chip, segmented control, pagination item,
/// menubar trigger) needs to lay out at that size. Every sized component at the
/// same [`ControlSize`] uses the same `height`, so mixed rows line up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ControlMetrics {
    /// Outer control height (border included). Icon buttons are `height` square.
    pub height: f64,
    /// Label font size.
    pub font: f32,
    /// Horizontal inner padding of a labelled control.
    pub padding_x: f64,
    /// Icon glyph size.
    pub icon: f64,
}

impl ControlMetrics {
    /// Metrics for an arbitrary `height`, with font / padding / icon derived
    /// proportionally (the Default md ratios: 28 px ⇒ 13 / 10 / 16). Works down to
    /// ~16 px tall controls (font floors at 8 px). Override any field afterwards.
    pub fn from_height(height: f64) -> Self {
        let height = height.max(8.0);
        let half = |v: f64| (v * 2.0).round() / 2.0;
        ControlMetrics {
            height,
            font: half(height * 0.465).max(8.0) as f32,
            padding_x: half(height * 0.36).max(2.0),
            icon: half(height * 0.57).max(8.0).min(height),
        }
    }
    /// Replace the font size.
    pub fn font(mut self, font: f32) -> Self {
        self.font = font;
        self
    }
    /// Replace the horizontal padding.
    pub fn padding_x(mut self, padding_x: f64) -> Self {
        self.padding_x = padding_x;
        self
    }
    /// Replace the icon size.
    pub fn icon(mut self, icon: f64) -> Self {
        self.icon = icon;
        self
    }
}

/// The size of a control. `Md` is the standard desktop size; `Custom` takes any
/// metrics a developer wants (see [`ControlSize::custom`]). Shared by every sized
/// component, so `.size(ControlSize::Sm)` means the same height everywhere.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub enum ControlSize {
    Xs,
    Sm,
    #[default]
    Md,
    Lg,
    /// Explicit metrics (any height, font, padding, icon size).
    Custom(ControlMetrics),
}

impl ControlSize {
    /// A custom size of `height` px with proportional font / padding / icon
    /// ([`ControlMetrics::from_height`]).
    pub fn custom(height: f64) -> Self {
        ControlSize::Custom(ControlMetrics::from_height(height))
    }
    /// Resolve against the current [`theme`]'s [`SizeScale`].
    pub fn metrics(self) -> ControlMetrics {
        theme().size(self)
    }
}

impl From<ControlMetrics> for ControlSize {
    fn from(m: ControlMetrics) -> Self {
        ControlSize::Custom(m)
    }
}

/// The xs / sm / md / lg [`ControlMetrics`] of a theme — seeded by the
/// [`DesignLanguage`], overridable like every other token.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SizeScale {
    pub xs: ControlMetrics,
    pub sm: ControlMetrics,
    pub md: ControlMetrics,
    pub lg: ControlMetrics,
}

const fn cm(height: f64, font: f32, padding_x: f64, icon: f64) -> ControlMetrics {
    ControlMetrics { height, font, padding_x, icon }
}

impl DesignLanguage {
    const fn tokens(self) -> DesignTokens {
        match self {
            // Sharp, tight, flat — power-user chrome. Zero radius (truly square).
            DesignLanguage::Default => DesignTokens {
                radius: 0.0,
                spacing: 6.0,
                control_pad_x: 10.0,
                control_pad_y: 4.0,
                control_height: 28.0,
                border_width: 1.0,
                elevation: 0.0,
                density: 0.82,
                // Standard desktop metrics (VS Code / JetBrains / Zed / GTK / macOS):
                // 13 px UI text, 28 px controls, 16 px icons.
                font_size: 13.0,
                sizes: SizeScale {
                    xs: cm(20.0, 12.0, 6.0, 14.0),
                    sm: cm(24.0, 12.0, 8.0, 14.0),
                    md: cm(28.0, 13.0, 10.0, 16.0),
                    lg: cm(32.0, 14.0, 12.0, 18.0),
                },
            },
            // The original shadcn/tailwind look (unchanged from Pebbles' first default).
            DesignLanguage::Tailwind => DesignTokens {
                radius: 8.0,
                spacing: 8.0,
                control_pad_x: 16.0,
                control_pad_y: 9.0,
                control_height: 36.0,
                border_width: 1.0,
                elevation: 1.0,
                density: 1.0,
                font_size: 14.0,
                sizes: SizeScale {
                    xs: cm(28.0, 12.0, 8.0, 14.0),
                    sm: cm(32.0, 13.0, 12.0, 16.0),
                    md: cm(36.0, 14.0, 16.0, 16.0),
                    lg: cm(40.0, 15.0, 22.0, 18.0),
                },
            },
            // Material 3: pill-ish radii, big targets, tonal elevation.
            DesignLanguage::Material => DesignTokens {
                radius: 18.0,
                spacing: 8.0,
                control_pad_x: 24.0,
                control_pad_y: 10.0,
                control_height: 40.0,
                border_width: 1.0,
                elevation: 3.0,
                density: 1.2,
                font_size: 14.0,
                sizes: SizeScale {
                    xs: cm(32.0, 12.0, 12.0, 16.0),
                    sm: cm(36.0, 13.0, 16.0, 18.0),
                    md: cm(40.0, 14.0, 24.0, 18.0),
                    lg: cm(48.0, 15.0, 32.0, 20.0),
                },
            },
        }
    }
}

/// The full token set: the base [`design`](Self::design) language plus the
/// resolved colors, corner radius, spacing, control density, border width,
/// elevation and font size it seeds. Every field is public and overridable —
/// switch [`design`](Self::design) for a wholesale re-skin, then tweak fields
/// (or individual components) from there.
#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub colors: Colors,
    /// The base design language these tokens were seeded from.
    pub design: DesignLanguage,
    /// Base corner radius (buttons, inputs, cards).
    pub radius: f64,
    /// Base spacing unit; paddings/gaps are multiples of this.
    pub spacing: f64,
    /// Default horizontal padding inside a control (button/input) at medium size.
    pub control_pad_x: f64,
    /// Default vertical padding inside a control at medium size.
    pub control_pad_y: f64,
    /// Minimum control height — the click/touch target (small on desktop, large on mobile).
    pub control_height: f64,
    /// Default border/outline thickness.
    pub border_width: f64,
    /// Default elevation (0 = flat). Drives whether surfaces cast a shadow; see
    /// [`elevation_shadow`](Self::elevation_shadow).
    pub elevation: f64,
    /// Padding-density multiplier (Default `<1`, Material `>1`). Scale a control's
    /// interior padding by this to make it tighten/loosen with the design language.
    pub density: f64,
    /// Base body font size (13 in the Default design — the desktop UI standard).
    pub font_size: f32,
    /// The control size scale (xs / sm / md / lg) every sized component uses.
    pub sizes: SizeScale,
    pub dark: bool,
}

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

impl Theme {
    /// The default light theme — Default design, zinc/neutral palette.
    pub fn light() -> Self {
        Self::seed(
            &Colors {
                background: rgb(0xFF, 0xFF, 0xFF),
                foreground: rgb(0x0A, 0x0A, 0x0A),
                card: rgb(0xFF, 0xFF, 0xFF),
                card_foreground: rgb(0x0A, 0x0A, 0x0A),
                popover: rgb(0xFF, 0xFF, 0xFF),
                popover_foreground: rgb(0x0A, 0x0A, 0x0A),
                primary: rgb(0x18, 0x18, 0x1B),
                primary_foreground: rgb(0xFA, 0xFA, 0xFA),
                secondary: rgb(0xF4, 0xF4, 0xF5),
                secondary_foreground: rgb(0x18, 0x18, 0x1B),
                muted: rgb(0xF4, 0xF4, 0xF5),
                muted_foreground: rgb(0x71, 0x71, 0x7A),
                accent: rgb(0xF4, 0xF4, 0xF5),
                accent_foreground: rgb(0x18, 0x18, 0x1B),
                destructive: rgb(0xEF, 0x44, 0x44),
                destructive_foreground: rgb(0xFA, 0xFA, 0xFA),
                success: rgb(0x22, 0xC5, 0x5E),
                warning: rgb(0xF5, 0x9E, 0x0B),
                border: rgb(0xE4, 0xE4, 0xE7),
                input: rgb(0xE4, 0xE4, 0xE7),
                ring: rgb(0xA1, 0xA1, 0xAA),
            },
            false,
        )
    }

    /// The default dark theme — Default design, dark palette.
    pub fn dark() -> Self {
        Self::seed(
            &Colors {
                background: rgb(0x0A, 0x0A, 0x0B),
                foreground: rgb(0xFA, 0xFA, 0xFA),
                card: rgb(0x14, 0x14, 0x16),
                card_foreground: rgb(0xFA, 0xFA, 0xFA),
                popover: rgb(0x14, 0x14, 0x16),
                popover_foreground: rgb(0xFA, 0xFA, 0xFA),
                primary: rgb(0xFA, 0xFA, 0xFA),
                primary_foreground: rgb(0x18, 0x18, 0x1B),
                secondary: rgb(0x27, 0x27, 0x2A),
                secondary_foreground: rgb(0xFA, 0xFA, 0xFA),
                muted: rgb(0x27, 0x27, 0x2A),
                muted_foreground: rgb(0xA1, 0xA1, 0xAA),
                accent: rgb(0x27, 0x27, 0x2A),
                accent_foreground: rgb(0xFA, 0xFA, 0xFA),
                destructive: rgb(0x7F, 0x1D, 0x1D),
                destructive_foreground: rgb(0xFA, 0xFA, 0xFA),
                success: rgb(0x16, 0xA3, 0x4A),
                warning: rgb(0xD9, 0x77, 0x06),
                border: rgb(0x27, 0x27, 0x2A),
                input: rgb(0x27, 0x27, 0x2A),
                ring: rgb(0x52, 0x52, 0x5B),
            },
            true,
        )
    }

    /// Build a theme from `colors` + `dark`, seeding shape/density/elevation from
    /// the **Default** base (the default design language).
    fn seed(colors: &Colors, dark: bool) -> Self {
        let design = DesignLanguage::Default;
        let t = design.tokens();
        Theme {
            colors: *colors,
            design,
            radius: t.radius,
            spacing: t.spacing,
            control_pad_x: t.control_pad_x,
            control_pad_y: t.control_pad_y,
            control_height: t.control_height,
            border_width: t.border_width,
            elevation: t.elevation,
            density: t.density,
            font_size: t.font_size,
            sizes: t.sizes,
            dark,
        }
    }

    /// Switch the base [`DesignLanguage`], re-seeding the shape / density /
    /// elevation / size-scale / font-size tokens to that language's defaults. **Colors are
    /// preserved**, and any field you set *after* this wins — so the flow is
    /// "pick a base, then customize":
    ///
    /// ```no_run
    /// use pebbles_foundation::Color;
    /// use pebbles_widgets::theme::{DesignLanguage, Theme};
    ///
    /// // Material base, but with the app's own radius and palette.
    /// let brand_blue = Color::from_rgba8(37, 99, 235, 255);
    /// let mut t = Theme::light().design(DesignLanguage::Material);
    /// t.radius = 10.0;
    /// t.colors.primary = brand_blue;
    /// t.make_current();
    /// ```
    pub fn design(mut self, lang: DesignLanguage) -> Self {
        self.design = lang;
        let t = lang.tokens();
        self.radius = t.radius;
        self.spacing = t.spacing;
        self.control_pad_x = t.control_pad_x;
        self.control_pad_y = t.control_pad_y;
        self.control_height = t.control_height;
        self.border_width = t.border_width;
        self.elevation = t.elevation;
        self.density = t.density;
        self.font_size = t.font_size;
        self.sizes = t.sizes;
        self
    }

    /// The [`ControlMetrics`] of `size` in this theme's [`SizeScale`]
    /// (`Custom` passes through).
    pub fn size(&self, size: ControlSize) -> ControlMetrics {
        match size {
            ControlSize::Xs => self.sizes.xs,
            ControlSize::Sm => self.sizes.sm,
            ControlSize::Md => self.sizes.md,
            ControlSize::Lg => self.sizes.lg,
            ControlSize::Custom(m) => m,
        }
    }

    /// Light theme on the **Tailwind** base — rounded and roomy (the original look).
    pub fn tailwind() -> Self {
        Self::light().design(DesignLanguage::Tailwind)
    }
    /// Light theme on the **Material 3** base — roomy, elevated, mobile-friendly.
    pub fn material() -> Self {
        Self::light().design(DesignLanguage::Material)
    }

    /// A tighter radius for small / nested elements (chips, menu items, key caps):
    /// `radius − 4`, floored at 0. Derived so a design switch scales them with the
    /// base (for the Tailwind base this is 4, matching the original look).
    pub fn radius_sm(&self) -> f64 {
        (self.radius - 4.0).max(0.0)
    }
    /// A slightly tighter radius for medium surfaces (cards, list rows, toasts):
    /// `radius − 2`, floored at 0. (Tailwind base ⇒ 6, matching the original.)
    pub fn radius_md(&self) -> f64 {
        (self.radius - 2.0).max(0.0)
    }

    /// The corner radius for a **pill-style** element (badge, chip, tag, progress
    /// bar): fully rounded in Tailwind and Material, but **square in the Default design** — a
    /// desktop/productivity UI uses rectangular tags and square-ended bars, not
    /// pills. (Genuinely *circular* shapes — a switch thumb, an avatar, a radio dot
    /// — stay round in every design; use `999.0` / `.circular()` for those.)
    pub fn pill(&self) -> f64 {
        match self.design {
            DesignLanguage::Default => self.radius,
            DesignLanguage::Material | DesignLanguage::Tailwind => 999.0,
        }
    }

    /// Symmetric control padding scaled by [`density`](Self::density) — pass the
    /// Tailwind-base values and they tighten (Default) or loosen (Material) with
    /// the design language.
    pub fn pad(&self, x: f64, y: f64) -> EdgeInsets {
        EdgeInsets::symmetric(x * self.density, y * self.density)
    }
    /// Uniform control padding scaled by [`density`](Self::density).
    pub fn pad_all(&self, v: f64) -> EdgeInsets {
        EdgeInsets::all(v * self.density)
    }

    /// A soft drop shadow for elevation `level` (0 or negative ⇒ `None`) —
    /// Material-style. Components use this to elevate surfaces per the active
    /// design language; `level` defaults to [`elevation`](Self::elevation).
    pub fn elevation_shadow(&self, level: f64) -> Option<BoxShadow> {
        if level <= 0.0 {
            return None;
        }
        // Ambient soft shadow: blur grows with elevation, alpha stays gentle.
        let alpha = (0.10 + level * 0.03).min(0.30);
        let a = (alpha * 255.0) as u8;
        Some(BoxShadow::new(Color::from_rgba8(0, 0, 0, a), Offset::new(0.0, level), level * 3.0, 0.0))
    }

    /// Install this theme as the current theme. Reactive: every component that read
    /// [`theme`] re-renders. Equivalent to [`set_theme`].
    pub fn make_current(self) {
        set_theme(self);
    }
}

/// Which brightness the app uses — Flutter's `ThemeMode`. The default, [`System`](Self::System),
/// follows the platform's light/dark setting (falling back to light when the platform reports
/// none); `Light` / `Dark` pin it regardless of the platform.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    /// Follow the platform brightness ([`set_platform_brightness`]); light if unknown.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// A platform light/dark preference (Flutter's `Brightness`, CSS `prefers-color-scheme`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Brightness {
    Light,
    Dark,
}

impl ThemeMode {
    /// Whether this mode is dark given the platform brightness (`None` = unknown ⇒ light,
    /// as Flutter's `platformBrightness` defaults to light).
    pub fn resolve(self, platform: Option<Brightness>) -> bool {
        match self {
            ThemeMode::Light => false,
            ThemeMode::Dark => true,
            ThemeMode::System => platform == Some(Brightness::Dark),
        }
    }
}

thread_local! {
    /// The global theme signal, created lazily at app scope by [`theme_signal`].
    static CURRENT: RefCell<Option<Signal<Theme>>> = const { RefCell::new(None) };
    /// The global [`ThemeMode`] signal (lazily, like `CURRENT`).
    static MODE: RefCell<Option<Signal<ThemeMode>>> = const { RefCell::new(None) };
    /// The last platform brightness the shell reported (`None` = unknown / unsupported).
    static PLATFORM: std::cell::Cell<Option<Brightness>> = const { std::cell::Cell::new(None) };
}

fn mode_signal() -> Signal<ThemeMode> {
    MODE.with(|cell| *cell.borrow_mut().get_or_insert_with(|| create_root_signal(ThemeMode::System)))
}

/// The active [`ThemeMode`] (reactive read).
pub fn theme_mode() -> ThemeMode {
    mode_signal().get()
}

/// The last platform brightness reported by the shell (`None` if the platform reports none).
pub fn platform_brightness() -> Option<Brightness> {
    PLATFORM.with(std::cell::Cell::get)
}

/// Switch the [`ThemeMode`]. Re-seeds the global theme's colors from the light/dark palette
/// when the resolved brightness changes (keeping the [`DesignLanguage`], like [`toggle_theme`]).
pub fn set_theme_mode(mode: ThemeMode) {
    mode_signal().set(mode);
    apply_brightness(mode.resolve(platform_brightness()));
}

/// Report the platform brightness (the shell calls this at startup and whenever the OS
/// setting changes; `None` = the platform does not expose one). Only affects the theme in
/// [`ThemeMode::System`].
pub fn set_platform_brightness(b: Option<Brightness>) {
    PLATFORM.with(|c| c.set(b));
    let mode = mode_signal().peek();
    if mode == ThemeMode::System {
        apply_brightness(mode.resolve(b));
    }
}

/// Swap the global theme to the light/dark palette matching `dark`, if it does not already.
fn apply_brightness(dark: bool) {
    let cur = theme_signal().peek();
    if cur.dark != dark {
        theme_signal().set(if dark { Theme::dark() } else { Theme::light() }.design(cur.design));
    }
}

/// Create the global theme signal. Call once at startup (before the tree runs) so the
/// signal is app-owned, not owned by whatever component renders first — mirrors
/// `overlay::init`/`focus::init`. The shell calls this automatically.
pub fn init() {
    let _ = theme_signal();
}

/// The global theme signal (reactive).
fn theme_signal() -> Signal<Theme> {
    CURRENT.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_root_signal(Theme::light()));
        }
        cell.unwrap()
    })
}

/// The current theme (cheap `Copy`). Reading it inside a component subscribes that
/// component, so it re-renders when the theme changes — and, inside a
/// [`theme_override`] subtree, returns that subtree's overridden theme instead of
/// the global one.
pub fn theme() -> Theme {
    let global = theme_signal().get(); // subscribe so a global toggle re-renders too
    pebbles_core::reactive::consume_context::<Theme>().unwrap_or(global)
}

/// Swap the current theme. Every component that read [`theme`] re-renders. This pins an
/// explicit [`ThemeMode`] (`Dark` if `theme.dark`, else `Light`), so a later platform
/// brightness change no longer overrides it; call [`set_theme_mode`]`(ThemeMode::System)` to
/// follow the platform again.
pub fn set_theme(theme: Theme) {
    let mode = if theme.dark { ThemeMode::Dark } else { ThemeMode::Light };
    if mode_signal().peek() != mode {
        mode_signal().set(mode);
    }
    theme_signal().set(theme);
}

/// **The one config a developer sets to choose their base look.** Switches the whole
/// app's [`DesignLanguage`] — every component re-skins its radius / padding / height
/// / border / elevation to match — while keeping the current colors. Call it once at
/// startup (`set_design(DesignLanguage::Default)`) and build from there, or change it
/// live behind a setting.
pub fn set_design(lang: DesignLanguage) {
    // Read the GLOBAL theme (`peek` — this is a setter, not a reactive read), never
    // `theme()`: inside a `theme_override` subtree `theme()` returns that subtree's private
    // theme, and writing it back would promote the override to the whole app. `.design(lang)`
    // keeps the global theme's colors and only swaps the design language.
    // Writes the signal directly (not `set_theme`): a design switch keeps the ThemeMode.
    theme_signal().set(theme_signal().peek().design(lang));
}

/// The active base [`DesignLanguage`] (reactive read).
pub fn design() -> DesignLanguage {
    theme().design
}

/// Flip the whole app between light and dark, **keeping the active [`DesignLanguage`]**.
/// Colors re-seed from the base light/dark palette, so app-specific color customizations are
/// reset on the flip (there is no 1:1 light↔dark mapping for a custom color) — re-apply them
/// after toggling if they must persist across modes. Pins an explicit [`ThemeMode`].
pub fn toggle_theme() {
    // Global theme (`peek`), never a `theme_override` subtree — see `set_design`.
    let cur = theme_signal().peek();
    let next = if cur.dark { Theme::light() } else { Theme::dark() }.design(cur.design);
    set_theme(next);
}

// ---------------------------------------------------------------------------
// Scoped theme override
// ---------------------------------------------------------------------------

/// Override the theme for `child`'s **whole subtree**: every component rendered
/// inside it reads `t` from [`theme`] instead of the global theme, while the rest
/// of the app is unaffected. Inner overrides shadow outer ones. The rendering
/// context is provided via [`pebbles_core::reactive::provide_context`], so the
/// override is visible exactly while the subtree reconciles.
pub fn theme_override(t: Theme, child: impl pebbles_core::IntoWidget) -> pebbles_core::Element {
    #[derive(Clone)]
    struct Props {
        theme: Theme,
        child: pebbles_core::widget::AnyWidget,
    }
    fn render(props: &Props) -> pebbles_core::Element {
        pebbles_core::reactive::provide_context(props.theme);
        props.child.clone()
    }
    pebbles_core::component_props(render, Props { theme: t, child: child.into_widget() }).into_widget()
}

/// Scale a color's RGB channels by `factor` (`<1.0` darkens, `>1.0` lightens),
/// keeping alpha. Used for hover/pressed state shading.
pub fn shade(color: Color, factor: f32) -> Color {
    let [r, g, b, a] = color.components;
    Color::new([(r * factor).clamp(0.0, 1.0), (g * factor).clamp(0.0, 1.0), (b * factor).clamp(0.0, 1.0), a])
}

/// Linearly interpolate between two colors (`t` in `0.0..=1.0`). Used for hover/
/// pressed overlays that stay visible on both light and dark surfaces.
pub fn mix(a: Color, b: Color, t: f32) -> Color {
    let [ar, ag, ab, aa] = a.components;
    let [br, bg, bb, ba] = b.components;
    let l = |x: f32, y: f32| x + (y - x) * t;
    Color::new([l(ar, br), l(ag, bg), l(ab, bb), l(aa, ba)])
}

impl Default for Theme {
    /// The light theme on the default design language.
    fn default() -> Self {
        Self::light()
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;

    fn reset() {
        set_theme_mode(ThemeMode::System);
        set_platform_brightness(None);
        set_theme(Theme::light());
        set_theme_mode(ThemeMode::System);
    }

    #[test]
    fn resolve_table() {
        assert!(!ThemeMode::System.resolve(None), "unknown platform falls back to light");
        assert!(!ThemeMode::System.resolve(Some(Brightness::Light)));
        assert!(ThemeMode::System.resolve(Some(Brightness::Dark)));
        assert!(ThemeMode::Dark.resolve(Some(Brightness::Light)));
        assert!(!ThemeMode::Light.resolve(Some(Brightness::Dark)));
        assert_eq!(ThemeMode::default(), ThemeMode::System);
    }

    #[test]
    fn system_follows_platform_and_keeps_design() {
        reset();
        set_design(DesignLanguage::Material);
        set_platform_brightness(Some(Brightness::Dark));
        assert!(theme_signal().peek().dark);
        assert_eq!(theme_signal().peek().design, DesignLanguage::Material);
        set_platform_brightness(Some(Brightness::Light));
        assert!(!theme_signal().peek().dark);
        set_platform_brightness(None);
        assert!(!theme_signal().peek().dark);
        assert_eq!(theme_mode(), ThemeMode::System);
    }

    #[test]
    fn explicit_modes_ignore_platform() {
        reset();
        set_theme_mode(ThemeMode::Dark);
        assert!(theme_signal().peek().dark);
        set_platform_brightness(Some(Brightness::Light));
        assert!(theme_signal().peek().dark, "Dark mode is pinned");
        set_theme_mode(ThemeMode::System);
        assert!(!theme_signal().peek().dark, "System re-resolves from the stored platform value");
    }

    #[test]
    fn set_theme_and_toggle_pin_an_explicit_mode() {
        reset();
        set_theme(Theme::dark());
        assert_eq!(theme_mode(), ThemeMode::Dark);
        set_platform_brightness(Some(Brightness::Light));
        assert!(theme_signal().peek().dark);
        toggle_theme();
        assert_eq!(theme_mode(), ThemeMode::Light);
        assert!(!theme_signal().peek().dark);
        set_platform_brightness(Some(Brightness::Dark));
        assert!(!theme_signal().peek().dark);
    }

    #[test]
    fn system_keeps_custom_theme_when_brightness_matches() {
        reset();
        let mut t = Theme::light();
        t.radius = 3.0;
        set_theme(t);
        set_theme_mode(ThemeMode::System);
        set_platform_brightness(Some(Brightness::Light));
        assert_eq!(theme_signal().peek().radius, 3.0, "no re-seed when the brightness already matches");
    }
}
