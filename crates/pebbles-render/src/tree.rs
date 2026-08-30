//! The render tree: a generational arena of [`RenderNode`]s plus the layout,
//! paint and hit-test drivers and the [`LayoutCx`] / [`PaintCx`] traversal contexts.
//!
//! ### Why an arena?
//! A parent render object must lay out its children, which means mutating another
//! node in the same tree while it is itself being mutated. `Rc<RefCell<…>>` makes
//! that possible but pays refcount + borrow-flag costs on every access and invites
//! borrow panics. Instead every node lives in a [`slotmap::SlotMap`] keyed by a
//! generational [`RenderId`], and traversal happens through ids. To lay out a
//! child we *take its boxed object out of the arena*, recurse with a fresh context
//! that holds `&mut` to the (now-hole-free) arena, then put the object back. No
//! aliasing, no interior mutability, no panics.

use std::any::Any;

use pebbles_foundation::{Offset, Rect, Size};
use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;
use vello::kurbo::Affine;

use crate::constraints::BoxConstraints;
use crate::object::RenderObject;
use crate::text::TextEnv;

new_key_type! {
    /// A stable, generational handle to a node in the [`RenderTree`].
    pub struct RenderId;
}

/// One node in the render tree: a boxed [`RenderObject`] plus the structural and
/// computed-layout data the framework maintains around it.
pub struct RenderNode {
    /// The render object. `None` only transiently, while it is taken out of the
    /// arena for a re-entrant layout call.
    object: Option<Box<dyn RenderObject>>,
    /// Parent link, or `None` for the root.
    pub parent: Option<RenderId>,
    /// Child links, in paint order (first painted first, i.e. bottom to top).
    pub children: SmallVec<[RenderId; 4]>,
    /// Position relative to the parent's origin, assigned by the parent's layout.
    pub offset: Offset,
    /// Size computed by the most recent layout pass.
    pub size: Size,
    /// Layout-time data the parent attaches to this child (e.g. flex factor).
    pub parent_data: Option<Box<dyn Any>>,
    /// Opaque id of the widget-layer element that created this node (for stable
    /// identity across rebuilds, e.g. gesture arming). Set by the widget layer.
    pub source: Option<u64>,
    pub needs_layout: bool,
    pub needs_paint: bool,
}

impl RenderNode {
    fn new(object: Box<dyn RenderObject>) -> Self {
        RenderNode {
            object: Some(object),
            parent: None,
            children: SmallVec::new(),
            offset: Offset::ZERO,
            size: Size::ZERO,
            parent_data: None,
            source: None,
            needs_layout: true,
            needs_paint: true,
        }
    }
}

/// The arena that owns every render object plus the identity of the root.
#[derive(Default)]
pub struct RenderTree {
    nodes: SlotMap<RenderId, RenderNode>,
    pub root: Option<RenderId>,
}

impl RenderTree {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a detached render object and return its id.
    pub fn insert(&mut self, object: Box<dyn RenderObject>) -> RenderId {
        self.nodes.insert(RenderNode::new(object))
    }

    pub fn contains(&self, id: RenderId) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn size_of(&self, id: RenderId) -> Size {
        self.nodes[id].size
    }

    /// Find the first render node whose object is of type `T`. Handy for tests and
    /// debugging (locate a specific box in a laid-out tree).
    pub fn find<T: RenderObject>(&self) -> Option<RenderId> {
        self.nodes
            .iter()
            .find(|(_, node)| node.object.as_deref().is_some_and(|o| o.is::<T>()))
            .map(|(id, _)| id)
    }

    pub fn offset_of(&self, id: RenderId) -> Offset {
        self.nodes[id].offset
    }

    /// Replace the boxed render object at `id` in place, preserving its structural
    /// links. Used by the widget layer when a widget updates its render object's
    /// properties without changing the tree shape.
    pub fn object_mut(&mut self, id: RenderId) -> &mut dyn RenderObject {
        self.nodes[id].object.as_deref_mut().expect("object present outside of layout")
    }

    pub fn object_ref(&self, id: RenderId) -> &dyn RenderObject {
        self.nodes[id].object.as_deref().expect("object present outside of paint")
    }

    /// Like [`object_mut`](Self::object_mut) but returns `None` if the node no longer
    /// exists (e.g. an animating scroll view was unmounted mid-tick).
    pub fn try_object_mut(&mut self, id: RenderId) -> Option<&mut dyn RenderObject> {
        self.nodes.get_mut(id)?.object.as_deref_mut()
    }

    /// Attach `child` under `parent` at `index` (appending if `index` is out of
    /// range). Marks the subtree dirty.
    pub fn insert_child(&mut self, parent: RenderId, child: RenderId, index: usize) {
        self.nodes[child].parent = Some(parent);
        let children = &mut self.nodes[parent].children;
        let idx = index.min(children.len());
        children.insert(idx, child);
        self.mark_needs_layout(parent);
    }

    /// Remove `child` from `parent`'s child list (does not free the node).
    pub fn remove_child(&mut self, parent: RenderId, child: RenderId) {
        if let Some(pos) = self.nodes[parent].children.iter().position(|&c| c == child) {
            self.nodes[parent].children.remove(pos);
        }
        if self.nodes.contains_key(child) {
            self.nodes[child].parent = None;
        }
        self.mark_needs_layout(parent);
    }

    /// Replace `parent`'s entire child list, fixing up parent links on both the
    /// old and new children. Used by the widget layer to re-project the element
    /// tree onto the render tree after a build.
    pub fn set_children(&mut self, parent: RenderId, children: Vec<RenderId>) {
        let old = std::mem::take(&mut self.nodes[parent].children);
        for c in old {
            if self.nodes.contains_key(c) {
                self.nodes[c].parent = None;
            }
        }
        let mut next = SmallVec::<[RenderId; 4]>::new();
        for c in children {
            if self.nodes.contains_key(c) {
                self.nodes[c].parent = Some(parent);
                next.push(c);
            }
        }
        self.nodes[parent].children = next;
        self.mark_needs_layout(parent);
    }

    /// Remove a single node (not its descendants), detaching it from its parent.
    /// The widget layer unmounts elements depth-first, so each element removes only
    /// its own render node.
    pub fn remove_node(&mut self, id: RenderId) {
        if !self.nodes.contains_key(id) {
            return;
        }
        if let Some(parent) = self.nodes[id].parent
            && self.nodes.contains_key(parent)
            && let Some(pos) = self.nodes[parent].children.iter().position(|&c| c == id)
        {
            self.nodes[parent].children.remove(pos);
        }
        self.nodes.remove(id);
        if self.root == Some(id) {
            self.root = None;
        }
    }

    /// Recursively free `id` and all of its descendants.
    pub fn drop_subtree(&mut self, id: RenderId) {
        if !self.nodes.contains_key(id) {
            return;
        }
        let children = std::mem::take(&mut self.nodes[id].children);
        for child in children {
            self.drop_subtree(child);
        }
        self.nodes.remove(id);
    }

    pub fn set_parent_data(&mut self, id: RenderId, data: Box<dyn Any>) {
        self.nodes[id].parent_data = Some(data);
    }

    /// Tag a node with the id of the element that created it.
    pub fn set_source(&mut self, id: RenderId, source: u64) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.source = Some(source);
        }
    }

    /// The element id that created a node, if tagged.
    pub fn source_of(&self, id: RenderId) -> Option<u64> {
        self.nodes.get(id).and_then(|n| n.source)
    }

    /// The first node tagged with `source` (element ids are unique per node).
    pub fn find_by_source(&self, source: u64) -> Option<RenderId> {
        self.nodes.iter().find(|(_, n)| n.source == Some(source)).map(|(id, _)| id)
    }

    /// A node's absolute (window-space) top-left, accumulated up the tree.
    pub fn absolute_offset(&self, id: RenderId) -> Offset {
        let mut acc = Offset::ZERO;
        let mut cur = Some(id);
        while let Some(c) = cur {
            let node = &self.nodes[c];
            acc += node.offset;
            cur = node.parent;
        }
        acc
    }

    /// Mark `id` (and, transitively, the path to the root) as needing layout.
    ///
    /// A production framework would stop at the nearest *relayout boundary*; v0
    /// keeps it simple and dirties up to the root, which is correct if less optimal.
    pub fn mark_needs_layout(&mut self, id: RenderId) {
        let mut cur = Some(id);
        while let Some(c) = cur {
            let node = &mut self.nodes[c];
            if node.needs_layout {
                // Ancestors above an already-dirty node are dirty too; stop early.
                node.needs_paint = true;
                break;
            }
            node.needs_layout = true;
            node.needs_paint = true;
            cur = node.parent;
        }
    }

    pub fn mark_needs_paint(&mut self, id: RenderId) {
        self.nodes[id].needs_paint = true;
    }

    /// Run a full layout pass from the root under `root_constraints`.
    pub fn layout(&mut self, text: &mut TextEnv, root_constraints: BoxConstraints) {
        let Some(root) = self.root else { return };
        let mut object = self.nodes[root].object.take().expect("root object present");
        let size = {
            let mut cx = LayoutCx { tree: self, current: root, text };
            object.layout(&mut cx, root_constraints)
        };
        let node = &mut self.nodes[root];
        node.object = Some(object);
        node.size = size;
        node.offset = Offset::ZERO;
        node.needs_layout = false;
    }

    /// Paint the whole tree into `scene` starting from the root at the origin.
    pub fn paint(&self, scene: &mut vello::Scene) {
        let Some(root) = self.root else { return };
        let mut cx = PaintCx { scene, tree: self, current: root };
        cx.paint_child(root, Offset::ZERO);
    }

    /// Return the render nodes under `point` (window coordinates), ordered from
    /// the root down to the deepest hit. All Pebbles render objects are box-shaped,
    /// so hit testing is a generic rectangle walk rather than a per-object method.
    pub fn hit_test(&self, point: Offset) -> Vec<RenderId> {
        let mut hits = Vec::new();
        if let Some(root) = self.root {
            self.hit_test_node(root, point, Affine::IDENTITY, &mut hits);
        }
        hits
    }

    /// `to_window` maps this node's parent's local space to window space. We carry a
    /// full affine (not just an offset) so a transformed ancestor inverts correctly:
    /// the window `point` is mapped into each node's own local space before the
    /// rectangle test. For untransformed trees this is exactly the old offset walk.
    fn hit_test_node(&self, id: RenderId, point: Offset, to_window: Affine, out: &mut Vec<RenderId>) {
        let node = &self.nodes[id];
        // This node's local frame, then its own paint transform (rotate/scale).
        let mut frame = to_window * Affine::translate((node.offset.x, node.offset.y));
        if let Some(t) = node.object.as_deref().and_then(|o| o.transform(node.size)) {
            frame *= t;
        }
        if frame.determinant().abs() < 1e-9 {
            return; // degenerate transform — nothing is hittable
        }
        let local = frame.inverse() * point.to_point();
        if !Rect::from_origin_size((0.0, 0.0), node.size).contains(local) {
            return;
        }
        out.push(id);
        for &child in &node.children {
            self.hit_test_node(child, point, frame, out);
        }
    }
}

/// Traversal context handed to [`RenderObject::layout`]. It exposes exactly the
/// operations a parent needs — enumerate children, lay a child out under
/// constraints, position it — while owning a `&mut` borrow of the whole tree.
pub struct LayoutCx<'a> {
    tree: &'a mut RenderTree,
    current: RenderId,
    /// Font/layout contexts for text render objects.
    pub text: &'a mut TextEnv,
}

impl LayoutCx<'_> {
    /// The children of the object currently being laid out, in paint order.
    pub fn children(&self) -> SmallVec<[RenderId; 4]> {
        self.tree.nodes[self.current].children.clone()
    }

    pub fn child_count(&self) -> usize {
        self.tree.nodes[self.current].children.len()
    }

    /// Lay `child` out under `constraints`, returning (and recording) its size.
    /// This is the re-entrant call: `child`'s object is lifted out of the arena
    /// for the duration so the recursion holds a hole-free `&mut RenderTree`.
    pub fn layout_child(&mut self, child: RenderId, constraints: BoxConstraints) -> Size {
        let mut object =
            self.tree.nodes[child].object.take().expect("child object present during layout");
        let size = {
            let mut cx = LayoutCx { tree: &mut *self.tree, current: child, text: &mut *self.text };
            object.layout(&mut cx, constraints)
        };
        let node = &mut self.tree.nodes[child];
        node.object = Some(object);
        node.size = size;
        node.needs_layout = false;
        size
    }

    /// Position an already-laid-out `child` relative to the current object's origin.
    pub fn set_child_offset(&mut self, child: RenderId, offset: Offset) {
        self.tree.nodes[child].offset = offset;
    }

    pub fn child_size(&self, child: RenderId) -> Size {
        self.tree.nodes[child].size
    }

    /// Read a child's typed parent data (e.g. a flex factor), if present and of
    /// the requested type.
    pub fn child_parent_data<T: 'static>(&self, child: RenderId) -> Option<&T> {
        self.tree.nodes[child].parent_data.as_ref().and_then(|d| d.downcast_ref::<T>())
    }
}

/// Traversal context handed to [`RenderObject::paint`]. It carries the mutable
/// [`vello::Scene`] being built, a shared borrow of the tree, and the id of the
/// object currently painting (so it can read its own size and children).
pub struct PaintCx<'a> {
    pub scene: &'a mut vello::Scene,
    tree: &'a RenderTree,
    current: RenderId,
}

impl PaintCx<'_> {
    /// The size of the object currently painting (computed during layout).
    pub fn size(&self) -> Size {
        self.tree.nodes[self.current].size
    }

    /// The children of the object currently painting, in paint order.
    pub fn children(&self) -> SmallVec<[RenderId; 4]> {
        self.tree.nodes[self.current].children.clone()
    }

    /// Paint `child` at the given **absolute** window offset. If the child object
    /// declares a `transform`, its subtree is painted into a fresh scene at the
    /// local origin and appended transformed, so rotation/scale affect the whole
    /// subtree (matching the inverse mapping in hit-testing).
    pub fn paint_child(&mut self, child: RenderId, absolute_offset: Offset) {
        let node = &self.tree.nodes[child];
        let Some(object) = node.object.as_deref() else { return };
        match object.transform(node.size) {
            Some(local_t) => {
                let mut sub_scene = vello::Scene::new();
                let mut sub = PaintCx { scene: &mut sub_scene, tree: self.tree, current: child };
                object.paint(&mut sub, Offset::ZERO);
                let placement =
                    Affine::translate((absolute_offset.x, absolute_offset.y)) * local_t;
                self.scene.append(&sub_scene, Some(placement));
            }
            None => {
                let mut sub = PaintCx { scene: &mut *self.scene, tree: self.tree, current: child };
                object.paint(&mut sub, absolute_offset);
            }
        }
    }

    /// The relative offset a child was assigned during layout.
    pub fn child_offset(&self, child: RenderId) -> Offset {
        self.tree.nodes[child].offset
    }

    pub fn child_size(&self, child: RenderId) -> Size {
        self.tree.nodes[child].size
    }
}
