//! [`Fab`] — a floating action button (Flutter's `FloatingActionButton`). A circular,
//! elevated action button; `.mini(true)` shrinks it and `.label(..)` turns it into an
//! extended pill (icon + text). The canonical placement is the [`Scaffold`]'s `.fab(..)`
//! slot (bottom-right), but it works standalone in any `Stack`.

use std::rc::Rc;

use pebbles_foundation::{Color, MainAxisSize, Offset};
use pebbles_render::{Border, BorderRadius, BoxDecoration, BoxShadow, Cursor, IconData};

use crate::components::icon;
use crate::theme::theme;
use crate::widgets::{Container, GestureDetector, Opacity, center, gap_w, row, text};
use pebbles_core::component::{Element, component_props};
use pebbles_core::context::Callback;
use pebbles_core::focus::create_focus;
use pebbles_core::widget::{AnyWidget, IntoWidget};

/// A floating action button. Built by [`fab`].
#[derive(Clone)]
pub struct Fab {
    icon: IconData,
    label: Option<String>,
    mini: bool,
    color: Option<Color>,
    disabled: bool,
    on_pressed: Option<Callback>,
}

/// A circular floating action button with `icon`. Add [`Fab::on_pressed`]; use
/// [`Fab::label`] for the extended (pill) form and [`Fab::mini`] for the small form.
pub fn fab(icon: impl Into<IconData>) -> Fab {
    Fab { icon: icon.into(), label: None, mini: false, color: None, disabled: false, on_pressed: None }
}

impl Fab {
    /// A trailing label — turns the FAB into an extended pill (icon + text). It also
    /// serves as the FAB's accessible label for screen readers.
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
    /// Dim the FAB and stop it responding to taps/keyboard (still announced, disabled).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
    /// The tap handler.
    pub fn on_pressed(mut self, cb: impl Fn() + 'static) -> Self {
        self.on_pressed = Some(pebbles_core::action(cb));
        self
    }
}

impl IntoWidget for Fab {
    fn into_widget(self) -> AnyWidget {
        Box::new(component_props(render_fab, self))
    }
}

fn render_fab(f: &Fab) -> Element {
    let node = create_focus();
    let th = theme();
    let c = th.colors;
    let bg = f.color.unwrap_or(c.primary);
    let fg = c.primary_foreground;
    let elevation = BoxShadow::new(Color::from_rgba8(0, 0, 0, 55), Offset::new(0.0, 5.0), 16.0, -3.0);
    let extended = f.label.is_some();
    let icon_size = if f.mini { 18.0 } else { 22.0 };
    let focused = !f.disabled && node.is_focused();

    let inner: AnyWidget = match f.label.clone() {
        Some(lbl) => row(vec![
            icon(f.icon).size(icon_size).color(fg).into_widget(),
            gap_w(10.0).into_widget(),
            text(lbl).size(14.0).semibold().color(fg).into_widget(),
        ])
        .main_axis_size(MainAxisSize::Min)
        .into_widget(),
        None => icon(f.icon).size(icon_size).color(fg).into_widget(),
    };

    let radius = BorderRadius::all(if extended { 16.0 } else { 999.0 });
    let mut deco = BoxDecoration::new().color(bg).radius(radius).shadow(elevation);
    // Focus ring.
    if focused {
        deco = deco.border(Border::new(c.ring, 2.0));
    }
    let pill = if extended {
        Container::new()
            .decoration(deco)
            .height(if f.mini { 40.0 } else { 48.0 })
            .padding(theme().pad(20.0, 0.0))
            .child(center(inner))
    } else {
        let d = if f.mini { 40.0 } else { 56.0 };
        Container::new().decoration(deco).width(d).height(d).child(center(inner))
    };

    // Accessibility wrapper — applied at every exit so a disabled FAB is announced too
    // (with its disabled state). The visible pill label doubles as the a11y label.
    let label = f.label.clone().unwrap_or_default();
    let a11y = |w: AnyWidget, disabled: bool| {
        crate::widgets::semantics(crate::widgets::SemanticsRole::Button, label.clone(), w)
            .disabled(disabled)
            .into_widget()
    };

    if f.disabled {
        return a11y(
            GestureDetector::new(Opacity::new(0.55, pill)).cursor(Cursor::NotAllowed).into_widget(),
            true,
        );
    }

    // Register keyboard activation (Enter/Space while focused).
    let activation: Rc<dyn Fn()> = match &f.on_pressed {
        Some(Callback::Plain(cb)) => cb.clone(),
        _ => Rc::new(|| {}),
    };
    node.register(activation, None, false);

    let mut gesture = GestureDetector::new(pill).cursor(Cursor::Pointer).on_pointer_down(move || {
        node.request_focus(); // clicking focuses, like Flutter
    });
    if let Some(cb) = f.on_pressed.clone() {
        gesture = gesture.on_tap(cb);
    }
    a11y(gesture.into_widget(), false)
}
