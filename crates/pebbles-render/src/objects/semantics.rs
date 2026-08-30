//! Accessibility semantics — a platform-neutral description of the interactive UI,
//! fed to an assistive-technology bridge (AccessKit, in the shell).
//!
//! A widget annotates its render subtree with [`SemanticsProps`] (role + label +
//! state). The widget layer stores that on the anchor render node; the shell then
//! walks the laid-out tree with [`RenderTree::semantics_tree`](crate::RenderTree::
//! semantics_tree) each frame to produce a flat list of [`SemanticsNode`]s (id, role,
//! label, window-space bounds, state) that it maps onto the platform accessibility
//! tree. Kept dependency-free here so the tree is unit-testable without a platform.

use pebbles_foundation::Rect;

/// The kind of control a semantics node represents. Maps to an AccessKit `Role` in the
/// shell; a small, useful subset for v1 (Button/toggles/text/slider/select + grouping).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SemanticsRole {
    Button,
    Checkbox,
    Switch,
    RadioButton,
    TextInput,
    Slider,
    ComboBox,
    Link,
    Image,
    Label,
    Group,
}

/// The accessibility description a widget attaches to its render subtree.
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticsProps {
    pub role: SemanticsRole,
    /// The accessible name announced by the screen reader (e.g. a button's text).
    pub label: String,
    /// A textual value, for text inputs / sliders / combo boxes.
    pub value: Option<String>,
    /// Toggle state for checkbox / switch / radio (`None` for non-toggles).
    pub checked: Option<bool>,
    /// Whether the control is disabled (announced, and not actionable).
    pub disabled: bool,
}

impl SemanticsProps {
    /// A node with just a role + label (the common case).
    pub fn new(role: SemanticsRole, label: impl Into<String>) -> Self {
        SemanticsProps { role, label: label.into(), value: None, checked: None, disabled: false }
    }
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = Some(checked);
        self
    }
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// One resolved accessibility node: a widget's semantics plus its window-space bounds
/// and the id of the element that owns it (for correlating focus). Produced by
/// [`RenderTree::semantics_tree`](crate::RenderTree::semantics_tree).
#[derive(Clone, Debug, PartialEq)]
pub struct SemanticsNode {
    /// Stable identity — the owning widget element id (from the render node's `source`),
    /// or a synthesized index when untagged. Matches focus ids so focus can be mapped.
    pub id: u64,
    pub props: SemanticsProps,
    /// Window-space bounds (logical pixels).
    pub bounds: Rect,
}
