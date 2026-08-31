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
        /// How children are distributed along the main axis (Flutter's
        /// `mainAxisAlignment:`).
        pub fn main_axis_alignment(mut self, value: MainAxisAlignment) -> Self {
            self.config.main_axis_alignment = value;
            self
        }
        /// How children are aligned on the cross axis (Flutter's
        /// `crossAxisAlignment:`).
        pub fn cross_axis_alignment(mut self, value: CrossAxisAlignment) -> Self {
            self.config.cross_axis_alignment = value;
            self
        }
        /// Whether the main axis fills the parent or shrink-wraps the children
        /// (Flutter's `mainAxisSize:`).
        pub fn main_axis_size(mut self, value: MainAxisSize) -> Self {
            self.config.main_axis_size = value;
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

/// Create a horizontal [`Row`]. Children are a `children![…]` list literal, or a
/// `Vec` for computed lists.
pub fn row(children: impl pebbles_core::widget::IntoChildren) -> Row {
    Row { children: children.into_children(), config: FlexConfig::default() }
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

/// Create a vertical [`Column`]. Children are a `children![…]` list literal, or a
/// `Vec` for computed lists.
pub fn column(children: impl pebbles_core::widget::IntoChildren) -> Column {
    Column { children: children.into_children(), config: FlexConfig::default() }
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
