//! [`Row`] and [`Column`] — multi-child flex widgets backing
//! [`pebbles_render::RenderFlex`]. Build their children with the [`children!`]
//! macro: `column(children![text("a"), text("b")])`.

use pebbles_foundation::{Axis, CrossAxisAlignment, MainAxisAlignment, MainAxisSize};
use pebbles_render::{RenderFlex, RenderObject};

use pebbles_core::widget::{AnyWidget, RenderWidget};

/// Shared flex configuration for `Row`/`Column`.
#[derive(Clone, Copy)]
struct FlexConfig {
    main_axis_alignment: MainAxisAlignment,
    cross_axis_alignment: CrossAxisAlignment,
    main_axis_size: MainAxisSize,
    spacing: f64,
}

impl Default for FlexConfig {
    fn default() -> Self {
        FlexConfig {
            main_axis_alignment: MainAxisAlignment::Start,
            cross_axis_alignment: CrossAxisAlignment::Center,
            main_axis_size: MainAxisSize::Max,
            spacing: 0.0,
        }
    }
}

macro_rules! flex_builders {
    () => {
        /// Set how children are distributed along the main axis.
        pub fn main_axis_alignment(mut self, value: MainAxisAlignment) -> Self {
            self.config.main_axis_alignment = value;
            self
        }
        /// Set how children are aligned on the cross axis.
        pub fn cross_axis_alignment(mut self, value: CrossAxisAlignment) -> Self {
            self.config.cross_axis_alignment = value;
            self
        }
        /// Shrink-wrap the main axis instead of filling it.
        pub fn main_axis_min(mut self) -> Self {
            self.config.main_axis_size = MainAxisSize::Min;
            self
        }
        /// A fixed gap between adjacent children (Flutter's `spacing:`).
        pub fn spacing(mut self, value: f64) -> Self {
            self.config.spacing = value;
            self
        }
    };
}

// ---------------------------------------------------------------------------
// Row
// ---------------------------------------------------------------------------

/// Lays children out horizontally.
#[derive(Clone)]
pub struct Row {
    children: Vec<AnyWidget>,
    config: FlexConfig,
}

/// Create a horizontal [`Row`]. Accepts `children![…]` or any iterator of widgets.
pub fn row<I, W>(children: I) -> Row
where
    I: IntoIterator<Item = W>,
    W: pebbles_core::widget::IntoWidget,
{
    Row { children: pebbles_core::widget::collect_widgets(children), config: FlexConfig::default() }
}

impl Row {
    flex_builders!();
}

pebbles_core::render_widget!(Row);

impl RenderWidget for Row {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderFlex::new(
            Axis::Horizontal,
            self.config.main_axis_alignment,
            self.config.cross_axis_alignment,
            self.config.main_axis_size,
            self.config.spacing,
        ))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(f) = object.downcast_mut::<RenderFlex>() {
            f.main_axis_alignment = self.config.main_axis_alignment;
            f.cross_axis_alignment = self.config.cross_axis_alignment;
            f.main_axis_size = self.config.main_axis_size;
            f.spacing = self.config.spacing;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        std::mem::take(&mut self.children)
    }
}

// ---------------------------------------------------------------------------
// Column
// ---------------------------------------------------------------------------

/// Lays children out vertically.
#[derive(Clone)]
pub struct Column {
    children: Vec<AnyWidget>,
    config: FlexConfig,
}

/// Create a vertical [`Column`]. Accepts `children![…]` or any iterator of widgets.
pub fn column<I, W>(children: I) -> Column
where
    I: IntoIterator<Item = W>,
    W: pebbles_core::widget::IntoWidget,
{
    Column { children: pebbles_core::widget::collect_widgets(children), config: FlexConfig::default() }
}

impl Column {
    flex_builders!();
}

pebbles_core::render_widget!(Column);

impl RenderWidget for Column {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderFlex::new(
            Axis::Vertical,
            self.config.main_axis_alignment,
            self.config.cross_axis_alignment,
            self.config.main_axis_size,
            self.config.spacing,
        ))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(f) = object.downcast_mut::<RenderFlex>() {
            f.main_axis_alignment = self.config.main_axis_alignment;
            f.cross_axis_alignment = self.config.cross_axis_alignment;
            f.main_axis_size = self.config.main_axis_size;
            f.spacing = self.config.spacing;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        std::mem::take(&mut self.children)
    }
}
