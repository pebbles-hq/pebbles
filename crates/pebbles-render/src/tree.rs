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

use pebbles_foundation::{Axis, Offset, Rect, Size};
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
    /// Accessibility annotation attached by a `Semantics` widget, if any.
    pub semantics: Option<crate::SemanticsProps>,
    /// The subtree's paint reach in this node's LOCAL space: this object's
    /// [`paint_bounds`](RenderObject::paint_bounds) unioned with every child's
    /// paint rect (offset), capped at clipping nodes. Recomputed after each
    /// layout pass; the paint-time viewport culling tests against it, so a
    /// shadow that bleeds outside a tight wrapper still gets painted.
    pub paint_rect: Rect,
    /// The constraints of this node's last layout — the layout-skip key: a clean
    /// node re-laid-out under identical constraints returns its stored size
    /// without running `layout` (Flutter's clean-subtree early-out).
    pub last_constraints: Option<BoxConstraints>,
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
            semantics: None,
            paint_rect: Rect::new(0.0, 0.0, 0.0, 0.0),
            last_constraints: None,
            needs_layout: true,
            needs_paint: true,
        }
    }
}

/// Replace any non-finite (∞ / NaN) dimension of a laid-out `size` with a finite
/// value — the finite max constraint if there is one, else 0. A non-finite size
/// is always a layout bug (a fill/stretch inside an unbounded constraint, a
/// divide-by-zero); left alone it becomes a NaN path coordinate that corrupts
/// vello's GPU glyph atlas and hard-panics its CPU renderer. Clamping here — the
/// single chokepoint every node size passes through — makes a layout bug degrade
/// to a visual glitch instead of a crash. In dev mode the `nan_report` tripwire
/// still surfaces the offending widget.
/// Throttle clamp warnings to once per ~3s per widget name (a persistent bug
/// re-clamps every frame otherwise).
fn clamp_should_log(name: &'static str) -> bool {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::time::Duration;

    use web_time::Instant; // std::time::Instant panics on wasm
    thread_local! {
        static SEEN: RefCell<HashMap<&'static str, Instant>> = RefCell::new(HashMap::new());
    }
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        let now = Instant::now();
        match seen.get(name) {
            Some(&t) if now.duration_since(t) < Duration::from_secs(3) => false,
            _ => {
                seen.insert(name, now);
                true
            }
        }
    })
}

fn sanitize_size(size: Size, constraints: BoxConstraints) -> Size {
    fn fix(v: f64, max: f64) -> f64 {
        if v.is_finite() {
            v
        } else if max.is_finite() {
            max
        } else {
            0.0
        }
    }
    Size::new(fix(size.width, constraints.max_width), fix(size.height, constraints.max_height))
}

/// The arena that owns every render object plus the identity of the root.
#[derive(Default)]
pub struct RenderTree {
    nodes: SlotMap<RenderId, RenderNode>,
    pub root: Option<RenderId>,
    /// The root constraints the last `layout()` ran under — so an idle frame
    /// (nothing dirty, same window size) can skip the whole pass.
    last_constraints: Option<BoxConstraints>,
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

    /// Number of live render nodes in this tree (debug observability for the
    /// lifecycle soak test).
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn size_of(&self, id: RenderId) -> Size {
        self.nodes[id].size
    }

    /// The render object's `debug_name` (for the F2 inspector / diagnostics).
    pub fn debug_name(&self, id: RenderId) -> &'static str {
        self.nodes.get(id).and_then(|n| n.object.as_ref()).map(|o| o.debug_name()).unwrap_or("<taken>")
    }

    /// Find the first render node whose object is of type `T`. Handy for tests and
    /// debugging (locate a specific box in a laid-out tree).
    pub fn find<T: RenderObject>(&self) -> Option<RenderId> {
        self.nodes
            .iter()
            .find(|(_, node)| node.object.as_deref().is_some_and(|o| o.is::<T>()))
            .map(|(id, _)| id)
    }

    /// All render nodes whose object is of type `T`, in insertion order. For tests
    /// and tooling that must tell multiple instances apart (or pick, say, the
    /// widest constrained box rather than the first).
    pub fn find_all<T: RenderObject>(&self) -> Vec<RenderId> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.object.as_deref().is_some_and(|o| o.is::<T>()))
            .map(|(id, _)| id)
            .collect()
    }

    /// Dev diagnostic: the first render node whose size or offset is non-finite
    /// (NaN / ∞) — such a value becomes a NaN path coordinate that corrupts the
    /// GPU glyph atlas and hard-panics vello's CPU renderer. Returns the node's
    /// debug name + the bad numbers so the offending widget can be found.
    pub fn nan_report(&self) -> Option<String> {
        fn bad(f: f64) -> bool {
            !f.is_finite()
        }
        // Focus on non-finite SIZE (an ∞ dimension is the root; NaN offsets are
        // just downstream fallout of positioning against ∞). The SOURCE is the
        // deepest node with a bad size whose children all have finite size.
        let size_bad =
            |id: RenderId| self.nodes.get(id).is_some_and(|n| bad(n.size.width) || bad(n.size.height));
        for (_id, node) in self.nodes.iter() {
            if !(bad(node.size.width) || bad(node.size.height)) {
                continue;
            }
            if node.children.iter().any(|&c| size_bad(c)) {
                continue; // a child is the deeper source
            }
            let name = node.object.as_deref().map(|ob| ob.debug_name()).unwrap_or("(lifted)");
            // Walk up to the root so we can see WHICH part of the UI it is.
            let mut chain = vec![name];
            let mut cur = node.parent;
            while let Some(p) = cur {
                let Some(pn) = self.nodes.get(p) else { break };
                chain.push(pn.object.as_deref().map(|ob| ob.debug_name()).unwrap_or("(lifted)"));
                cur = pn.parent;
            }
            chain.reverse();
            return Some(format!(
                "{name}: size {}×{} — SOURCE of the ∞\n    path: {}",
                node.size.width,
                node.size.height,
                chain.join(" › ")
            ));
        }
        None
    }

    /// A human-readable tree dump (debug names + sizes), indented by depth. For
    /// tests and tooling.
    pub fn debug_dump(&self) -> String {
        fn walk(tree: &RenderTree, id: RenderId, depth: usize, out: &mut String) {
            let node = &tree.nodes[id];
            let name = node.object.as_deref().map(|o| o.debug_name()).unwrap_or("(lifted)");
            out.push_str(&format!(
                "{}{} {:.1}×{:.1} @ {:.1},{:.1}\n",
                "  ".repeat(depth),
                name,
                node.size.width,
                node.size.height,
                node.offset.x,
                node.offset.y,
            ));
            for &child in &node.children {
                walk(tree, child, depth + 1, out);
            }
        }
        let mut out = String::new();
        if let Some(root) = self.root {
            walk(self, root, 0, &mut out);
        }
        out
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

    /// Attach an accessibility annotation to a node (set by a `Semantics` widget).
    pub fn set_semantics(&mut self, id: RenderId, props: crate::SemanticsProps) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.semantics = Some(props);
        }
    }

    /// Walk the laid-out tree and collect every semantics-annotated node into a flat
    /// list (each with window-space bounds + the owning element id), in paint order.
    /// The shell maps this onto the platform accessibility tree each frame.
    pub fn semantics_tree(&self) -> Vec<crate::SemanticsNode> {
        let mut out = Vec::new();
        if let Some(root) = self.root {
            let mut synth = 0u64;
            self.collect_semantics(root, &mut out, &mut synth);
        }
        out
    }

    fn collect_semantics(&self, id: RenderId, out: &mut Vec<crate::SemanticsNode>, synth: &mut u64) {
        let node = &self.nodes[id];
        if let Some(props) = &node.semantics {
            let origin = self.absolute_offset(id);
            let nid = node.source.unwrap_or_else(|| {
                *synth += 1;
                // Synthesized ids live above any real element id space (fits usize::MAX
                // element ids in the low bits); fine for untagged decorative nodes.
                u64::MAX - *synth
            });
            out.push(crate::SemanticsNode {
                id: nid,
                props: props.clone(),
                bounds: Rect::from_origin_size((origin.x, origin.y), (node.size.width, node.size.height)),
            });
        }
        for &child in &node.children {
            self.collect_semantics(child, out, synth);
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
            // Stale ids reach here (scroll springs / animations can outlive the
            // node they drive by a frame) — a dirty-mark on a dead node is a
            // no-op, never a panic.
            let Some(node) = self.nodes.get_mut(c) else { return };
            // A repaint boundary on the dirty path must re-encode its fragment.
            // (Ancestors above an already-dirty node were flagged when IT was
            // first marked, so the early break below stays sound.)
            if let Some(b) =
                node.object.as_deref_mut().and_then(|o| o.downcast_mut::<crate::objects::RenderBoundary>())
            {
                b.mark_dirty();
            }
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

    /// Mark `id` as needing paint, walking up and invalidating the nearest
    /// enclosing repaint boundary's retained fragment (paint-only changes —
    /// scroll offsets, recolors — must re-encode the fragment they live in).
    pub fn mark_needs_paint(&mut self, id: RenderId) {
        let mut cur = Some(id);
        while let Some(c) = cur {
            let Some(node) = self.nodes.get_mut(c) else { return };
            node.needs_paint = true;
            if let Some(b) =
                node.object.as_deref_mut().and_then(|o| o.downcast_mut::<crate::objects::RenderBoundary>())
            {
                b.mark_dirty();
                return; // contained: outer composition re-appends fragments anyway
            }
            cur = node.parent;
        }
    }

    /// Re-position a scroll viewport's (single, clipped) content child without a
    /// layout pass — the paint-time half of "scrolling is paint, not layout".
    /// The child keeps its laid-out size; only its offset inside the clipping
    /// viewport moves, which is exactly what paint and hit-testing read.
    pub fn set_scrolled_child_offset(&mut self, viewport: RenderId, offset: Offset) {
        let Some(&child) = self.nodes.get(viewport).and_then(|n| n.children.first()) else {
            return;
        };
        if let Some(node) = self.nodes.get_mut(child) {
            node.offset = offset;
        }
    }

    /// Run a layout pass from the root under `root_constraints`.
    ///
    /// Skips entirely when the tree is clean (`root.needs_layout == false`, which
    /// — because `mark_needs_layout` propagates to the root — means NO node needs
    /// layout) AND the window size is unchanged. This makes idle/paint-only frames
    /// (a blinking caret, a hover fade) free instead of re-laying-out the whole
    /// tree, which for a large document was tens of ms every frame. A structural
    /// change or resize clears the skip because it dirties the root or changes the
    /// constraints.
    pub fn layout(&mut self, text: &mut TextEnv, root_constraints: BoxConstraints) {
        let Some(root) = self.root else { return };
        if !self.nodes[root].needs_layout && self.last_constraints == Some(root_constraints) {
            return;
        }
        let mut object = self.nodes[root].object.take().expect("root object present");
        let size = {
            let mut cx = LayoutCx { tree: self, current: root, text };
            object.layout(&mut cx, root_constraints)
        };
        let node = &mut self.nodes[root];
        node.object = Some(object);
        node.size = sanitize_size(size, root_constraints);
        node.offset = Offset::ZERO;
        node.needs_layout = false;
        self.last_constraints = Some(root_constraints);
        self.recompute_paint_rects(root);
    }

    /// Recompute every node's subtree paint rect (bottom-up) after a layout pass:
    /// own `paint_bounds` ∪ each child's paint rect at its offset (transform-aware),
    /// capped at clipping nodes. O(n), but only runs when layout actually ran.
    fn recompute_paint_rects(&mut self, id: RenderId) -> Rect {
        let children = self.nodes[id].children.clone();
        let (mut acc, clips) = {
            let node = &self.nodes[id];
            match node.object.as_deref() {
                Some(o) => (o.paint_bounds(node.size), o.clips_children()),
                None => (Rect::from_origin_size((0.0, 0.0), node.size), false),
            }
        };
        for child in children {
            let r = self.recompute_paint_rects(child);
            if clips {
                continue; // children paint inside this node's own bounds anyway
            }
            let cnode = &self.nodes[child];
            let r = match cnode.object.as_deref().and_then(|o| o.transform(cnode.size)) {
                Some(t) => t.transform_rect_bbox(r),
                None => r,
            };
            let off = cnode.offset;
            acc = acc.union(Rect::new(r.x0 + off.x, r.y0 + off.y, r.x1 + off.x, r.y1 + off.y));
        }
        self.nodes[id].paint_rect = acc;
        acc
    }

    /// Paint the whole tree into `scene` starting from the root at the origin.
    /// The visible window (viewport culling) starts as the root's own rect.
    ///
    /// `text` gives paint-time shaping access (P5.2): a huge text field
    /// materializes only the lines that scroll into view. Returns the nodes that
    /// requested a **corrective relayout** (a lazy measurement changed a size the
    /// last layout pass estimated) — the caller marks them dirty and schedules
    /// one more frame, the ListView estimate-then-measure pattern.
    pub fn paint(&self, text: &mut TextEnv, scene: &mut vello::Scene) -> SmallVec<[RenderId; 2]> {
        let Some(root) = self.root else { return SmallVec::new() };
        let relayout = std::cell::RefCell::new(SmallVec::new());
        let visible = Rect::from_origin_size((0.0, 0.0), self.nodes[root].size);
        let mut cx = PaintCx { scene, text, tree: self, current: root, visible, relayout: &relayout };
        cx.paint_child(root, Offset::ZERO);
        relayout.into_inner()
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
        // Clean-subtree early-out: nothing below this node is dirty (dirt marks
        // propagate to the root) and the constraints are byte-identical to the
        // last pass — the stored size is still the answer. This is what makes a
        // local change O(dirty path) instead of O(tree).
        {
            let node = &self.tree.nodes[child];
            if !node.needs_layout && node.last_constraints == Some(constraints) {
                crate::stats::bump_layout_skip();
                return node.size;
            }
        }
        crate::stats::bump_layout();
        let mut object = self.tree.nodes[child].object.take().expect("child object present during layout");
        let size = {
            let mut cx = LayoutCx { tree: &mut *self.tree, current: child, text: &mut *self.text };
            object.layout(&mut cx, constraints)
        };
        // A non-finite (∞/NaN) size is always a bug — a widget filled an unbounded
        // constraint (e.g. a fill/stretch inside a scroll view) or divided by zero.
        // It would become a NaN path coordinate that corrupts the GPU glyph atlas
        // and hard-panics vello's CPU renderer, so clamp it here at the one
        // chokepoint every child size passes through. The clamp target is the
        // finite max constraint, else 0.
        let clean = sanitize_size(size, constraints);
        if clean != size && pebbles_foundation::log::dev_mode() {
            let name = object.debug_name();
            // The deepest node lays out first, so the first clamp of a frame names
            // the SOURCE widget. Throttled so it doesn't spam every frame.
            if clamp_should_log(name) {
                pebbles_foundation::log::warn(
                    pebbles_foundation::log::Cat::Layout,
                    format!(
                        "clamped non-finite size on {name}: {}×{} → {}×{} (a fill/stretch in an \
                         unbounded constraint, or a divide-by-zero — fix the widget's sizing)",
                        size.width, size.height, clean.width, clean.height
                    ),
                );
            }
        }
        let size = clean;
        let node = &mut self.tree.nodes[child];
        node.object = Some(object);
        node.size = size;
        node.needs_layout = false;
        node.last_constraints = Some(constraints);
        size
    }

    /// Ask `child` for its intrinsic extent on `axis` (see
    /// [`RenderObject::intrinsic`]), with `cross_extent` fixed on the perpendicular
    /// axis. The intrinsic-objects ([`crate::objects::RenderIntrinsicWidth`]) drive layout from
    /// this; ordinary parents rarely need it.
    pub fn child_intrinsic(&mut self, child: RenderId, axis: Axis, cross_extent: f64) -> Option<f64> {
        let object =
            self.tree.nodes[child].object.take().expect("child object present during intrinsic measurement");
        let result = {
            let mut cx = IntrinsicCx { tree: &mut *self.tree, current: child, text: &mut *self.text };
            object.intrinsic(&mut cx, axis, cross_extent)
        };
        let node = &mut self.tree.nodes[child];
        node.object = Some(object);
        result
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

    /// Ask `child` for its first text baseline, in this parent's coordinate space
    /// (the child's own baseline plus its top offset). See
    /// [`RenderObject::baseline`].
    pub fn child_baseline(&mut self, child: RenderId) -> Option<f64> {
        let offset = self.tree.nodes[child].offset;
        let object = self.tree.nodes[child].object.take().expect("child object present");
        let result = {
            let mut cx = LayoutCx { tree: &mut *self.tree, current: child, text: &mut *self.text };
            object.baseline(&mut cx)
        };
        let node = &mut self.tree.nodes[child];
        node.object = Some(object);
        result.map(|b| b + offset.y)
    }
}

/// Traversal context handed to [`RenderObject::intrinsic`]. Mirrors [`LayoutCx`]'s
/// arena discipline: a parent asks for its children's intrinsic extents through
/// the context, which lifts each child's object out of the arena for the duration.
pub struct IntrinsicCx<'a> {
    tree: &'a mut RenderTree,
    current: RenderId,
    /// Font/layout contexts for text render objects.
    pub text: &'a mut TextEnv,
}

impl IntrinsicCx<'_> {
    /// The children of the object currently being measured, in paint order.
    pub fn children(&self) -> SmallVec<[RenderId; 4]> {
        self.tree.nodes[self.current].children.clone()
    }

    /// Ask `child` for its intrinsic extent on `axis`, given `cross_extent` fixed
    /// on the perpendicular axis (infinite when unconstrained). `None` when the
    /// child has no intrinsic notion.
    pub fn child_intrinsic(&mut self, child: RenderId, axis: Axis, cross_extent: f64) -> Option<f64> {
        let object = self.tree.nodes[child].object.take().expect("child object present during intrinsics");
        let result = {
            let mut cx = IntrinsicCx { tree: &mut *self.tree, current: child, text: &mut *self.text };
            object.intrinsic(&mut cx, axis, cross_extent)
        };
        let node = &mut self.tree.nodes[child];
        node.object = Some(object);
        result
    }
}

/// Traversal context handed to [`RenderObject::paint`]. It carries the mutable
/// [`vello::Scene`] being built, a shared borrow of the tree, and the id of the
/// object currently painting (so it can read its own size and children).
pub struct PaintCx<'a> {
    pub scene: &'a mut vello::Scene,
    /// Font/layout contexts + the window's shape cache (P5.2): paint-time
    /// shaping access, so lazily materialized text (a huge field's line table)
    /// shapes lines the moment they scroll into view.
    pub text: &'a mut TextEnv,
    tree: &'a RenderTree,
    current: RenderId,
    /// The window-space rect anything painted from here can possibly show in —
    /// the window rect, narrowed by each clipping ancestor (scroll viewports,
    /// clip-rrects, opacity layers). `paint_child` culls subtrees against it.
    visible: Rect,
    /// Corrective-relayout requests (see [`RenderTree::paint`]).
    relayout: &'a std::cell::RefCell<SmallVec<[RenderId; 2]>>,
}

/// Strict rect overlap — an empty or exactly-touching intersection is a miss.
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// Whether an affine is a pure translation (identity linear part).
fn is_translation(t: Affine) -> bool {
    let c = t.as_coeffs();
    c[0] == 1.0 && c[1] == 0.0 && c[2] == 0.0 && c[3] == 1.0
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

    /// The visible window (in window space) painting is currently culled against.
    pub fn visible(&self) -> Rect {
        self.visible
    }

    /// Request a **corrective relayout** for the object currently painting: a
    /// lazy paint-time measurement (P5.2) changed geometry the last layout pass
    /// only estimated. The node is marked `needs_layout` after this paint and
    /// one more frame is scheduled — the estimate-then-measure settle loop.
    pub fn request_relayout(&mut self) {
        self.relayout.borrow_mut().push(self.current);
    }

    /// Paint `child` at the given **absolute** window offset. If the child object
    /// declares a `transform`, its subtree is painted into a fresh scene at the
    /// local origin and appended transformed, so rotation/scale affect the whole
    /// subtree (matching the inverse mapping in hit-testing).
    ///
    /// This is the ONE viewport-culling chokepoint: a subtree whose
    /// [`paint_bounds`](RenderObject::paint_bounds) cannot intersect the visible
    /// window is skipped entirely — nothing offscreen is ever encoded into the
    /// scene. Each child is judged by its OWN bounds (children may overflow their
    /// parent); a pure-translation transform folds into the offset with no
    /// sub-scene cost.
    pub fn paint_child(&mut self, child: RenderId, absolute_offset: Offset) {
        let node = &self.tree.nodes[child];
        let Some(object) = node.object.as_deref() else { return };
        let local = node.paint_rect;
        match object.transform(node.size) {
            // Pure translation: no sub-scene, no bbox math — fold into the offset.
            Some(t) if is_translation(t) => {
                let c = t.as_coeffs();
                let at = absolute_offset + Offset::new(c[4], c[5]);
                let world = Rect::new(local.x0 + at.x, local.y0 + at.y, local.x1 + at.x, local.y1 + at.y);
                if !overlaps(world, self.visible) {
                    crate::stats::bump_culled();
                    return;
                }
                crate::stats::bump_painted();
                let mut sub = PaintCx {
                    scene: &mut *self.scene,
                    text: &mut *self.text,
                    tree: self.tree,
                    current: child,
                    visible: self.visible,
                    relayout: self.relayout,
                };
                object.paint(&mut sub, at);
            }
            Some(local_t) => {
                let placement = Affine::translate((absolute_offset.x, absolute_offset.y)) * local_t;
                if !overlaps(placement.transform_rect_bbox(local), self.visible) {
                    crate::stats::bump_culled();
                    return;
                }
                crate::stats::bump_painted();
                // Map the visible window into the subtree's local space so nested
                // culling keeps working under the transform; a degenerate matrix
                // disables culling for the subtree rather than mis-culling it.
                let local_visible = if placement.determinant().abs() > 1e-9 {
                    placement.inverse().transform_rect_bbox(self.visible)
                } else {
                    Rect::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY)
                };
                let mut sub_scene = vello::Scene::new();
                let mut sub = PaintCx {
                    scene: &mut sub_scene,
                    text: &mut *self.text,
                    tree: self.tree,
                    current: child,
                    visible: local_visible,
                    relayout: self.relayout,
                };
                object.paint(&mut sub, Offset::ZERO);
                self.scene.append(&sub_scene, Some(placement));
            }
            None => {
                let world = Rect::new(
                    local.x0 + absolute_offset.x,
                    local.y0 + absolute_offset.y,
                    local.x1 + absolute_offset.x,
                    local.y1 + absolute_offset.y,
                );
                if !overlaps(world, self.visible) {
                    crate::stats::bump_culled();
                    return;
                }
                crate::stats::bump_painted();
                let mut sub = PaintCx {
                    scene: &mut *self.scene,
                    text: &mut *self.text,
                    tree: self.tree,
                    current: child,
                    visible: self.visible,
                    relayout: self.relayout,
                };
                object.paint(&mut sub, absolute_offset);
            }
        }
    }

    /// Like [`paint_child`](Self::paint_child), but narrows the visible window to
    /// `clip` for the child's whole subtree. Clipping render objects (scroll
    /// viewports, clip-rrects, opacity layers) call this alongside their vello
    /// clip layer so culling matches what the layer would clip away anyway.
    pub fn paint_child_clipped(&mut self, child: RenderId, absolute_offset: Offset, clip: Rect) {
        let saved = self.visible;
        self.visible = Rect::new(
            saved.x0.max(clip.x0),
            saved.y0.max(clip.y0),
            saved.x1.min(clip.x1),
            saved.y1.min(clip.y1),
        );
        self.paint_child(child, absolute_offset);
        self.visible = saved;
    }

    /// Encode `child`'s subtree into `fragment` at the LOCAL origin with an
    /// unbounded visible window. Repaint boundaries call this: fragments must be
    /// viewport-INDEPENDENT so re-appending them at any scroll offset is sound.
    pub fn encode_fragment(&mut self, child: RenderId, fragment: &mut vello::Scene) {
        fragment.reset();
        let mut sub = PaintCx {
            scene: fragment,
            text: &mut *self.text,
            tree: self.tree,
            current: child,
            visible: Rect::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::INFINITY),
            relayout: self.relayout,
        };
        sub.paint_child(child, Offset::ZERO);
    }

    /// The relative offset a child was assigned during layout.
    pub fn child_offset(&self, child: RenderId) -> Offset {
        self.tree.nodes[child].offset
    }

    pub fn child_size(&self, child: RenderId) -> Size {
        self.tree.nodes[child].size
    }
}
