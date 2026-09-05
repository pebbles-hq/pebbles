//! Surface & display components: [`Card`], [`Badge`], [`Alert`], [`Avatar`],
//! [`Separator`] and [`Skeleton`].

use pebbles_foundation::{Alignment, Color, CrossAxisAlignment, EdgeInsets, MainAxisSize, Offset};
use pebbles_render::{Border, BorderRadius, BorderSide, BoxDecoration, BoxShadow, IconData, IconKind};

#[cfg(feature = "image-view")]
use crate::ImageView;
use crate::style::{Style, style, styled};
use crate::theme::{mix, theme};
use crate::widgets::{
    ClipRRect, Container, Positioned, center, column, gap_h, gap_w, row, spacer, stack, text,
};
use pebbles_core::children;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{component_props, create_loop};

use crate::components::icon;

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

/// An elevated content surface with border, radius, shadow and padding. Mirrors
/// shadcn's `Card`: an optional header (title / description / trailing action), a
/// content body, and an optional footer.
#[derive(Clone, Default)]
pub struct Card {
    content: Option<AnyWidget>,
    title: Option<String>,
    description: Option<String>,
    action: Option<AnyWidget>,
    footer: Option<AnyWidget>,
    padding: EdgeInsets,
    style: Option<Style>,
}

/// Create an empty [`Card`] and compose it with the builder methods.
pub fn card() -> Card {
    Card { padding: EdgeInsets::all(16.0), ..Default::default() }
}

impl Card {
    /// The main content body.
    pub fn child(mut self, child: impl IntoWidget) -> Self {
        self.content = Some(child.into_widget());
        self
    }
    /// A header title.
    pub fn title(mut self, s: impl Into<String>) -> Self {
        self.title = Some(s.into());
        self
    }
    /// A muted header description under the title.
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = Some(s.into());
        self
    }
    /// A widget pinned to the top-right of the header (e.g. a menu button).
    pub fn action(mut self, w: impl IntoWidget) -> Self {
        self.action = Some(w.into_widget());
        self
    }
    /// A footer row under the content (e.g. actions).
    pub fn footer(mut self, w: impl IntoWidget) -> Self {
        self.footer = Some(w.into_widget());
        self
    }
    pub fn padding(mut self, insets: EdgeInsets) -> Self {
        self.padding = insets;
        self
    }
    /// Override the surface presentation (bg / border / radius / shadow / size …) by
    /// merging a [`Style`] onto the card's base — user fields win.
    pub fn style(mut self, s: Style) -> Self {
        self.style = Some(s);
        self
    }
}

impl IntoWidget for Card {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let has_header = self.title.is_some() || self.description.is_some() || self.action.is_some();

        let mut kids: Vec<AnyWidget> = Vec::new();
        if has_header {
            let mut head_texts: Vec<AnyWidget> = Vec::new();
            if let Some(t) = self.title.take() {
                head_texts.push(text(t).size(16.0).semibold().color(c.foreground).into_widget());
            }
            if let Some(d) = self.description.take() {
                if !head_texts.is_empty() {
                    head_texts.push(gap_h(4.0).into_widget());
                }
                head_texts.push(text(d).size(13.5).line_height(1.4).color(c.muted_foreground).into_widget());
            }
            let head_col = column(head_texts)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min);
            let header: AnyWidget = match self.action.take() {
                Some(action) => row(children![head_col.into_widget(), spacer(), action])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .into_widget(),
                None => head_col.into_widget(),
            };
            kids.push(header);
        }
        if let Some(content) = self.content.take() {
            if has_header {
                kids.push(gap_h(14.0).into_widget());
            }
            kids.push(content);
        }
        if let Some(footer) = self.footer.take() {
            kids.push(gap_h(16.0).into_widget());
            kids.push(footer);
        }

        let body: AnyWidget = if kids.len() == 1 {
            kids.pop().unwrap()
        } else {
            column(kids)
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min)
                .into_widget()
        };

        // Base presentation as a Style; the user's `.style(..)` merges on top (wins).
        let base = style()
            .background(c.card)
            .border(Border::new(c.border, 1.0))
            .radius_all(theme().radius + 4.0)
            .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 18), Offset::new(0.0, 2.0), 8.0, 0.0))
            .padding(self.padding);
        styled(body, base.merge(self.style.take().unwrap_or_default()))
    }
}

// ---------------------------------------------------------------------------
// Badge
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum BadgeVariant {
    #[default]
    Default,
    Secondary,
    Destructive,
    Outline,
    Success,
}

/// A small status pill.
#[derive(Clone, Default)]
pub struct Badge {
    label: String,
    variant: BadgeVariant,
    style: Option<Style>,
}

/// Create a [`Badge`].
pub fn badge(label: impl Into<String>) -> Badge {
    Badge { label: label.into(), ..Default::default() }
}

impl Badge {
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }
    /// Merge a [`Style`] onto the pill's base presentation (user fields win).
    pub fn style(mut self, s: Style) -> Self {
        self.style = Some(s);
        self
    }
}

impl IntoWidget for Badge {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let (bg, fg, border) = match self.variant {
            BadgeVariant::Default => (Some(c.primary), c.primary_foreground, false),
            BadgeVariant::Secondary => (Some(c.secondary), c.secondary_foreground, false),
            BadgeVariant::Destructive => (Some(c.destructive), c.destructive_foreground, false),
            BadgeVariant::Success => (Some(c.success), Color::WHITE, false),
            BadgeVariant::Outline => (None, c.foreground, true),
        };
        let mut base = style().radius_all(999.0).padding_xy(10.0, 3.0);
        if let Some(bg) = bg {
            base = base.background(bg);
        }
        if border {
            base = base.border(Border::new(c.border, 1.0));
        }
        let label = text(std::mem::take(&mut self.label)).size(12.0).weight(500.0).color(fg);
        styled(label, base.merge(self.style.take().unwrap_or_default()))
    }
}

// ---------------------------------------------------------------------------
// Alert
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AlertVariant {
    #[default]
    Info,
    Success,
    Warning,
    Destructive,
}

/// A callout with an icon, title and description.
#[derive(Clone, Default)]
pub struct Alert {
    title: String,
    description: String,
    variant: AlertVariant,
    style: Option<Style>,
}

/// Create an [`Alert`] with a title; add a body line with [`description`](Alert::description).
pub fn alert(title: impl Into<String>) -> Alert {
    Alert { title: title.into(), ..Default::default() }
}

impl Alert {
    /// A muted body line under the title (omitted when unset).
    pub fn description(mut self, s: impl Into<String>) -> Self {
        self.description = s.into();
        self
    }
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }
    /// Merge a [`Style`] onto the callout's base surface (user fields win).
    pub fn style(mut self, s: Style) -> Self {
        self.style = Some(s);
        self
    }
}

impl IntoWidget for Alert {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let (accent, kind) = match self.variant {
            AlertVariant::Info => (c.foreground, IconKind::Info),
            AlertVariant::Success => (c.success, IconKind::Check),
            AlertVariant::Warning => (c.warning, IconKind::Warning),
            AlertVariant::Destructive => (c.destructive, IconKind::Warning),
        };
        let base = style()
            .background(c.card)
            .border(Border::new(c.border, 1.0))
            .radius_all(theme().radius)
            .padding_all(14.0);
        let mut texts: Vec<AnyWidget> = vec![
            text(std::mem::take(&mut self.title)).size(14.0).semibold().color(c.foreground).into_widget(),
        ];
        if !self.description.is_empty() {
            texts.push(gap_h(2.0).into_widget());
            texts.push(
                text(std::mem::take(&mut self.description))
                    .size(13.0)
                    .color(c.muted_foreground)
                    .into_widget(),
            );
        }
        let body = row(children![
            icon(kind).size(18.0).color(accent),
            gap_w(12.0),
            column(texts).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_size(MainAxisSize::Min),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min);
        styled(body, base.merge(self.style.take().unwrap_or_default()))
    }
}

// ---------------------------------------------------------------------------
// Banner (MaterialBanner)
// ---------------------------------------------------------------------------

/// A full-width message bar with an optional leading icon and trailing actions, and a
/// bottom divider — Flutter's `MaterialBanner`. Persistent and inline (unlike a toast),
/// it sits at the top of content to carry a prominent message.
#[derive(Clone, Default)]
pub struct Banner {
    message: String,
    icon: Option<IconData>,
    actions: Vec<AnyWidget>,
}

/// Create a [`Banner`] with a `message`. Add a leading [`icon`](Banner::icon) and
/// trailing [`action`](Banner::action)s.
pub fn banner(message: impl Into<String>) -> Banner {
    Banner { message: message.into(), ..Default::default() }
}

impl Banner {
    /// A leading icon.
    pub fn icon(mut self, kind: impl Into<IconData>) -> Self {
        self.icon = Some(kind.into());
        self
    }
    /// A trailing action (usually a `Button`). Multiple are laid out in order.
    pub fn action(mut self, w: impl IntoWidget) -> Self {
        self.actions.push(w.into_widget());
        self
    }
}

impl IntoWidget for Banner {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let mut items: Vec<AnyWidget> = Vec::new();
        if let Some(kind) = self.icon.take() {
            items.push(icon(kind).size(18.0).color(c.foreground).into_widget());
            items.push(gap_w(12.0).into_widget());
        }
        items.push(text(std::mem::take(&mut self.message)).size(13.5).color(c.foreground).into_widget());
        items.push(spacer().into_widget());
        for (i, action) in std::mem::take(&mut self.actions).into_iter().enumerate() {
            if i > 0 {
                items.push(gap_w(6.0).into_widget());
            }
            items.push(action);
        }
        // A full-width bar with a bottom divider (a banner, not a card).
        let bottom = Border {
            top: BorderSide::new(c.border, 0.0),
            right: BorderSide::new(c.border, 0.0),
            bottom: BorderSide::new(c.border, 1.0),
            left: BorderSide::new(c.border, 0.0),
        };
        Container::new()
            .decoration(BoxDecoration::new().color(c.card).border(bottom))
            .padding(EdgeInsets::symmetric(16.0, 12.0))
            .child(row(items).cross_axis_alignment(CrossAxisAlignment::Center))
            .into_widget()
    }
}

// ---------------------------------------------------------------------------
// Avatar
// ---------------------------------------------------------------------------

/// The corner shape of an [`Avatar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AvatarShape {
    #[default]
    Circle,
    Rounded,
    Square,
}

/// An avatar showing an image (with an initials fallback) or just initials on a
/// colored background. Mirrors shadcn's `Avatar` (image + fallback), plus optional
/// shape and a status dot.
#[derive(Clone, Default)]
pub struct Avatar {
    initials: String,
    size: f64,
    color: Option<Color>,
    src: Option<String>,
    shape: AvatarShape,
    status: Option<Color>,
}

/// Create an [`Avatar`] from initials (e.g. "RS").
pub fn avatar(initials: impl Into<String>) -> Avatar {
    Avatar { initials: initials.into(), size: 40.0, ..Default::default() }
}

impl Avatar {
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }
    /// Background color behind the initials fallback.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// Load an image from a URL; the initials show while it loads / if it fails.
    pub fn src(mut self, url: impl Into<String>) -> Self {
        self.src = Some(url.into());
        self
    }
    pub fn shape(mut self, shape: AvatarShape) -> Self {
        self.shape = shape;
        self
    }
    /// A small status dot at the bottom-right (e.g. `palette::emerald::S500`).
    pub fn status(mut self, color: Color) -> Self {
        self.status = Some(color);
        self
    }

    fn radius(&self) -> f64 {
        match self.shape {
            AvatarShape::Circle => self.size / 2.0,
            AvatarShape::Rounded => self.size * 0.22,
            AvatarShape::Square => 0.0,
        }
    }
}

/// The initials-on-color face (also used as the image fallback).
fn initials_face(initials: String, bg: Color, fg: Color, size: f64, radius: f64) -> AnyWidget {
    ClipRRect::new(
        BorderRadius::all(radius),
        Container::new()
            .color(bg)
            .width(size)
            .height(size)
            .alignment(Alignment::CENTER)
            .child(center(text(initials).size((size * 0.4) as f32).semibold().color(fg))),
    )
    .into_widget()
}

impl IntoWidget for Avatar {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let bg = self.color.unwrap_or(c.secondary);
        let size = self.size;
        let radius = self.radius();
        let initials = std::mem::take(&mut self.initials);
        let mk = |init: String| initials_face(init, bg, c.secondary_foreground, size, radius);

        let face: AnyWidget = match self.src.take() {
            #[cfg(feature = "image-view")]
            Some(url) => ImageView::network(url)
                .size(size, size)
                .fit(pebbles_render::ImageFit::Cover)
                .radius(BorderRadius::all(radius))
                .placeholder(mk(initials.clone()))
                .error(mk(initials.clone()))
                .into_widget(),
            // Without the `image-view` feature a `src` URL degrades to the initials
            // face (the same thing `.error(..)` would show).
            #[cfg(not(feature = "image-view"))]
            Some(_) => mk(initials),
            None => mk(initials),
        };

        match self.status {
            None => face,
            Some(sc) => {
                let d = (size * 0.28).max(8.0);
                let dot = Container::new()
                    .decoration(
                        BoxDecoration::new()
                            .color(sc)
                            .border(Border::new(c.background, 2.0))
                            .radius(BorderRadius::all(999.0)),
                    )
                    .width(d)
                    .height(d);
                Container::new()
                    .width(size)
                    .height(size)
                    .child(stack(children![face, Positioned::new(dot).left(size - d).top(size - d),]))
                    .into_widget()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AvatarGroup
// ---------------------------------------------------------------------------

/// A row of overlapping avatars, capped with a "+N" bubble — shadcn's avatar stack.
#[derive(Clone, Default)]
pub struct AvatarGroup {
    avatars: Vec<Avatar>,
    max: Option<usize>,
    size: f64,
}

/// Create an [`AvatarGroup`] from a list of avatars.
pub fn avatar_group(avatars: Vec<Avatar>) -> AvatarGroup {
    AvatarGroup { avatars, size: 40.0, ..Default::default() }
}

impl AvatarGroup {
    /// Show at most `n` avatars; the rest collapse into a "+N" bubble.
    pub fn max(mut self, n: usize) -> Self {
        self.max = Some(n);
        self
    }
    /// The diameter each avatar is normalized to (drives the overlap).
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }
}

impl IntoWidget for AvatarGroup {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let size = self.size;
        let ring = 2.0;
        let outer = size + ring * 2.0;
        let step = outer * 0.68; // ~32% overlap

        let all = std::mem::take(&mut self.avatars);
        let total = all.len();
        let shown = self.max.map(|m| m.min(total)).unwrap_or(total);
        let overflow = total - shown;

        // Each ringed avatar sits in a background-colored circle so overlaps read.
        let ringed = |child: AnyWidget| -> AnyWidget {
            Container::new()
                .decoration(BoxDecoration::new().color(c.background).radius(BorderRadius::all(999.0)))
                .padding(EdgeInsets::all(ring))
                .child(child)
                .into_widget()
        };

        let mut items: Vec<AnyWidget> = Vec::new();
        let mut left = 0.0;
        for a in all.into_iter().take(shown) {
            let av = a.size(size).into_widget();
            items.push(Positioned::new(ringed(av)).left(left).top(0.0).into_widget());
            left += step;
        }
        if overflow > 0 {
            let bubble = initials_face(format!("+{overflow}"), c.muted, c.muted_foreground, size, size / 2.0);
            items.push(Positioned::new(ringed(bubble)).left(left).top(0.0).into_widget());
            left += step;
        }

        let width = (left - step + outer).max(outer);
        Container::new().width(width).height(outer).child(stack(items)).into_widget()
    }
}

// ---------------------------------------------------------------------------
// Separator
// ---------------------------------------------------------------------------

/// A hairline divider — shadcn's `Separator`, horizontal or vertical.
#[derive(Clone, Default)]
pub struct Separator {
    vertical: bool,
    length: Option<f64>,
    thickness: f64,
    color: Option<Color>,
}

/// A horizontal separator (fills the available width unless given a length).
pub fn separator() -> Separator {
    Separator { thickness: 1.0, ..Default::default() }
}

impl Separator {
    /// A vertical separator (give it a length or place it in a bounded row).
    pub fn vertical() -> Self {
        Separator { vertical: true, thickness: 1.0, ..Default::default() }
    }
    /// Fixed extent along the separator's run (width if horizontal, height if
    /// vertical). Omit horizontally to fill the parent.
    pub fn length(mut self, length: f64) -> Self {
        self.length = Some(length);
        self
    }
    /// Line thickness (default `1`).
    pub fn thickness(mut self, t: f64) -> Self {
        self.thickness = t;
        self
    }
    /// Custom color (defaults to the theme border).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

impl IntoWidget for Separator {
    fn into_widget(self) -> AnyWidget {
        let color = self.color.unwrap_or(theme().colors.border);
        let mut container = Container::new().color(color);
        container = if self.vertical {
            container.width(self.thickness).height(self.length.unwrap_or(20.0))
        } else {
            match self.length {
                Some(l) => container.height(self.thickness).width(l),
                None => container.height(self.thickness),
            }
        };
        container.into_widget()
    }
}

// ---------------------------------------------------------------------------
// Skeleton
// ---------------------------------------------------------------------------

/// A loading placeholder block. Static by default; call [`shimmer`](Skeleton::shimmer)
/// for an animated sweep.
pub struct Skeleton {
    width: f64,
    height: f64,
    shimmer: bool,
}

/// Create a [`Skeleton`] of the given size.
pub fn skeleton(width: f64, height: f64) -> Skeleton {
    Skeleton { width, height, shimmer: false }
}

impl Skeleton {
    /// Animate a light band sweeping across the block (shadcn's shimmer).
    pub fn shimmer(mut self) -> Self {
        self.shimmer = true;
        self
    }
}

impl IntoWidget for Skeleton {
    fn into_widget(self) -> AnyWidget {
        component_props(render_skeleton, self).into_widget()
    }
}

fn render_skeleton(s: &Skeleton) -> AnyWidget {
    let c = theme().colors;
    let radius = BorderRadius::all(6.0);
    let base = Container::new()
        .decoration(BoxDecoration::new().color(c.muted).radius(radius))
        .width(s.width)
        .height(s.height);
    if !s.shimmer {
        return base.into_widget();
    }
    // A ~40%-wide lighter band sweeps left→right, clipped to the block.
    let phase = create_loop(1.2).get();
    let band_w = s.width * 0.4;
    let x = -band_w + (s.width + band_w) * phase;
    let band = Container::new()
        .decoration(BoxDecoration::new().color(mix(c.muted, c.background, 0.6)).radius(radius))
        .width(band_w)
        .height(s.height);
    base.child(ClipRRect::new(radius, stack(children![Positioned::new(band).left(x).top(0.0)]))).into_widget()
}
