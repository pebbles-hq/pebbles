//! Surface & display components: [`Card`], [`Badge`], [`Alert`], [`Avatar`],
//! [`Separator`] and [`Skeleton`].

use pebbles_foundation::{Alignment, Color, CrossAxisAlignment, EdgeInsets, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, ImageFit, IconKind};

use pebbles_core::children;
use pebbles_core::context::BuildContext;
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};
use crate::widgets::{ClipRRect, Container, Positioned, SizedBox, center, column, row, spacer, stack, text};
use crate::ImageView;

use crate::components::icon;

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

/// An elevated content surface with border, radius, shadow and padding. Mirrors
/// shadcn's `Card`: an optional header (title / description / trailing action), a
/// content body, and an optional footer.
#[derive(Clone)]
pub struct Card {
    content: Option<AnyWidget>,
    title: Option<String>,
    description: Option<String>,
    action: Option<AnyWidget>,
    footer: Option<AnyWidget>,
    padding: EdgeInsets,
}

/// Create an empty [`Card`] and compose it with the builder methods.
pub fn card() -> Card {
    Card {
        content: None,
        title: None,
        description: None,
        action: None,
        footer: None,
        padding: EdgeInsets::all(16.0),
    }
}

impl Card {
    /// Create a card wrapping `child` directly (no header/footer).
    pub fn new(child: impl IntoWidget) -> Self {
        Card { content: Some(child.into_widget()), ..card() }
    }
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
}

pebbles_core::stateless_widget!(Card);

impl StatelessWidget for Card {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
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
                    head_texts.push(SizedBox::spacer(0.0, 4.0).into_widget());
                }
                head_texts.push(text(d).size(13.5).line_height(1.4).color(c.muted_foreground).into_widget());
            }
            let head_col =
                column(head_texts).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min();
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
                kids.push(SizedBox::spacer(0.0, 14.0).into_widget());
            }
            kids.push(content);
        }
        if let Some(footer) = self.footer.take() {
            kids.push(SizedBox::spacer(0.0, 16.0).into_widget());
            kids.push(footer);
        }

        let body: AnyWidget = if kids.len() == 1 {
            kids.pop().unwrap()
        } else {
            column(kids).cross_axis_alignment(CrossAxisAlignment::Start).main_axis_min().into_widget()
        };

        Container::new()
            .decoration(
                BoxDecoration::new()
                    .color(c.card)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(theme().radius + 4.0))
                    .shadow(BoxShadow::new(
                        Color::from_rgba8(0, 0, 0, 18),
                        Offset::new(0.0, 2.0),
                        8.0,
                        0.0,
                    )),
            )
            .padding(self.padding)
            .child(body)
            .into_widget()
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
#[derive(Clone)]
pub struct Badge {
    label: String,
    variant: BadgeVariant,
}

/// Create a [`Badge`].
pub fn badge(label: impl Into<String>) -> Badge {
    Badge { label: label.into(), variant: BadgeVariant::default() }
}

impl Badge {
    pub fn variant(mut self, variant: BadgeVariant) -> Self {
        self.variant = variant;
        self
    }
}

pebbles_core::stateless_widget!(Badge);

impl StatelessWidget for Badge {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let c = theme().colors;
        let (bg, fg, border) = match self.variant {
            BadgeVariant::Default => (Some(c.primary), c.primary_foreground, false),
            BadgeVariant::Secondary => (Some(c.secondary), c.secondary_foreground, false),
            BadgeVariant::Destructive => (Some(c.destructive), c.destructive_foreground, false),
            BadgeVariant::Success => (Some(c.success), Color::WHITE, false),
            BadgeVariant::Outline => (None, c.foreground, true),
        };
        let mut deco = BoxDecoration::new().radius(BorderRadius::all(999.0));
        if let Some(bg) = bg {
            deco = deco.color(bg);
        }
        if border {
            deco = deco.border(Border::new(c.border, 1.0));
        }
        Container::new()
            .decoration(deco)
            .padding(EdgeInsets::symmetric(10.0, 3.0))
            .child(text(std::mem::take(&mut self.label)).size(12.0).weight(500.0).color(fg))
            .into_widget()
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
#[derive(Clone)]
pub struct Alert {
    title: String,
    description: String,
    variant: AlertVariant,
}

/// Create an [`Alert`].
pub fn alert(title: impl Into<String>, description: impl Into<String>) -> Alert {
    Alert { title: title.into(), description: description.into(), variant: AlertVariant::default() }
}

impl Alert {
    pub fn variant(mut self, variant: AlertVariant) -> Self {
        self.variant = variant;
        self
    }
}

pebbles_core::stateless_widget!(Alert);

impl StatelessWidget for Alert {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let c = theme().colors;
        let (accent, kind) = match self.variant {
            AlertVariant::Info => (c.foreground, IconKind::Info),
            AlertVariant::Success => (c.success, IconKind::Check),
            AlertVariant::Warning => (c.warning, IconKind::Warning),
            AlertVariant::Destructive => (c.destructive, IconKind::Warning),
        };
        Container::new()
            .decoration(
                BoxDecoration::new()
                    .color(c.card)
                    .border(Border::new(c.border, 1.0))
                    .radius(BorderRadius::all(theme().radius)),
            )
            .padding(EdgeInsets::all(14.0))
            .child(
                row(children![
                    icon(kind).size(18.0).color(accent),
                    SizedBox::spacer(12.0, 0.0),
                    column(children![
                        text(std::mem::take(&mut self.title)).size(14.0).semibold().color(c.foreground),
                        SizedBox::spacer(0.0, 2.0),
                        text(std::mem::take(&mut self.description)).size(13.0).color(c.muted_foreground),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_min(),
                ])
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_min(),
            )
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
#[derive(Clone)]
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
    Avatar { initials: initials.into(), size: 40.0, color: None, src: None, shape: AvatarShape::default(), status: None }
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

pebbles_core::stateless_widget!(Avatar);

impl StatelessWidget for Avatar {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let c = theme().colors;
        let bg = self.color.unwrap_or(c.secondary);
        let size = self.size;
        let radius = self.radius();
        let initials = std::mem::take(&mut self.initials);
        let mk = |init: String| initials_face(init, bg, c.secondary_foreground, size, radius);

        let face: AnyWidget = match self.src.take() {
            Some(url) => ImageView::network(url)
                .size(size, size)
                .fit(ImageFit::Cover)
                .radius(BorderRadius::all(radius))
                .placeholder(mk(initials.clone()))
                .error(mk(initials.clone()))
                .into_widget(),
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
                    .child(stack(children![
                        face,
                        Positioned::new(dot).left(size - d).top(size - d),
                    ]))
                    .into_widget()
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AvatarGroup
// ---------------------------------------------------------------------------

/// A row of overlapping avatars, capped with a "+N" bubble — shadcn's avatar stack.
#[derive(Clone)]
pub struct AvatarGroup {
    avatars: Vec<Avatar>,
    max: Option<usize>,
    size: f64,
}

/// Create an [`AvatarGroup`] from a list of avatars.
pub fn avatar_group(avatars: Vec<Avatar>) -> AvatarGroup {
    AvatarGroup { avatars, max: None, size: 40.0 }
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

pebbles_core::stateless_widget!(AvatarGroup);

impl StatelessWidget for AvatarGroup {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
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
            let bubble = initials_face(
                format!("+{overflow}"),
                c.muted,
                c.muted_foreground,
                size,
                size / 2.0,
            );
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
#[derive(Clone)]
pub struct Separator {
    vertical: bool,
    length: Option<f64>,
    thickness: f64,
    color: Option<Color>,
}

/// A horizontal separator (fills the available width unless given a length).
pub fn separator() -> Separator {
    Separator { vertical: false, length: None, thickness: 1.0, color: None }
}

impl Separator {
    /// A vertical separator (give it a length or place it in a bounded row).
    pub fn vertical() -> Self {
        Separator { vertical: true, length: None, thickness: 1.0, color: None }
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

pebbles_core::stateless_widget!(Separator);

impl StatelessWidget for Separator {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
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

/// A loading placeholder block.
#[derive(Clone)]
pub struct Skeleton {
    width: f64,
    height: f64,
}

/// Create a [`Skeleton`] of the given size.
pub fn skeleton(width: f64, height: f64) -> Skeleton {
    Skeleton { width, height }
}

pebbles_core::stateless_widget!(Skeleton);

impl StatelessWidget for Skeleton {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let c = theme().colors;
        Container::new()
            .decoration(BoxDecoration::new().color(c.muted).radius(BorderRadius::all(6.0)))
            .width(self.width)
            .height(self.height)
            .into_widget()
    }
}
