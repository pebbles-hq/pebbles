//! [`Stack`] + [`Positioned`] — overlay layout. `Stack` overlays children;
//! `Positioned` (a `ParentDataWidget`) pins a child to edges.

use std::any::Any;

use pebbles_foundation::Alignment;
use pebbles_render::{RenderObject, RenderStack, StackFit, StackParentData};

use pebbles_core::widget::{AnyWidget, ParentDataWidget, RenderWidget};

/// Overlays its children, aligning non-positioned ones and pinning `Positioned`
/// ones to their edges.
#[derive(Clone)]
pub struct Stack {
    children: Vec<AnyWidget>,
    alignment: Alignment,
    fit: StackFit,
}

/// Create a [`Stack`] overlaying `children`. Accepts `children![…]` or any iterator.
pub fn stack<I, W>(children: I) -> Stack
where
    I: IntoIterator<Item = W>,
    W: pebbles_core::widget::IntoWidget,
{
    Stack {
        children: pebbles_core::widget::collect_widgets(children),
        alignment: Alignment::TOP_LEFT,
        fit: StackFit::Loose,
    }
}

impl Stack {
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
    /// Force non-positioned children to fill the stack.
    pub fn expand(mut self) -> Self {
        self.fit = StackFit::Expand;
        self
    }
}

pebbles_core::render_widget!(Stack);

impl RenderWidget for Stack {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderStack::new(self.alignment, self.fit))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(s) = object.downcast_mut::<RenderStack>() {
            s.alignment = self.alignment;
            s.fit = self.fit;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        std::mem::take(&mut self.children)
    }
}

/// Pins a child within a [`Stack`] by any combination of edges + explicit size.
#[derive(Clone, Default)]
pub struct Positioned {
    data: StackParentData,
    child: Option<AnyWidget>,
}

impl Positioned {
    pub fn new(child: impl pebbles_core::IntoWidget) -> Self {
        Positioned { data: StackParentData::default(), child: Some(child.into_widget()) }
    }
    /// Pin to all four edges (fills the stack).
    pub fn fill(child: impl pebbles_core::IntoWidget) -> Self {
        Positioned::new(child).left(0.0).top(0.0).right(0.0).bottom(0.0)
    }
    pub fn left(mut self, v: f64) -> Self {
        self.data.left = Some(v);
        self
    }
    pub fn top(mut self, v: f64) -> Self {
        self.data.top = Some(v);
        self
    }
    pub fn right(mut self, v: f64) -> Self {
        self.data.right = Some(v);
        self
    }
    pub fn bottom(mut self, v: f64) -> Self {
        self.data.bottom = Some(v);
        self
    }
    pub fn width(mut self, v: f64) -> Self {
        self.data.width = Some(v);
        self
    }
    pub fn height(mut self, v: f64) -> Self {
        self.data.height = Some(v);
        self
    }
}

pebbles_core::parent_data_widget!(Positioned);

impl ParentDataWidget for Positioned {
    fn take_child(&mut self) -> Option<AnyWidget> {
        self.child.take()
    }
    fn parent_data(&self) -> Box<dyn Any> {
        Box::new(self.data)
    }
}
