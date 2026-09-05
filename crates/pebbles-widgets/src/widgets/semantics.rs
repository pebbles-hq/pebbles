//! [`Semantics`] — annotate a widget subtree with accessibility metadata (role,
//! label, state). It wraps a child and attaches [`SemanticsProps`] to that child's
//! render node (via the parent-data channel), where the shell reads it to build the
//! platform accessibility tree. Layout- and paint-transparent: it changes nothing
//! visible, only what assistive technology sees.
//!
//! Interactive widgets (Button, Checkbox/Switch/Radio, TextField, Slider, Select) wrap
//! themselves in this, so apps get accessibility for free; use it directly for custom
//! controls or to label a decorative element.

use std::any::Any;

use pebbles_render::{RenderObject, RenderSemanticsBoundary, SemanticsFlag, SemanticsProps, SemanticsRole};

use pebbles_core::widget::{AnyWidget, IntoWidget, ParentDataWidget, RenderWidget, Widget};

/// Wraps a child with an accessibility annotation.
#[derive(Clone)]
pub struct Semantics {
    props: SemanticsProps,
    child: Option<AnyWidget>,
}

/// Annotate `child` with the given role + label. Chain `.value()/.checked()/.disabled()`
/// (on the returned widget) for richer state.
pub fn semantics(role: SemanticsRole, label: impl Into<String>, child: impl IntoWidget) -> Semantics {
    Semantics { props: SemanticsProps::new(role, label), child: Some(child.into_widget()) }
}

impl Semantics {
    /// A textual value (text inputs, sliders, combo boxes).
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.props.value = Some(value.into());
        self
    }
    /// Toggle state (checkbox / switch / radio).
    pub fn checked(mut self, checked: bool) -> Self {
        self.props.checked = Some(checked);
        self
    }
    /// Mark the control disabled (announced, not actionable).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.props.disabled = disabled;
        self
    }
}

pebbles_core::parent_data_widget!(Semantics);

impl ParentDataWidget for Semantics {
    fn take_child(&mut self) -> Option<AnyWidget> {
        self.child.take()
    }
    fn parent_data(&self) -> Box<dyn Any> {
        Box::new(self.props.clone())
    }
}

/// Child-first convenience: `my_button.labelled(SemanticsRole::Button, "Save")`.
pub trait SemanticsExt: IntoWidget + Sized {
    /// Wrap `self` in a [`Semantics`] annotation.
    fn labelled(self, role: SemanticsRole, label: impl Into<String>) -> Semantics {
        semantics(role, label, self)
    }
}
impl<W: IntoWidget> SemanticsExt for W {}

// ===========================================================================
// Semantics combinators — MergeSemantics / ExcludeSemantics / BlockSemantics
// ===========================================================================

/// A transparent wrapper that reshapes how a subtree appears to a screen reader.
/// Built by [`merge_semantics`] / [`exclude_semantics`] / [`block_semantics`]; layout
/// and paint pass straight through.
#[derive(Clone)]
pub struct SemanticsBoundary {
    flag: SemanticsFlag,
    child: Option<AnyWidget>,
}

/// Collapse `child`'s subtree into ONE semantics node (its labels joined) — a screen
/// reader reads the group as a single item. Flutter's `MergeSemantics`.
pub fn merge_semantics(child: impl IntoWidget) -> SemanticsBoundary {
    SemanticsBoundary { flag: SemanticsFlag::Merge, child: Some(child.into_widget()) }
}

/// Hide `child`'s subtree from the accessibility tree entirely (still painted).
/// Flutter's `ExcludeSemantics`.
pub fn exclude_semantics(child: impl IntoWidget) -> SemanticsBoundary {
    SemanticsBoundary { flag: SemanticsFlag::Exclude, child: Some(child.into_widget()) }
}

/// Drop the semantics of everything painted BEFORE `child` in the same parent (lower
/// layers / earlier siblings) — a modal barrier. Flutter's `BlockSemantics`.
pub fn block_semantics(child: impl IntoWidget) -> SemanticsBoundary {
    SemanticsBoundary { flag: SemanticsFlag::Block, child: Some(child.into_widget()) }
}

impl Widget for SemanticsBoundary {
    fn debug_name(&self) -> &'static str {
        "SemanticsBoundary"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn clone_box(&self) -> AnyWidget {
        Box::new(self.clone())
    }
    fn as_render(&self) -> Option<&dyn RenderWidget> {
        Some(self)
    }
    fn as_render_mut(&mut self) -> Option<&mut dyn RenderWidget> {
        Some(self)
    }
}

impl RenderWidget for SemanticsBoundary {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderSemanticsBoundary::new(self.flag))
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(b) = object.downcast_mut::<RenderSemanticsBoundary>() {
            b.flag = self.flag;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}
