//! [`Fab`] — a floating action button (Flutter's `FloatingActionButton`). A circular,
//! elevated action button; `.mini(true)` shrinks it and `.label(..)` turns it into an
//! extended pill (icon + text). The canonical placement is the [`Scaffold`]'s `.fab(..)`
//! slot (bottom-right), but it works standalone in any `Stack`.

use pebbles_foundation::{Color, EdgeInsets, MainAxisSize, Offset};
use pebbles_render::{BorderRadius, BoxDecoration, BoxShadow, Cursor, IconData};

use crate::components::icon;
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, center, gap_w, row, text};
use pebbles_core::context::Callback;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A floating action button. Built by [`fab`].
#[derive(Clone)]
pub struct Fab {
    icon: IconData,
    label: Option<String>,
    mini: bool,
    color: Option<Color>,
    on_pressed: Option<Callback>,
}

/// A circular floating action button with `icon`. Add [`Fab::on_pressed`]; use
/// [`Fab::label`] for the extended (pill) form and [`Fab::mini`] for the small form.
pub fn fab(icon: impl Into<IconData>) -> Fab {
    Fab { icon: icon.into(), label: None, mini: false, color: None, on_pressed: None }
}

impl Fab {
    /// A trailing label — turns the FAB into an extended pill (icon + text).
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }
    /// The smaller 40px form (default is the 56px standard FAB).
    pub fn mini(mut self, mini: bool) -> Self {
        self.mini = mini;
        self
    }
    /// Override the background color (default: the theme's primary).
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    /// The tap handler.
    pub fn on_pressed(mut self, cb: impl Fn() + 'static) -> Self {
        self.on_pressed = Some(pebbles_core::action(cb));
        self
    }
}

impl IntoWidget for Fab {
    fn into_widget(mut self) -> AnyWidget {
        let c = theme().colors;
        let bg = self.color.unwrap_or(c.primary);
        let fg = c.primary_foreground;
        let elevation = BoxShadow::new(Color::from_rgba8(0, 0, 0, 55), Offset::new(0.0, 5.0), 16.0, -3.0);
        let extended = self.label.is_some();
        let icon_size = if self.mini { 18.0 } else { 22.0 };

        let inner: AnyWidget = match self.label.take() {
            Some(lbl) => row(vec![
                icon(self.icon).size(icon_size).color(fg).into_widget(),
                gap_w(10.0).into_widget(),
                text(lbl).size(14.0).semibold().color(fg).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min)
            .into_widget(),
            None => icon(self.icon).size(icon_size).color(fg).into_widget(),
        };

        let radius = BorderRadius::all(if extended { 16.0 } else { 999.0 });
        let deco = BoxDecoration::new().color(bg).radius(radius).shadow(elevation);
        let pill = if extended {
            Container::new()
                .decoration(deco)
                .height(if self.mini { 40.0 } else { 48.0 })
                .padding(EdgeInsets::symmetric(20.0, 0.0))
                .child(center(inner))
        } else {
            let d = if self.mini { 40.0 } else { 56.0 };
            Container::new().decoration(deco).width(d).height(d).child(center(inner))
        };

        match self.on_pressed.take() {
            Some(on_pressed) => {
                GestureDetector::new(pill).on_tap(on_pressed).cursor(Cursor::Pointer).into_widget()
            }
            None => pill.into_widget(),
        }
    }
}
