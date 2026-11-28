//! [`Tabs`] — a tab bar plus the selected tab's content. Controlled: `selected` is a
//! prop and each tab reports selection through an `on_select` callback. The strip is
//! keyboard-navigable (focus it, then Left/Right cycle tabs) and the content area
//! cross-fades when the selection changes.

use std::rc::Rc;

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisSize, palette};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor};

use crate::theme::{mix, theme};
use crate::widgets::{
    Container, GestureDetector, Opacity, Padding, Positioned, column, gap_h, gap_w, row, stack, text,
};
use pebbles_core::context::Callback;
use pebbles_core::focus::create_focus;
use pebbles_core::keyboard::{KeyInput, Motion};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animate_to, animated, component_props, create_signal};

/// The visual style of a [`Tabs`] strip.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TabsVariant {
    /// The shadcn classic: a hairline under the strip, the active tab carrying a
    /// 2px accent underline on top of it (the default).
    #[default]
    Underline,
    /// Light: the active tab sits in a tinted rounded pill on a plain strip.
    Pills,
    /// Solid: a muted, rounded strip with the active tab elevated as a card
    /// fill — the classic boxed look.
    Solid,
}

#[derive(Clone)]
struct TabDef {
    label: String,
    /// A count badge after the label (`None` or 0 = none).
    badge: Option<usize>,
    content: AnyWidget,
    on_select: Option<Callback>,
    disabled: bool,
}

/// A tabbed panel.
#[derive(Clone, Default)]
pub struct Tabs {
    selected: usize,
    tabs: Vec<TabDef>,
    variant: TabsVariant,
    autofocus: bool,
    active_color: Option<pebbles_foundation::Color>,
    /// Render only the strip (no content slot): see [`tab_strip`].
    strip_only: bool,
    // `None` = the design-language default (resolved at render so it re-skins live).
    content_padding: Option<EdgeInsets>,
    tab_padding: Option<EdgeInsets>,
    style: Option<crate::style::Style>,
}

/// Create a [`Tabs`] with the given selected index.
pub fn tabs(selected: usize) -> Tabs {
    Tabs { selected, ..Default::default() }
}

/// A bare tab strip (no content area): the one tab design for every tab row — labels
/// with an optional count badge and a 2 px accent underline under the selected tab.
/// Add tabs with [`Tabs::item`].
pub fn tab_strip(selected: usize) -> Tabs {
    Tabs { selected, strip_only: true, ..Default::default() }
}

/// Height of the selected tab's accent underline.
pub const TAB_INDICATOR_H: f64 = 2.0;
/// Semantics label of the selected tab's indicator (tests and GUI checks find it by it).
pub const TAB_INDICATOR_LABEL: &str = "Tab indicator";

/// The selected-tab indicator: a 2 px rounded accent bar. Place it with
/// `Positioned::new(..).left(0).right(0).bottom(0)` over a tab so it spans the tab's
/// bottom edge; every tab design (strips, editor tabs) draws it through this.
pub fn tab_indicator(color: pebbles_foundation::Color) -> AnyWidget {
    crate::widgets::semantics(
        pebbles_render::SemanticsRole::Group,
        TAB_INDICATOR_LABEL,
        Container::new()
            .decoration(BoxDecoration::new().color(color).radius(BorderRadius::all(TAB_INDICATOR_H)))
            .height(TAB_INDICATOR_H),
    )
    .into_widget()
}

impl Tabs {
    /// Add a tab to a [`tab_strip`]: label, optional count badge, selection callback.
    pub fn item(
        mut self,
        label: impl Into<String>,
        badge: Option<usize>,
        on_select: impl pebbles_core::IntoCallback,
    ) -> Self {
        self.tabs.push(TabDef {
            label: label.into(),
            badge,
            content: gap_h(0.0).into_widget(),
            on_select: Some(on_select.into_callback()),
            disabled: false,
        });
        self
    }
    /// Add a tab with a label, content and a selection callback.
    pub fn tab(
        mut self,
        label: impl Into<String>,
        content: impl IntoWidget,
        on_select: impl pebbles_core::IntoCallback,
    ) -> Self {
        self.tabs.push(TabDef {
            label: label.into(),
            badge: None,
            content: content.into_widget(),
            on_select: Some(on_select.into_callback()),
            disabled: false,
        });
        self
    }
    /// Disable tab `index`: muted, not-allowed cursor, no callback, and keyboard
    /// navigation skips it.
    pub fn tab_disabled(mut self, index: usize) -> Self {
        if let Some(t) = self.tabs.get_mut(index) {
            t.disabled = true;
        }
        self
    }
    /// The strip style (default [`TabsVariant::Underline`]).
    pub fn variant(mut self, variant: TabsVariant) -> Self {
        self.variant = variant;
        self
    }
    /// Focus the strip on mount (Tab reaches it, then Left/Right switch tabs).
    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
    /// Merge a [`Style`](crate::Style) over the strip: box props (background,
    /// border, radius) style the bar; text props (color, size, weight) style
    /// the tab labels.
    pub fn style(mut self, style: crate::style::Style) -> Self {
        self.style = Some(style);
        self
    }
    /// The active tab's accent — the underline color, the pill tint and the
    /// active label (defaults to the theme primary).
    pub fn active_color(mut self, color: pebbles_foundation::Color) -> Self {
        self.active_color = Some(color);
        self
    }
    /// The padding around the content area (default: the design-language `(0, 16)`).
    pub fn content_padding(mut self, insets: EdgeInsets) -> Self {
        self.content_padding = Some(insets);
        self
    }
    /// The padding inside each tab button (default: the design-language `(14, 8)`).
    pub fn tab_padding(mut self, insets: EdgeInsets) -> Self {
        self.tab_padding = Some(insets);
        self
    }
}

impl IntoWidget for Tabs {
    fn into_widget(self) -> AnyWidget {
        component_props(render_tabs, self).into_widget()
    }
}

fn render_tabs(p: &Tabs) -> AnyWidget {
    let th = theme();
    // Resolve the design-language padding defaults here (so a live design switch
    // re-skins them); an explicit `.tab_padding(..)`/`.content_padding(..)` wins.
    let tab_padding = p.tab_padding.unwrap_or_else(|| th.pad(14.0, 8.0));
    let content_padding = p.content_padding.unwrap_or_else(|| th.pad(0.0, 16.0));
    let node = create_focus();
    let n = p.tabs.len();
    let selected = p.selected;
    let merged =
        crate::style::style().background(th.colors.background).merge(p.style.clone().unwrap_or_default());
    let label_color = merged.color.unwrap_or(th.colors.foreground);
    let label_size = merged.font_size.unwrap_or(14.0);
    let label_weight = merged.font_weight.unwrap_or(500.0);
    let active_color = p.active_color.unwrap_or(th.colors.primary);
    // Unselected labels: the theme's muted foreground on the default strip. On a strip
    // the caller coloured, muted grey can vanish (grey on violet), so — like Flutter's
    // TabBar, whose unselected label is the label colour at 70 % — use the strip's text
    // colour (or black/white by the strip's brightness) at 70 %.
    let inactive_color = match p.style.as_ref().and_then(|s| s.background) {
        Some(strip) => merged
            .color
            .unwrap_or_else(|| crate::components::display::surfaces::on_color(strip))
            .with_alpha(0.7),
        None => th.colors.muted_foreground,
    };

    // Keyboard: while the strip is focused, Left/Right cycle to the next enabled
    // tab (wrapping); disabled tabs are skipped.
    node.register(Rc::new(|| {}), None, p.autofocus);
    let selectors: Vec<Option<Rc<dyn Fn()>>> = p
        .tabs
        .iter()
        .map(|t| match (&t.on_select, t.disabled) {
            (Some(Callback::Plain(f)), false) => Some(f.clone()),
            _ => None,
        })
        .collect();
    // register_keys (not register_editor): consume only the Left/Right/Up/Down it
    // handles and return false for everything else, so Ctrl shortcuts and typing fall
    // through instead of being swallowed while the strip holds focus.
    node.register_keys(Rc::new(move |k: KeyInput| {
        if n == 0 {
            return false;
        }
        let step = match k {
            KeyInput::Move { motion, .. } => match motion {
                Motion::Right | Motion::Down => 1i64,
                Motion::Left | Motion::Up => -1i64,
                _ => return false,
            },
            _ => return false,
        };
        for offset in 1..=n {
            let i = ((selected as i64 + step * offset as i64).rem_euclid(n as i64)) as usize;
            if let Some(f) = &selectors[i] {
                f();
                break;
            }
        }
        true
    }));

    // --- strip -------------------------------------------------------------
    let mut bar: Vec<AnyWidget> = Vec::new();
    let mut selected_content: Option<AnyWidget> = None;
    for (i, tab) in p.tabs.iter().enumerate() {
        if i == p.selected {
            selected_content = Some(tab.content.clone());
        }
        bar.push(
            component_props(
                render_tab_button,
                TabButtonProps {
                    label: tab.label.clone(),
                    badge: tab.badge,
                    selected: i == p.selected,
                    disabled: tab.disabled,
                    variant: p.variant,
                    on_tap: tab.on_select.clone(),
                    color: label_color,
                    inactive_color,
                    size: label_size,
                    weight: label_weight,
                    active_color,
                    solid_active_label: p.active_color.unwrap_or(th.colors.foreground),
                    tab_padding,
                },
            )
            .into_widget(),
        );
    }

    // Per-variant strip: Underline carries a hairline bottom border (the active
    // underline overlaps it); Pills is plain; Solid is a muted rounded trough.
    let (strip_deco, strip_pad): (BoxDecoration, Option<EdgeInsets>) = match p.variant {
        TabsVariant::Underline => {
            let base = crate::style::style()
                .background(th.colors.background)
                .border_bottom(pebbles_render::BorderSide::new(th.colors.border, 1.0));
            let m = base.merge(p.style.clone().unwrap_or_default());
            let mut deco = m.decoration().unwrap_or_default();
            if node.is_focused() {
                deco = deco.border(Border::new(th.colors.ring, 2.0));
            }
            // 1 px above the strip's bottom hairline (it paints over the tab cells), so the
            // whole 2 px indicator shows.
            (deco, Some(EdgeInsets::only(0.0, 0.0, 0.0, 1.0)))
        }
        TabsVariant::Pills => {
            let base = crate::style::style().background(th.colors.background);
            let m = base.merge(p.style.clone().unwrap_or_default());
            let mut deco = m.decoration().unwrap_or_default();
            if node.is_focused() {
                deco = deco.border(Border::new(th.colors.ring, 2.0));
            }
            (deco, None)
        }
        TabsVariant::Solid => {
            let base = crate::style::style().background(th.colors.muted).radius_all(th.radius);
            let m = base.merge(p.style.clone().unwrap_or_default());
            let mut deco = m.decoration().unwrap_or_default();
            if node.is_focused() {
                deco = deco.border(Border::new(th.colors.ring, 2.0));
            }
            (deco, Some(th.pad_all(4.0)))
        }
    };
    let mut strip = Container::new().decoration(strip_deco).child(row(bar).main_axis_size(MainAxisSize::Min));
    if let Some(pad) = strip_pad {
        strip = strip.padding(pad);
    }
    // C7: the strip is a TabList (its buttons are Tabs — see render_tab_button).
    let strip = crate::widgets::semantics(pebbles_render::SemanticsRole::TabList, "", strip).into_widget();

    if p.strip_only {
        return strip;
    }

    // --- content (cross-faded on switch) ------------------------------------
    let content = selected_content
        .map(|w| {
            component_props(render_fade_swap, FadeSwapProps { index: selected, content: w }).into_widget()
        })
        .unwrap_or_else(|| gap_h(0.0).into_widget());

    let mut body = vec![strip];
    body.push(Padding::new(content_padding, content).into_widget());

    column(body)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

/// Props for one tab button in the strip.
struct TabButtonProps {
    label: String,
    badge: Option<usize>,
    selected: bool,
    disabled: bool,
    variant: TabsVariant,
    on_tap: Option<Callback>,
    color: pebbles_foundation::Color,
    /// Unselected label colour (readable on the strip).
    inactive_color: pebbles_foundation::Color,
    size: f32,
    weight: f32,
    active_color: pebbles_foundation::Color,
    /// Solid: the selected label sits on the raised `background` card, not on the strip,
    /// so it takes `.active_color(..)` (else the foreground), never the strip's text color.
    solid_active_label: pebbles_foundation::Color,
    tab_padding: EdgeInsets,
}

/// One strip button: underline or pill, hover feedback, pointer cursor, tap
/// callback, and pointer-down focuses the strip for keyboard navigation.
fn render_tab_button(p: &TabButtonProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let pressed = create_signal(false);
    let hv = if p.disabled { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let label_color = if p.disabled {
        p.inactive_color.with_alpha(0.5)
    } else if p.selected {
        p.color
    } else {
        mix(p.inactive_color, p.color, 0.3 * hv as f32)
    };

    let cell: AnyWidget = match p.variant {
        TabsVariant::Underline => {
            // The indicator is a 2 px accent bar at the tab's bottom edge spanning its
            // full width (Stack + Positioned: a bare Container would collapse to 0 wide
            // under the row's unbounded cross constraints), over the strip's hairline.
            // Hover / pressed tint the whole tab; selected text keeps full contrast.
            let tint = if p.disabled || !(hovered.get() || pressed.get()) {
                palette::TRANSPARENT
            } else {
                c.foreground.with_alpha(if pressed.get() { 0.12 } else { 0.06 })
            };
            let mut inner: Vec<AnyWidget> =
                vec![text(p.label.clone()).size(p.size).weight(p.weight).color(label_color).into_widget()];
            if let Some(n) = p.badge.filter(|n| *n > 0) {
                inner.push(gap_w(5.0).into_widget());
                inner.push(
                    Container::new()
                        .padding(EdgeInsets::symmetric(5.0, 0.0))
                        .decoration(
                            BoxDecoration::new()
                                .color(c.muted.with_alpha(0.6))
                                .radius(BorderRadius::all(8.0)),
                        )
                        .child(text(n.to_string()).size((p.size - 2.0).max(9.0)).color(c.muted_foreground))
                        .into_widget(),
                );
            }
            let mut layers: Vec<AnyWidget> = vec![
                Container::new()
                    .decoration(BoxDecoration::new().color(tint).radius(BorderRadius::all(4.0)))
                    .child(Padding::new(
                        p.tab_padding,
                        row(inner)
                            .main_axis_size(MainAxisSize::Min)
                            .cross_axis_alignment(CrossAxisAlignment::Center),
                    ))
                    .into_widget(),
            ];
            if p.selected {
                layers.push(
                    Positioned::new(tab_indicator(p.active_color))
                        .left(0.0)
                        .right(0.0)
                        .bottom(0.0)
                        .into_widget(),
                );
            }
            stack(layers).into_widget()
        }
        TabsVariant::Pills => {
            let bg = if p.selected { mix(c.background, p.active_color, 0.14) } else { palette::TRANSPARENT };
            Container::new()
                .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(999.0)))
                .child(Padding::new(
                    p.tab_padding,
                    text(p.label.clone()).size(p.size).weight(p.weight).color(label_color),
                ))
                .into_widget()
        }
        TabsVariant::Solid => {
            let bg = if p.selected { c.background } else { palette::TRANSPARENT };
            let label_color = if p.selected { p.solid_active_label } else { label_color };
            Container::new()
                .decoration(BoxDecoration::new().color(bg).radius(BorderRadius::all(theme().radius)))
                .child(Padding::new(
                    p.tab_padding,
                    text(p.label.clone()).size(p.size).weight(p.weight).color(label_color),
                ))
                .into_widget()
        }
    };

    if p.disabled {
        // C7: a disabled Tab.
        return crate::widgets::semantics(
            pebbles_render::SemanticsRole::Tab,
            p.label.clone(),
            GestureDetector::new(cell).cursor(Cursor::NotAllowed),
        )
        .disabled(true)
        .into_widget();
    }
    let mut g = GestureDetector::new(cell)
        .cursor(crate::design::theme::action_cursor())
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || {
            hovered.set(false);
            pressed.set(false);
        })
        .on_pointer_down(move || pressed.set(true))
        .on_pointer_up(move || pressed.set(false));
    // NOTE: clicking does NOT focus the strip — the focus ring is keyboard-only
    // (Tab reaches the strip, then Left/Right navigate). No ring flash on click.
    if let Some(cb) = p.on_tap.clone() {
        g = g.on_tap(cb);
    }
    // C7: each strip button is a Tab; the selected one carries value "selected".
    let node = crate::widgets::semantics(pebbles_render::SemanticsRole::Tab, p.label.clone(), g);
    let node = if p.selected { node.value("selected") } else { node };
    node.into_widget()
}

/// Props for the cross-fading content slot.
struct FadeSwapProps {
    index: usize,
    content: AnyWidget,
}

/// Fades the content in from transparent whenever `index` changes (tab switches).
fn render_fade_swap(p: &FadeSwapProps) -> AnyWidget {
    let last = create_signal(usize::MAX);
    let t = create_signal(1.0f64);
    if last.get() != p.index {
        last.set(p.index);
        t.set(0.0);
        animate_to(t, 1.0, 0.15);
    }
    Opacity::new(t.get() as f32, p.content.clone()).into_widget()
}
