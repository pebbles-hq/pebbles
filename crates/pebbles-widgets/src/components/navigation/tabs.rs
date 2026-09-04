//! [`Tabs`] — a tab bar plus the selected tab's content. Controlled: `selected` is a
//! prop and each tab reports selection through an `on_select` callback. The strip is
//! keyboard-navigable (focus it, then Left/Right cycle tabs) and the content area
//! cross-fades when the selection changes.

use std::rc::Rc;

use pebbles_foundation::{CrossAxisAlignment, EdgeInsets, MainAxisSize, palette};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor};

use crate::theme::{mix, theme};
use crate::widgets::{
    Container, GestureDetector, Opacity, Padding, Positioned, column, gap_h, row, stack, text,
};
use pebbles_core::context::Callback;
use pebbles_core::focus::create_focus;
use pebbles_core::keyboard::{KeyInput, Motion};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{animate_to, animated, children, component_props, create_signal};

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
    content_padding: EdgeInsets,
    tab_padding: EdgeInsets,
    style: Option<crate::style::Style>,
}

/// Create a [`Tabs`] with the given selected index.
pub fn tabs(selected: usize) -> Tabs {
    Tabs {
        selected,
        content_padding: EdgeInsets::symmetric(0.0, 16.0),
        tab_padding: EdgeInsets::symmetric(14.0, 8.0),
        ..Default::default()
    }
}

impl Tabs {
    /// Add a tab with a label, content and a selection callback.
    pub fn tab(
        mut self,
        label: impl Into<String>,
        content: impl IntoWidget,
        on_select: impl pebbles_core::IntoCallback,
    ) -> Self {
        self.tabs.push(TabDef {
            label: label.into(),
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
    /// The padding around the content area (default `(0, 16)`).
    pub fn content_padding(mut self, insets: EdgeInsets) -> Self {
        self.content_padding = insets;
        self
    }
    /// The padding inside each tab button (default `(14, 8)`).
    pub fn tab_padding(mut self, insets: EdgeInsets) -> Self {
        self.tab_padding = insets;
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
    let node = create_focus();
    let n = p.tabs.len();
    let selected = p.selected;
    let merged =
        crate::style::style().background(th.colors.background).merge(p.style.clone().unwrap_or_default());
    let label_color = merged.color.unwrap_or(th.colors.foreground);
    let label_size = merged.font_size.unwrap_or(14.0);
    let label_weight = merged.font_weight.unwrap_or(500.0);
    let active_color = p.active_color.unwrap_or(th.colors.primary);

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
    node.register_editor(Rc::new(move |k: KeyInput| {
        if n == 0 {
            return;
        }
        let step = match k {
            KeyInput::Move { motion, .. } => match motion {
                Motion::Right | Motion::Down => 1i64,
                Motion::Left | Motion::Up => -1i64,
                _ => return,
            },
            _ => return,
        };
        for offset in 1..=n {
            let i = ((selected as i64 + step * offset as i64).rem_euclid(n as i64)) as usize;
            if let Some(f) = &selectors[i] {
                f();
                break;
            }
        }
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
                    selected: i == p.selected,
                    disabled: tab.disabled,
                    variant: p.variant,
                    on_tap: tab.on_select.clone(),
                    color: label_color,
                    size: label_size,
                    weight: label_weight,
                    active_color,
                    tab_padding: p.tab_padding,
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
            let mut deco = m.decoration().unwrap_or_else(BoxDecoration::new);
            if node.is_focused() {
                deco = deco.border(Border::new(th.colors.ring, 2.0));
            }
            (deco, None)
        }
        TabsVariant::Pills => {
            let base = crate::style::style().background(th.colors.background);
            let m = base.merge(p.style.clone().unwrap_or_default());
            let mut deco = m.decoration().unwrap_or_else(BoxDecoration::new);
            if node.is_focused() {
                deco = deco.border(Border::new(th.colors.ring, 2.0));
            }
            (deco, None)
        }
        TabsVariant::Solid => {
            let base = crate::style::style().background(th.colors.muted).radius_all(th.radius);
            let m = base.merge(p.style.clone().unwrap_or_default());
            let mut deco = m.decoration().unwrap_or_else(BoxDecoration::new);
            if node.is_focused() {
                deco = deco.border(Border::new(th.colors.ring, 2.0));
            }
            (deco, Some(EdgeInsets::all(4.0)))
        }
    };
    let mut strip = Container::new().decoration(strip_deco).child(row(bar).main_axis_size(MainAxisSize::Min));
    if let Some(pad) = strip_pad {
        strip = strip.padding(pad);
    }
    // C7: the strip is a TabList (its buttons are Tabs — see render_tab_button).
    let strip = crate::widgets::semantics(pebbles_render::SemanticsRole::TabList, "", strip).into_widget();

    // --- content (cross-faded on switch) ------------------------------------
    let content = selected_content
        .map(|w| {
            component_props(render_fade_swap, FadeSwapProps { index: selected, content: w }).into_widget()
        })
        .unwrap_or_else(|| gap_h(0.0).into_widget());

    let mut body = vec![strip];
    body.push(Padding::new(p.content_padding, content).into_widget());

    column(body)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .into_widget()
}

/// Props for one tab button in the strip.
struct TabButtonProps {
    label: String,
    selected: bool,
    disabled: bool,
    variant: TabsVariant,
    on_tap: Option<Callback>,
    color: pebbles_foundation::Color,
    size: f32,
    weight: f32,
    active_color: pebbles_foundation::Color,
    tab_padding: EdgeInsets,
}

/// One strip button: underline or pill, hover feedback, pointer cursor, tap
/// callback, and pointer-down focuses the strip for keyboard navigation.
fn render_tab_button(p: &TabButtonProps) -> AnyWidget {
    let c = theme().colors;
    let hovered = create_signal(false);
    let hv = if p.disabled { 0.0 } else { animated(if hovered.get() { 1.0 } else { 0.0 }, 0.12) };
    let label_color = if p.selected { p.color } else { mix(c.muted_foreground, p.color, 0.3 * hv as f32) };

    let cell: AnyWidget = match p.variant {
        TabsVariant::Underline => {
            // The 2px accent line sits at the very bottom of the cell — on top of
            // the strip's hairline border (the shadcn overlap).
            // The Google-style indicator: a 3px rounded bar at the very bottom,
            // spanning the tab's FULL width (the Stack + Positioned fill gives it
            // the label's width — a bare Container would collapse to 0 wide under
            // the row's unbounded cross constraints).
            let underline_color = if p.selected { p.active_color } else { palette::TRANSPARENT };
            let underline = Container::new()
                .decoration(BoxDecoration::new().color(underline_color).radius(BorderRadius::all(999.0)))
                .height(3.0);
            stack(children![
                Padding::new(
                    p.tab_padding,
                    text(p.label.clone()).size(p.size).weight(p.weight).color(label_color),
                ),
                Positioned::new(underline).left(0.0).right(0.0).bottom(0.0),
            ])
            .into_widget()
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
        .cursor(Cursor::Pointer)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false));
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
