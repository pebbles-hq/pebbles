//! The [`Widget`] trait and its categories: **function components**, **render-object**
//! and **parent-data** widgets.
//!
//! A widget is an *immutable configuration*. It is cheap to construct and throw
//! away; the retained state lives in the element and render trees. The framework
//! discovers a widget's category by asking it (`as_component` / `as_render` /
//! `as_parent_data`), which keeps everything object-safe without a big enum.

use std::any::Any;

use pebbles_render::RenderObject;

use crate::key::Key;

/// A type-erased, owned widget. The currency of the build methods and child lists.
pub type AnyWidget = Box<dyn Widget>;

// `Box<dyn Widget>` is cloneable via `clone_box` (Box is a fundamental type and
// `dyn Widget` is local, so this impl is allowed). This makes `Option<AnyWidget>`
// and `Vec<AnyWidget>` fields `#[derive(Clone)]`-able throughout the catalog.
impl Clone for Box<dyn Widget> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// The base trait every widget implements. Concrete widgets normally get this via
/// the [`render_widget!`] or [`parent_data_widget!`] macro rather than by hand;
/// composite UI is authored as [`component`](crate::component) function components.
pub trait Widget: 'static {
    /// A name for diagnostics and tree dumps.
    fn debug_name(&self) -> &'static str;

    /// This widget's reconciliation key, if any.
    fn key(&self) -> Option<Key> {
        None
    }

    /// Upcast for type comparison during reconciliation.
    fn as_any(&self) -> &dyn Any;

    /// Clone this widget into a fresh boxed copy. Widgets are immutable, cheap
    /// configuration objects (like Flutter's), so cloning a subtree is a normal,
    /// inexpensive operation — it's what lets a component hold and re-render an
    /// arbitrary child across reactive updates. The `render_widget!`/
    /// `parent_data_widget!` macros implement this for you (the widget just needs
    /// to `#[derive(Clone)]`).
    fn clone_box(&self) -> AnyWidget;

    fn as_render(&self) -> Option<&dyn RenderWidget> {
        None
    }
    fn as_render_mut(&mut self) -> Option<&mut dyn RenderWidget> {
        None
    }
    fn as_parent_data(&self) -> Option<&dyn ParentDataWidget> {
        None
    }
    fn as_parent_data_mut(&mut self) -> Option<&mut dyn ParentDataWidget> {
        None
    }
    /// A function component: its `(identity, render thunk)` if this widget is one.
    fn as_component(&self) -> Option<(usize, std::rc::Rc<dyn Fn() -> AnyWidget>)> {
        None
    }
}

/// Downcast helpers for `dyn Widget` — recover a concrete widget type from a boxed
/// one. Uses trait upcasting to `dyn Any`.
impl dyn Widget {
    pub fn downcast_ref<T: Widget>(&self) -> Option<&T> {
        self.as_any().downcast_ref::<T>()
    }
    pub fn is<T: Widget>(&self) -> bool {
        self.as_any().is::<T>()
    }
}

/// Conversion into an [`AnyWidget`]. Implemented for every concrete widget (by
/// boxing) and for [`AnyWidget`] itself (identity), so constructors can accept
/// either a concrete widget or an already-boxed one without double-boxing — which
/// would corrupt the type identity reconciliation relies on.
pub trait IntoWidget {
    fn into_widget(self) -> AnyWidget;
}

impl<W: Widget> IntoWidget for W {
    fn into_widget(self) -> AnyWidget {
        Box::new(self)
    }
}

impl IntoWidget for AnyWidget {
    fn into_widget(self) -> AnyWidget {
        self
    }
}

/// A list of child widgets for `row`/`column`/`wrap`/`stack` (and any other
/// children-taking API). **One syntax** (see the UI syntax guide):
///
/// * literal children — the [`children!`] list: `column(children![text("a"), button("b")])`
/// * computed children — a `Vec`: `column(items.iter().map(row_for).collect::<Vec<_>>())`
///
/// Both are the same thing — `children![..]` builds the `Vec` while boxing each
/// element to [`AnyWidget`] (Rust children are heterogeneous concrete types; the macro
/// erases them, which Dart's untyped `List<Widget>` did implicitly). The `Vec<W>` impl
/// accepts both `Vec<AnyWidget>` and a `Vec` of one concrete widget type.
pub trait IntoChildren {
    fn into_children(self) -> Vec<AnyWidget>;
}

impl<W: IntoWidget> IntoChildren for Vec<W> {
    fn into_children(self) -> Vec<AnyWidget> {
        self.into_iter().map(IntoWidget::into_widget).collect()
    }
}

/// A widget that owns a [`RenderObject`] — the leaves of composition where actual
/// layout and painting happen. `take_children` yields the child *widgets* (none
/// for a leaf, one for a single-child box, many for a flex), consumed once by the
/// framework as it inflates or reconciles the subtree.
pub trait RenderWidget: 'static {
    /// Create the backing render object.
    fn create_render_object(&self) -> Box<dyn RenderObject>;

    /// Push this widget's current properties onto an existing render object,
    /// avoiding a rebuild when only values changed.
    fn update_render_object(&self, object: &mut dyn RenderObject);

    /// Move the child widgets out of this configuration. Leaf render widgets keep
    /// the default (no children).
    fn take_children(&mut self) -> Vec<AnyWidget> {
        Vec::new()
    }
}

/// A widget that owns no render object but attaches *parent data* to its child's
/// render object — e.g. `Expanded`/`Flexible` (flex factor) or `Positioned`
/// (stack edges). The engine sets [`ParentDataWidget::parent_data`] on the nearest
/// render descendant, where the enclosing `RenderFlex`/`RenderStack` reads it.
pub trait ParentDataWidget: 'static {
    /// The single wrapped child.
    fn take_child(&mut self) -> Option<AnyWidget>;

    /// The parent-data value to attach (e.g. a boxed `FlexParentData`).
    fn parent_data(&self) -> Box<dyn Any>;
}

/// Implement [`Widget`] for a parent-data widget type.
#[macro_export]
macro_rules! parent_data_widget {
    ($ty:ty) => {
        impl $crate::Widget for $ty {
            fn debug_name(&self) -> &'static str {
                stringify!($ty)
            }
            fn as_any(&self) -> &dyn ::core::any::Any {
                self
            }
            fn clone_box(&self) -> $crate::AnyWidget {
                ::std::boxed::Box::new(::core::clone::Clone::clone(self))
            }
            fn as_parent_data(&self) -> Option<&dyn $crate::ParentDataWidget> {
                Some(self)
            }
            fn as_parent_data_mut(&mut self) -> Option<&mut dyn $crate::ParentDataWidget> {
                Some(self)
            }
        }
    };
}

/// Implement [`Widget`] for a render-object widget type.
#[macro_export]
macro_rules! render_widget {
    ($ty:ty) => {
        impl $crate::Widget for $ty {
            fn debug_name(&self) -> &'static str {
                stringify!($ty)
            }
            fn as_any(&self) -> &dyn ::core::any::Any {
                self
            }
            fn clone_box(&self) -> $crate::AnyWidget {
                ::std::boxed::Box::new(::core::clone::Clone::clone(self))
            }
            fn as_render(&self) -> Option<&dyn $crate::RenderWidget> {
                Some(self)
            }
            fn as_render_mut(&mut self) -> Option<&mut dyn $crate::RenderWidget> {
                Some(self)
            }
        }
    };
}

/// Build a `Vec<AnyWidget>` from a comma-separated list of children. Each entry may
/// be a concrete widget *or* an already-boxed [`AnyWidget`] — [`IntoWidget`] handles
/// both without double-boxing.
#[macro_export]
macro_rules! children {
    ($($child:expr),* $(,)?) => {
        vec![ $( $crate::IntoWidget::into_widget($child) ),* ]
    };
}
