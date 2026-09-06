//! The traversal contexts handed to `RenderObject` — [`LayoutCx`], [`IntrinsicCx`],
//! and [`PaintCx`]. Each owns a borrow of the [`RenderTree`](super::RenderTree) and
//! exposes exactly the operations an object needs (enumerate / lay out / measure / paint
//! children). Split out of `tree.rs` as a CHILD module so it keeps direct access to the
//! tree's private internals while `tree.rs` stays focused on the arena + reconcile.

use super::*;

/// Traversal context handed to [`RenderObject::layout`]. It exposes exactly the
/// operations a parent needs — enumerate children, lay a child out under
/// constraints, position it — while owning a `&mut` borrow of the whole tree.
pub struct LayoutCx<'a> {
    pub(super) tree: &'a mut RenderTree,
    pub(super) current: RenderId,
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
        // Clear any parent-applied transform; a parent that wants one re-sets it with
        // `set_child_transform` after this call (so stale transforms never linger).
        self.tree.nodes[child].layout_transform = None;
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

    /// Apply an affine to `child`'s whole subtree (paint + hit-test), on top of the
    /// child's own [`RenderObject::transform`] — the affine generalization of
    /// [`set_child_offset`](Self::set_child_offset). Cleared before each layout, so
    /// re-set it every pass. Backs `Flow`'s per-child transforms.
    pub fn set_child_transform(&mut self, child: RenderId, transform: Affine) {
        self.tree.nodes[child].layout_transform = Some(transform);
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
    pub(super) tree: &'a mut RenderTree,
    pub(super) current: RenderId,
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
    /// The backend-agnostic drawing surface (see [`crate::paint`]). RenderObjects call
    /// its verbs (`fill`, `stroke`, `push_layer`, `draw_glyphs`, …) and never name a
    /// concrete GPU scene, so the rasterizer is swappable.
    pub scene: crate::paint::Painter<'a>,
    /// Font/layout contexts + the window's shape cache (P5.2): paint-time
    /// shaping access, so lazily materialized text (a huge field's line table)
    /// shapes lines the moment they scroll into view.
    pub text: &'a mut TextEnv,
    pub(super) tree: &'a RenderTree,
    pub(super) current: RenderId,
    /// The window-space rect anything painted from here can possibly show in —
    /// the window rect, narrowed by each clipping ancestor (scroll viewports,
    /// clip-rrects, opacity layers). `paint_child` culls subtrees against it.
    pub(super) visible: Rect,
    /// Corrective-relayout requests (see [`RenderTree::paint`]).
    pub(super) relayout: &'a std::cell::RefCell<SmallVec<[RenderId; 2]>>,
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
        match node_transform(node) {
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
                    scene: self.scene.reborrow(),
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
                let mut sub_scene = crate::paint::scene();
                let mut sub = PaintCx {
                    scene: crate::paint::Painter::new(&mut sub_scene),
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
                    scene: self.scene.reborrow(),
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
    pub fn encode_fragment(&mut self, child: RenderId, fragment: &mut crate::paint::Scene) {
        fragment.reset();
        let mut sub = PaintCx {
            scene: crate::paint::Painter::new(fragment),
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
