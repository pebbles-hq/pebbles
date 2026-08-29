//! Surface & display components: [`Card`], [`Badge`], [`Alert`], [`Avatar`],
//! [`Separator`] and [`Skeleton`].

use pebbles_foundation::{Alignment, Color, CrossAxisAlignment, EdgeInsets, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, IconKind};

use pebbles_core::children;
use pebbles_core::context::BuildContext;
use crate::theme::theme;
use pebbles_core::widget::{AnyWidget, IntoWidget, StatelessWidget};
use crate::widgets::{ClipRRect, Container, SizedBox, center, column, row, text};

use crate::components::icon;

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

/// An elevated content surface with border, radius, shadow and padding.
#[derive(Clone)]
pub struct Card {
    child: Option<AnyWidget>,
    padding: EdgeInsets,
}

impl Card {
    pub fn new(child: impl IntoWidget) -> Self {
        Card { child: Some(child.into_widget()), padding: EdgeInsets::all(16.0) }
    }
    pub fn padding(mut self, insets: EdgeInsets) -> Self {
        self.padding = insets;
        self
    }
}

pebbles_core::stateless_widget!(Card);

impl StatelessWidget for Card {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let th = theme();
        Container::new()
            .decoration(
                BoxDecoration::new()
                    .color(th.colors.card)
                    .border(Border::new(th.colors.border, 1.0))
                    .radius(BorderRadius::all(th.radius + 4.0))
                    .shadow(BoxShadow::new(
                        Color::from_rgba8(0, 0, 0, 18),
                        Offset::new(0.0, 2.0),
                        8.0,
                        0.0,
                    )),
            )
            .padding(self.padding)
            .child(self.child.take().unwrap())
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

/// A circular avatar showing initials on a colored background.
#[derive(Clone)]
pub struct Avatar {
    initials: String,
    size: f64,
    color: Option<Color>,
}

/// Create an [`Avatar`] from initials (e.g. "RS").
pub fn avatar(initials: impl Into<String>) -> Avatar {
    Avatar { initials: initials.into(), size: 40.0, color: None }
}

impl Avatar {
    pub fn size(mut self, size: f64) -> Self {
        self.size = size;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
}

pebbles_core::stateless_widget!(Avatar);

impl StatelessWidget for Avatar {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let c = theme().colors;
        let bg = self.color.unwrap_or(c.secondary);
        ClipRRect::new(
            BorderRadius::all(self.size / 2.0),
            Container::new()
                .color(bg)
                .width(self.size)
                .height(self.size)
                .alignment(Alignment::CENTER)
                .child(center(
                    text(std::mem::take(&mut self.initials))
                        .size((self.size * 0.4) as f32)
                        .semibold()
                        .color(c.secondary_foreground),
                )),
        )
        .into_widget()
    }
}

// ---------------------------------------------------------------------------
// Separator
// ---------------------------------------------------------------------------

/// A hairline divider.
#[derive(Clone)]
pub struct Separator {
    vertical: bool,
    length: Option<f64>,
}

/// A horizontal separator.
pub fn separator() -> Separator {
    Separator { vertical: false, length: None }
}

impl Separator {
    /// A vertical separator (give it a length or place it in a bounded row).
    pub fn vertical() -> Self {
        Separator { vertical: true, length: None }
    }
    pub fn length(mut self, length: f64) -> Self {
        self.length = Some(length);
        self
    }
}

pebbles_core::stateless_widget!(Separator);

impl StatelessWidget for Separator {
    fn build(&mut self, _cx: &mut BuildContext) -> AnyWidget {
        let c = theme().colors;
        let mut container = Container::new().color(c.border);
        container = if self.vertical {
            container.width(1.0).height(self.length.unwrap_or(20.0))
        } else {
            match self.length {
                Some(l) => container.height(1.0).width(l),
                None => container.height(1.0),
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
