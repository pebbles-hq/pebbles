//! [`Expanded`] and [`Flexible`] — `ParentDataWidget`s that give a `Row`/`Column`
//! child a flex factor, plus [`spacer`], a flexible empty gap.

use std::any::Any;
use crate::widgets::{gap_h};

use pebbles_foundation::FlexFit;
use pebbles_render::FlexParentData;

use pebbles_core::widget::{AnyWidget, ParentDataWidget};

/// A flex child that fills its share of the main axis (`FlexFit::Tight`).
#[derive(Clone)]
pub struct Expanded {
    flex: u32,
    child: Option<AnyWidget>,
}

impl Expanded {
    pub fn new(child: impl pebbles_core::IntoWidget) -> Self {
        Expanded { flex: 1, child: Some(child.into_widget()) }
    }
    /// Set the flex factor (default 1).
    pub fn flex(mut self, flex: u32) -> Self {
        self.flex = flex.max(1);
        self
    }
}

pebbles_core::parent_data_widget!(Expanded);

impl ParentDataWidget for Expanded {
    fn take_child(&mut self) -> Option<AnyWidget> {
        self.child.take()
    }
    fn parent_data(&self) -> Box<dyn Any> {
        Box::new(FlexParentData { flex: self.flex, fit: FlexFit::Tight })
    }
}

/// A flex child that may take *up to* its share of the main axis (`FlexFit::Loose`).
#[derive(Clone)]
pub struct Flexible {
    flex: u32,
    child: Option<AnyWidget>,
}

impl Flexible {
    pub fn new(child: impl pebbles_core::IntoWidget) -> Self {
        Flexible { flex: 1, child: Some(child.into_widget()) }
    }
    pub fn flex(mut self, flex: u32) -> Self {
        self.flex = flex.max(1);
        self
    }
}

pebbles_core::parent_data_widget!(Flexible);

impl ParentDataWidget for Flexible {
    fn take_child(&mut self) -> Option<AnyWidget> {
        self.child.take()
    }
    fn parent_data(&self) -> Box<dyn Any> {
        Box::new(FlexParentData { flex: self.flex, fit: FlexFit::Loose })
    }
}

/// A flexible empty gap that pushes siblings apart in a `Row`/`Column`.
pub fn spacer() -> Expanded {
    Expanded::new(gap_h(0.0))
}
