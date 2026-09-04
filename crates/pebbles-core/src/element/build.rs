//! Tree building: widget inflation, reconciliation (update-in-place vs replace),
//! unmount/dispose, and the element → render-tree projection. The other half of the
//! [`Ui`] impl.

#[allow(clippy::wildcard_imports)]
use super::*;

impl Ui {
    // ----- inflation -------------------------------------------------------

    pub(super) fn inflate(&mut self, parent: Option<ElementId>, mut widget: AnyWidget) -> ElementId {
        let depth = parent.map(|p| self.elements[p].depth + 1).unwrap_or(0);
        match category(&*widget) {
            Category::Function => {
                let (_, render) = widget.as_component().unwrap();
                let id = self.elements.insert(ElementNode {
                    parent,
                    widget,
                    kind: ElementKind::Function,
                    children: Vec::new(),
                    render_id: None,
                    depth,
                });
                // Run the component with reactive tracking on this element, and keep
                // the guard alive while its child reconciles — render-time contexts
                // (theme overrides, focus scopes) must stay visible to the subtree.
                let child = {
                    let guard = crate::reactive::begin_component(id);
                    let out = render();
                    let child = self.inflate(Some(id), out);
                    crate::reactive::end_component(guard);
                    child
                };
                self.elements[id].children.push(child);
                id
            }
            Category::Render => {
                let render_object = widget.as_render().unwrap().create_render_object();
                let render_id = self.render.insert(render_object);
                let child_widgets = widget.as_render_mut().unwrap().take_children();
                let id = self.elements.insert(ElementNode {
                    parent,
                    widget,
                    kind: ElementKind::Render,
                    children: Vec::new(),
                    render_id: Some(render_id),
                    depth,
                });
                // Tag the render node with its element id for stable gesture identity.
                self.render.set_source(render_id, id.data().as_ffi());
                let mut children = Vec::with_capacity(child_widgets.len());
                for cw in child_widgets {
                    children.push(self.inflate(Some(id), cw));
                }
                self.elements[id].children = children;
                id
            }
            Category::ParentData => {
                let child_widget = widget.as_parent_data_mut().unwrap().take_child();
                let id = self.elements.insert(ElementNode {
                    parent,
                    widget,
                    kind: ElementKind::ParentData,
                    children: Vec::new(),
                    render_id: None,
                    depth,
                });
                if let Some(cw) = child_widget {
                    let child = self.inflate(Some(id), cw);
                    self.elements[id].children.push(child);
                }
                id
            }
        }
    }

    // ----- reconciliation --------------------------------------------------

    /// The core reconcile primitive: given the (optional) existing element and the
    /// (optional) new widget for one slot, return the resulting element id.
    fn update_child(
        &mut self,
        parent: ElementId,
        old: Option<ElementId>,
        new_widget: Option<AnyWidget>,
    ) -> Option<ElementId> {
        match (old, new_widget) {
            (None, None) => None,
            (Some(old), None) => {
                self.unmount(old);
                None
            }
            (None, Some(w)) => Some(self.inflate(Some(parent), w)),
            (Some(old), Some(w)) => {
                if self.can_update(old, &*w) {
                    self.update(old, w);
                    Some(old)
                } else {
                    self.unmount(old);
                    Some(self.inflate(Some(parent), w))
                }
            }
        }
    }

    /// Two widgets are compatible (element reused) iff same concrete type + key —
    /// and, for function components, the same component function.
    fn can_update(&self, old: ElementId, new_widget: &dyn Widget) -> bool {
        let existing = &self.elements[old].widget;
        let same_type =
            (existing.as_any() as &dyn Any).type_id() == (new_widget.as_any() as &dyn Any).type_id();
        if !same_type || existing.key() != new_widget.key() {
            return false;
        }
        match (existing.as_component(), new_widget.as_component()) {
            (Some((a, _)), Some((b, _))) => a == b,
            _ => true,
        }
    }

    fn update(&mut self, id: ElementId, mut new_widget: AnyWidget) {
        match category(&*new_widget) {
            Category::Function => {
                let (_, render) = new_widget.as_component().unwrap();
                self.elements[id].widget = new_widget;
                let old_child = self.elements[id].children.first().copied();
                // Keep the component guard alive across the child reconcile so the
                // render-time contexts this component provides cover its subtree.
                let new_child = {
                    let guard = crate::reactive::begin_component(id);
                    let out = render();
                    let child = self.update_child(id, old_child, Some(out));
                    crate::reactive::end_component(guard);
                    child
                };
                self.elements[id].children = new_child.into_iter().collect();
            }
            Category::Render => {
                let child_widgets = new_widget.as_render_mut().unwrap().take_children();
                if let Some(rid) = self.elements[id].render_id {
                    new_widget.as_render().unwrap().update_render_object(self.render.object_mut(rid));
                }
                self.elements[id].widget = new_widget;
                self.reconcile_children(id, child_widgets);
            }
            Category::ParentData => {
                let child_widget = new_widget.as_parent_data_mut().unwrap().take_child();
                self.elements[id].widget = new_widget;
                let old_child = self.elements[id].children.first().copied();
                let new_child = self.update_child(id, old_child, child_widget);
                self.elements[id].children = new_child.into_iter().collect();
            }
        }
    }

    /// Reconcile a render element's child list against `new_widgets`, matched by
    /// index (keyed reordering is a later refinement).
    fn reconcile_children(&mut self, parent: ElementId, new_widgets: Vec<AnyWidget>) {
        let old_children = self.elements[parent].children.clone();
        let mut incoming: Vec<Option<AnyWidget>> = new_widgets.into_iter().map(Some).collect();
        let count = old_children.len().max(incoming.len());
        let mut result = Vec::with_capacity(count);
        for i in 0..count {
            let old = old_children.get(i).copied();
            let new_widget = incoming.get_mut(i).and_then(|slot| slot.take());
            if let Some(el) = self.update_child(parent, old, new_widget) {
                result.push(el);
            }
        }
        self.elements[parent].children = result;
    }

    pub(super) fn rebuild_element(&mut self, id: ElementId) {
        // Only function components are ever marked dirty (via their reactive signals);
        // render/parent-data elements are reconciled top-down by their parent's rebuild.
        debug_assert!(
            matches!(self.elements[id].kind, ElementKind::Function),
            "a dirty element must be a function component"
        );
        let (_, render) =
            self.elements[id].widget.as_component().expect("a dirty element must be a function component");
        let old_child = self.elements[id].children.first().copied();
        // Guard spans the child reconcile (render-time contexts stay visible to the
        // subtree — same discipline as `update`/`inflate`).
        let new_child = {
            let guard = crate::reactive::begin_component(id);
            let out = render();
            let child = self.update_child(id, old_child, Some(out));
            crate::reactive::end_component(guard);
            child
        };
        self.elements[id].children = new_child.into_iter().collect();
    }

    /// Tear down this window's whole tree: unmount every element — running component
    /// cleanups (animation loops, timeouts, focus/scroll registrations) — and free its
    /// signals from the shared runtime. The shell calls this when the OS window
    /// closes. Without it the closed window's components stay alive in the shared
    /// runtime: any `create_loop` in them pins the frame loop at full rate forever,
    /// and every open/close cycle leaks the whole tree.
    pub fn dispose(&mut self) {
        self.make_current();
        if let Some(root) = self.root.take() {
            self.unmount(root);
        }
        // Drop any re-render requests already queued for this window — nothing will
        // ever drain them again.
        let _ = crate::reactive::take_pending_components(self.ui_id);
        self.dirty.clear();
        self.hovered = None;
        self.scrollbar_drag = None;
        self.scroll_anim.clear();
    }

    fn unmount(&mut self, id: ElementId) {
        let children = std::mem::take(&mut self.elements[id].children);
        for child in children {
            self.unmount(child);
        }
        if let ElementKind::Function = &self.elements[id].kind {
            crate::reactive::dispose_component(id);
            crate::focus::unregister(id);
        }
        if let Some(rid) = self.elements[id].render_id {
            self.render.remove_node(rid);
        }
        self.dirty.retain(|&d| d != id);
        self.elements.remove(id);
    }

    // ----- render-tree projection -----------------------------------------

    /// Re-project the element tree onto the render tree: every render element's
    /// render node gets, as its children, the nearest render descendants of its
    /// child elements — skipping component/stateful elements that own no render
    /// object. O(n) per build; simple and correct.
    pub(super) fn sync_render(&mut self, el: ElementId) {
        if let Some(rid) = self.elements[el].render_id {
            let kids = self.render_children_of(el);
            self.render.set_children(rid, kids);
        }
        for child in self.elements[el].children.clone() {
            self.sync_render(child);
        }
    }

    fn render_children_of(&self, el: ElementId) -> Vec<RenderId> {
        let mut out = Vec::new();
        for &child in &self.elements[el].children {
            if let Some(rid) = self.elements[child].render_id {
                out.push(rid);
            } else {
                out.extend(self.render_children_of(child));
            }
        }
        out
    }

    /// Walk the element tree; for each `ParentData` element, attach its parent data
    /// to its nearest render descendant — the render node that is a direct child of
    /// the enclosing flex/stack, where that container reads it during layout.
    pub(super) fn apply_parent_data(&mut self, el: ElementId) {
        if let Some(pd) = self.elements[el].widget.as_parent_data() {
            let data = pd.parent_data();
            if let Some(rid) = self.render_children_of(el).first().copied() {
                // A `Semantics` widget rides the same parent-data channel but routes to
                // the node's accessibility slot instead of its layout parent-data.
                match data.downcast::<pebbles_render::SemanticsProps>() {
                    Ok(props) => self.render.set_semantics(rid, *props),
                    Err(data) => self.render.set_parent_data(rid, data),
                }
            }
        }
        for child in self.elements[el].children.clone() {
            self.apply_parent_data(child);
        }
    }
}
