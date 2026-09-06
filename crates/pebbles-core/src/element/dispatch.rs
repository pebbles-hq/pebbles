//! Input dispatch: hit-testing, pointer/tap/pan/long-press gestures, hover,
//! keyboard routing, wheel + scrollbar + drag-to-scroll handling. One half of the
//! [`Ui`] impl — state lives on [`Ui`] in the parent module.

#[allow(clippy::wildcard_imports)]
use super::*;

/// The pointer's position along a scroll view's axis (for drag-to-scroll).
fn drag_axis_pos(axis: Axis, point: Offset) -> f64 {
    match axis {
        Axis::Vertical => point.y,
        Axis::Horizontal => point.x,
    }
}

impl Ui {
    // ----- input -----------------------------------------------------------

    /// Resolve every callback in the `pick`ed slot of a listener into runnable form.
    fn invokes_of(
        listener: &RenderPointerListener,
        pick: impl Fn(&RenderPointerListener) -> &[pebbles_render::TapCallback],
    ) -> Vec<Invoke> {
        pick(listener)
            .iter()
            .filter_map(|any| any.downcast_ref::<Callback>())
            .map(|cb| match cb {
                Callback::Plain(f) => Invoke::Plain(f.clone()),
                Callback::Event(f) => Invoke::Event(f.clone()),
            })
            .collect()
    }

    fn run_invoke(&mut self, invoke: Invoke, event: PointerEvent) {
        match invoke {
            Invoke::Plain(f) => f(),
            Invoke::Event(f) => f(event),
        }
    }

    /// Fire every callback in the topmost listener that has any for the picked event,
    /// passing a [`PointerEvent`] with the position in that widget's local space.
    fn fire_pointer(
        &mut self,
        point: Offset,
        button: PointerButton,
        pick: impl Fn(&RenderPointerListener) -> &[pebbles_render::TapCallback],
    ) -> bool {
        let hits = self.render.hit_test(point); // ordered root -> leaf
        for &rid in hits.iter().rev() {
            let invokes = self
                .render
                .object_ref(rid)
                .downcast_ref::<RenderPointerListener>()
                .map(|l| Self::invokes_of(l, &pick))
                .unwrap_or_default();
            if !invokes.is_empty() {
                let local = point - self.render.absolute_offset(rid);
                let event = PointerEvent { position: local, global: point, button, delta: Offset::ZERO };
                for invoke in invokes {
                    self.run_invoke(invoke, event);
                }
                return true;
            }
        }
        false
    }

    /// A primary-button tap at `point`. Returns `true` if handled.
    pub fn dispatch_tap(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_tap)
    }

    /// A primary-button double tap at `point`.
    pub fn dispatch_double_tap(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_double_tap)
    }

    /// A secondary-button (right-click) tap at `point`.
    pub fn dispatch_secondary_tap(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Secondary, |l| &l.on_secondary_tap)
    }

    /// Secondary button pressed down at `point`.
    pub fn dispatch_secondary_tap_down(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Secondary, |l| &l.on_secondary_tap_down)
    }

    /// Secondary button released at `point`.
    pub fn dispatch_secondary_tap_up(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Secondary, |l| &l.on_secondary_tap_up)
    }

    /// A long-press at `point` (button held past the long-press interval).
    pub fn dispatch_long_press(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_long_press)
    }

    /// Primary button pressed down at `point`.
    pub fn dispatch_pointer_down(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_pointer_down)
    }

    /// Primary button released at `point`.
    pub fn dispatch_pointer_up(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Primary, |l| &l.on_pointer_up)
    }

    /// The source id of the topmost primary-tap-family listener under `point` (tap
    /// or double-tap) — used to arm a press so a release elsewhere becomes a cancel,
    /// and so a double-tap-only widget (no `on_tap`) still receives its event.
    pub fn tap_target_at(&self, point: Offset) -> Option<u64> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            let l = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            if l.on_tap.is_empty() && l.on_double_tap.is_empty() { None } else { self.render.source_of(rid) }
        })
    }

    /// Fire a picked event slot on the listener with the given source id, computing
    /// the [`PointerEvent`] in that widget's local space. Used for gestures that stay
    /// bound to their original target after the pointer moves (cancel, long-press).
    fn fire_source(
        &mut self,
        source: u64,
        point: Offset,
        button: PointerButton,
        pick: impl Fn(&RenderPointerListener) -> &[pebbles_render::TapCallback],
    ) -> bool {
        let Some(rid) = self.render.find_by_source(source) else { return false };
        let invokes = self
            .render
            .object_ref(rid)
            .downcast_ref::<RenderPointerListener>()
            .map(|l| Self::invokes_of(l, &pick))
            .unwrap_or_default();
        if invokes.is_empty() {
            return false;
        }
        let local = point - self.render.absolute_offset(rid);
        let event = PointerEvent { position: local, global: point, button, delta: Offset::ZERO };
        for invoke in invokes {
            self.run_invoke(invoke, event);
        }
        true
    }

    /// Fire `on_tap_cancel` on the armed target.
    pub fn dispatch_tap_cancel(&mut self, source: u64) -> bool {
        self.fire_source(source, Offset::ZERO, PointerButton::Primary, |l| &l.on_tap_cancel)
    }

    /// The source id of the topmost long-press listener under `point`.
    pub fn long_press_target_at(&self, point: Offset) -> Option<u64> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            let l = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            let wants = !l.on_long_press.is_empty()
                || !l.on_long_press_down.is_empty()
                || !l.on_long_press_start.is_empty()
                || !l.on_long_press_move.is_empty()
                || !l.on_long_press_up.is_empty()
                || !l.on_long_press_end.is_empty()
                || !l.on_long_press_cancel.is_empty();
            if wants { self.render.source_of(rid) } else { None }
        })
    }

    /// Long press: pointer contacted (may begin a long press).
    pub fn dispatch_long_press_down(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_down)
    }
    /// Long press recognized: fires `on_long_press` + `on_long_press_start`.
    pub fn dispatch_long_press_begin(&mut self, source: u64, point: Offset) -> bool {
        let a = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press);
        let b = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_start);
        a | b
    }
    /// Pointer moved during a long press.
    pub fn dispatch_long_press_move(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_move)
    }
    /// Long press ended: fires `on_long_press_up` + `on_long_press_end`.
    pub fn dispatch_long_press_end(&mut self, source: u64, point: Offset) -> bool {
        let a = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_up);
        let b = self.fire_source(source, point, PointerButton::Primary, |l| &l.on_long_press_end);
        a | b
    }
    /// A pending long press was cancelled.
    pub fn dispatch_long_press_cancel(&mut self, source: u64) -> bool {
        self.fire_source(source, Offset::ZERO, PointerButton::Primary, |l| &l.on_long_press_cancel)
    }

    /// The source id of the topmost drag (pan) listener under `point`.
    pub fn pan_target_at(&self, point: Offset) -> Option<u64> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            let l = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            if l.wants_pan() { self.render.source_of(rid) } else { None }
        })
    }

    /// The source id of the topmost axis-drag listener under `point` (the
    /// mutually-exclusive alternative to [`Ui::pan_target_at`](crate::Ui::pan_target_at)).
    pub fn axis_pan_target_at(&self, point: Offset) -> Option<u64> {
        let hits = self.render.hit_test(point);
        hits.iter().rev().find_map(|&rid| {
            let l = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            if l.wants_axis_drag() { self.render.source_of(rid) } else { None }
        })
    }

    /// A drag began on the target (primary press).
    pub fn dispatch_pan_start(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_pan_start)
    }
    /// The pointer moved during an active drag.
    pub fn dispatch_pan_update(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_pan_update)
    }
    /// The drag ended (primary released).
    pub fn dispatch_pan_end(&mut self, source: u64, point: Offset) -> bool {
        self.fire_source(source, point, PointerButton::Primary, |l| &l.on_pan_end)
    }

    // ----- content drag (drag-to-scroll viewports) -------------------------

    /// Whether a content drag is currently in progress (began with
    /// [`begin_content_drag`](Self::begin_content_drag)).
    pub fn content_drag_active(&self) -> bool {
        self.content_drag.is_some()
    }

    /// Claim a pointer drag as a **content drag** on a drag-scroll viewport under
    /// `point`. Arbitration: a pan-hungry descendant at the point wins — the
    /// viewport only claims drags nothing else wants (A4).
    pub fn begin_content_drag(&mut self, point: Offset) -> bool {
        if self.content_drag.is_some() || self.pan_target_at(point).is_some() {
            return false;
        }
        let hits = self.render.hit_test(point);
        let rid = hits.iter().rev().find_map(|&rid| {
            let s = self.render.object_ref(rid).downcast_ref::<RenderScroll>()?;
            s.drag_scroll.then_some(rid)
        });
        let Some(rid) = rid else { return false };
        let now = self.clock_now();
        let (at, claimed) = self
            .render
            .object_mut(rid)
            .downcast_mut::<RenderScroll>()
            .map(|s| (drag_axis_pos(s.axis, point), s.drag_begin(drag_axis_pos(s.axis, point), now)))
            .unwrap_or((0.0, false));
        let _ = at;
        if claimed {
            self.content_drag = Some(rid);
        }
        claimed
    }

    /// The pointer moved during a content drag: moves the content 1:1 (the
    /// rubber-band math lives in [`RenderScroll::drag_move`]) and fires any
    /// pull-to-refresh arm trigger (A5).
    pub fn update_content_drag(&mut self, point: Offset) -> bool {
        let Some(rid) = self.content_drag else { return false };
        let now = self.clock_now();
        // The scroll view may have unmounted mid-drag (navigation, overlay close)
        // — drop the stale drag instead of indexing a freed node.
        let Some(moved) =
            self.render.try_object_mut(rid).and_then(|o| o.downcast_mut::<RenderScroll>()).map(|s| {
                let at = drag_axis_pos(s.axis, point);
                let moved = s.drag_move(at, now);
                s.refresh_update();
                moved
            })
        else {
            self.content_drag = None;
            return false;
        };
        if moved {
            self.scroll_moved(rid);
        }
        moved
    }

    /// End the content drag: estimates the fling velocity and lets the spring
    /// settle (or snap back from overscroll). An armed pull-to-refresh fires its
    /// release callback here. Frames keep ticking while the spring settles.
    pub fn end_content_drag(&mut self, point: Offset) -> bool {
        let Some(rid) = self.content_drag.take() else { return false };
        let now = self.clock_now();
        // Unmounted mid-drag → nothing to settle; the drag is simply over.
        let ended = self
            .render
            .try_object_mut(rid)
            .and_then(|o| o.downcast_mut::<RenderScroll>())
            .map(|s| {
                let at = drag_axis_pos(s.axis, point);
                s.drag_move(at, now);
                s.refresh_end();
                s.drag_end(now)
            })
            .unwrap_or(false);
        if ended {
            self.scroll_anim.insert(rid);
            self.scroll_moved(rid);
        }
        ended
    }

    /// Monotonic seconds for drag velocity estimation. Production uses the wall
    /// clock; tests fix it with [`set_test_clock`](Self::set_test_clock) so fling
    /// velocities are deterministic.
    fn clock_now(&self) -> f64 {
        if let Some(t) = self.clock_override {
            t
        } else {
            // web_time, not std::time: SystemTime::now() panics on wasm.
            web_time::SystemTime::now()
                .duration_since(web_time::UNIX_EPOCH)
                .map(|d| d.as_secs_f64())
                .unwrap_or(0.0)
        }
    }

    /// Fix the drag clock to `t` (test-only): drag/fling velocity estimates then
    /// use exactly this time base. Pass `None` to restore the wall clock.
    #[doc(hidden)]
    pub fn set_test_clock(&mut self, t: Option<f64>) {
        self.clock_override = t;
    }

    /// Fire an axis-drag event with a movement `delta` in the event.
    fn fire_source_delta(
        &mut self,
        source: u64,
        point: Offset,
        delta: Offset,
        pick: impl Fn(&RenderPointerListener) -> &[pebbles_render::TapCallback],
    ) -> bool {
        let Some(rid) = self.render.find_by_source(source) else { return false };
        let invokes = self
            .render
            .object_ref(rid)
            .downcast_ref::<RenderPointerListener>()
            .map(|l| Self::invokes_of(l, &pick))
            .unwrap_or_default();
        if invokes.is_empty() {
            return false;
        }
        let local = point - self.render.absolute_offset(rid);
        let event = PointerEvent { position: local, global: point, button: PointerButton::Primary, delta };
        for invoke in invokes {
            self.run_invoke(invoke, event);
        }
        true
    }

    /// A vertical drag began (vertical axis won the slop).
    pub fn dispatch_vertical_drag_start(&mut self, source: u64, point: Offset, delta: Offset) -> bool {
        self.fire_source_delta(source, point, delta, |l| &l.on_vertical_drag_start)
    }
    /// Pointer moved during a vertical drag.
    pub fn dispatch_vertical_drag_update(&mut self, source: u64, point: Offset, delta: Offset) -> bool {
        self.fire_source_delta(source, point, delta, |l| &l.on_vertical_drag_update)
    }
    /// The vertical drag ended.
    pub fn dispatch_vertical_drag_end(&mut self, source: u64, point: Offset, delta: Offset) -> bool {
        self.fire_source_delta(source, point, delta, |l| &l.on_vertical_drag_end)
    }
    /// A horizontal drag began (horizontal axis won the slop).
    pub fn dispatch_horizontal_drag_start(&mut self, source: u64, point: Offset, delta: Offset) -> bool {
        self.fire_source_delta(source, point, delta, |l| &l.on_horizontal_drag_start)
    }
    /// Pointer moved during a horizontal drag.
    pub fn dispatch_horizontal_drag_update(&mut self, source: u64, point: Offset, delta: Offset) -> bool {
        self.fire_source_delta(source, point, delta, |l| &l.on_horizontal_drag_update)
    }
    /// The horizontal drag ended.
    pub fn dispatch_horizontal_drag_end(&mut self, source: u64, point: Offset, delta: Offset) -> bool {
        self.fire_source_delta(source, point, delta, |l| &l.on_horizontal_drag_end)
    }

    /// Tertiary (middle) button pressed at `point`.
    pub fn dispatch_tertiary_down(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Middle, |l| &l.on_tertiary_tap_down)
    }
    /// Tertiary (middle) button released at `point`.
    pub fn dispatch_tertiary_up(&mut self, point: Offset) -> bool {
        self.fire_pointer(point, PointerButton::Middle, |l| &l.on_tertiary_tap_up)
    }

    /// Update hover state for the pointer at `point`, firing enter/exit callbacks as
    /// the topmost hover-listener changes. Identity is by element id (stable across
    /// rebuilds); exit actions are stored on enter so they fire reliably even after
    /// the widget restyles itself. A pointer staying over the same widget is a no-op.
    pub fn dispatch_hover(&mut self, point: Offset) -> bool {
        let hits = self.render.hit_test(point);
        let found: Option<(u64, Vec<Invoke>, Vec<Invoke>)> = hits.iter().rev().find_map(|&rid| {
            let listener = self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?;
            if !listener.wants_hover() {
                return None;
            }
            let source = self.render.source_of(rid)?;
            let enters = Self::invokes_of(listener, |l| &l.on_enter);
            let exits = Self::invokes_of(listener, |l| &l.on_exit);
            Some((source, enters, exits))
        });

        let new_key = found.as_ref().map(|(s, _, _)| *s);
        let old_key = self.hovered.as_ref().map(|h| h.source);
        if new_key == old_key {
            return false; // still over the same widget
        }

        let hover_event = PointerEvent {
            position: point,
            global: point,
            button: PointerButton::Primary,
            delta: Offset::ZERO,
        };
        let mut fired = false;
        if let Some(old) = self.hovered.take() {
            // Only fire the previously-hovered widget's exit handlers if its element
            // still exists. If it unmounted while hovered (e.g. a click swapped the
            // panel out from under the cursor), its handler closures capture now-freed
            // signals — invoking them would use-after-free. Keying by the stable source
            // (not the render id) means a mere re-render that reassigned render ids
            // still counts as "exists", so exit fires correctly.
            if self.render.find_by_source(old.source).is_some() {
                for invoke in old.exits {
                    self.run_invoke(invoke, hover_event);
                    fired = true;
                }
            }
        }
        if let Some((source, enters, exits)) = found {
            for invoke in enters {
                self.run_invoke(invoke, hover_event);
                fired = true;
            }
            self.hovered = Some(HoverTarget { source, exits });
        }
        fired
    }

    /// The cursor icon the topmost hover-listener under `point` requests, if any.
    pub fn cursor_at(&self, point: Offset) -> Option<pebbles_render::Cursor> {
        let hits = self.render.hit_test(point);
        hits.iter()
            .rev()
            .find_map(|&rid| self.render.object_ref(rid).downcast_ref::<RenderPointerListener>()?.cursor)
    }

    /// Activate the focused widget (Enter/Space). Returns whether handled.
    pub fn dispatch_activate(&mut self) -> bool {
        crate::focus::activate_focused()
    }

    /// Route a keyboard edit intent to the focused text editor. Returns whether an
    /// editor consumed it.
    pub fn dispatch_key(&mut self, key: crate::keyboard::KeyInput) -> bool {
        crate::focus::dispatch_key(key)
    }

    /// Whether the focused node is a text editor (shell key-routing precedence).
    pub fn focused_is_editor(&self) -> bool {
        crate::focus::focused_is_editor()
    }

    /// Move keyboard focus to the next (`forward`) or previous focusable (Tab).
    pub fn focus_move(&mut self, forward: bool) -> bool {
        crate::focus::focus_move(self.ui_id, forward)
    }

    /// Scroll the topmost scrollable under `point` by `delta` (logical px). Returns
    /// `true` if an offset actually changed (caller should relayout + redraw).
    ///
    /// The wheel arrives as a vertical delta, so a vertical scrollable is preferred:
    /// we bubble PAST an axis-mismatched scrollable (e.g. a horizontal carousel) rather
    /// than let it steal a vertical wheel. Only if no vertical scrollable takes it do we
    /// fall back to any axis, so a lone horizontal list still wheels when it's the only
    /// option.
    pub fn dispatch_scroll(&mut self, point: Offset, delta: f64) -> bool {
        self.scroll_pass(point, delta, Some(Axis::Vertical)) || self.scroll_pass(point, delta, None)
    }

    /// One hit-test walk (leaf → root) applying `delta` to the first scrollable that
    /// matches `want` (any axis when `None`) and isn't already pinned at its edge.
    fn scroll_pass(&mut self, point: Offset, delta: f64, want: Option<Axis>) -> bool {
        let hits = self.render.hit_test(point);
        for &rid in hits.iter().rev() {
            // Imperative scroll view: nudge its spring target directly.
            if let Some(s) = self.render.object_ref(rid).downcast_ref::<RenderScroll>() {
                if want.is_some_and(|a| a != s.axis) {
                    continue; // wrong axis for this wheel — let a matching ancestor take it
                }
                if s.at_edge(delta) {
                    continue; // bubble to an ancestor scroll view
                }
                let moved = self
                    .render
                    .object_mut(rid)
                    .downcast_mut::<RenderScroll>()
                    .is_some_and(|s| s.scroll_by(delta));
                if moved {
                    self.scroll_anim.insert(rid);
                    self.scroll_moved(rid);
                }
                return moved;
            }
            // Controlled (virtualized) list: route to its offset signal.
            if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
                if want.is_some_and(|a| a != list.axis) {
                    continue;
                }
                if list.at_edge(delta) {
                    continue;
                }
                return crate::scroll::dispatch(list.id, ScrollTo::By(delta));
            }
        }
        false
    }

    /// A scroll offset changed: re-position the clipped content (a paint-time
    /// concern) and request paint. Scrolling NEVER relayouts the content — layout
    /// runs only when the viewport or the content itself changes. The content
    /// child keeps the size/offsets of its last layout; only its offset within
    /// the (clipping) viewport moves.
    fn scroll_moved(&mut self, rid: RenderId) {
        let Some((axis, off)) = self
            .render
            .try_object_mut(rid)
            .and_then(|o| o.downcast_mut::<RenderScroll>())
            .map(|s| (s.axis, s.offset))
        else {
            return;
        };
        let child_offset = match axis {
            Axis::Vertical => Offset::new(0.0, -off),
            Axis::Horizontal => Offset::new(-off, 0.0),
        };
        self.render.set_scrolled_child_offset(rid, child_offset);
        self.render.mark_needs_paint(rid);
    }

    /// Advance every animating scroll spring by `dt`. Returns whether any are still
    /// moving (the shell keeps requesting frames while true).
    pub fn tick_scrolls(&mut self, dt: f64) -> bool {
        if self.scroll_anim.is_empty() {
            return false;
        }
        for rid in self.scroll_anim.iter().copied().collect::<Vec<_>>() {
            // The node may have unmounted mid-spring (navigation while the wheel
            // momentum is still settling) — drop the dead spring, touch nothing.
            let still = self
                .render
                .try_object_mut(rid)
                .and_then(|o| o.downcast_mut::<RenderScroll>())
                .map(|s| s.tick(dt));
            match still {
                Some(still) => {
                    self.scroll_moved(rid);
                    if !still {
                        self.scroll_anim.remove(&rid);
                    }
                }
                None => {
                    self.scroll_anim.remove(&rid);
                }
            }
        }
        !self.scroll_anim.is_empty()
    }

    /// The viewport extent of the innermost scroll view under `point`.
    fn viewport_under(&self, point: Offset) -> Option<f64> {
        self.render.hit_test(point).iter().rev().find_map(|&rid| {
            if let Some(s) = self.render.object_ref(rid).downcast_ref::<RenderScroll>() {
                return Some(s.viewport_extent);
            }
            self.render.object_ref(rid).downcast_ref::<RenderList>().map(|l| l.viewport())
        })
    }

    /// Keyboard page scroll (`sign` = +1 down / -1 up) on the scroll view under
    /// `point`. Scrolls ~85% of a viewport.
    pub fn scroll_page(&mut self, point: Offset, sign: f64) -> bool {
        match self.viewport_under(point) {
            Some(vp) => self.dispatch_scroll(point, sign * vp * 0.85),
            None => false,
        }
    }

    /// Keyboard line scroll (`sign` = +1 down / -1 up).
    pub fn scroll_line(&mut self, point: Offset, sign: f64) -> bool {
        self.dispatch_scroll(point, sign * 48.0)
    }

    /// Keyboard Home/End — jump to the start or end of the scroll view under `point`.
    pub fn scroll_to_end(&mut self, point: Offset, end: bool) -> bool {
        let hits = self.render.hit_test(point);
        for &rid in hits.iter().rev() {
            if self.render.object_ref(rid).is::<RenderScroll>() {
                if let Some(s) = self.render.object_mut(rid).downcast_mut::<RenderScroll>() {
                    let to = if end { s.max_offset } else { 0.0 };
                    s.scroll_to(to);
                    self.scroll_anim.insert(rid);
                    self.scroll_moved(rid);
                }
                return true;
            }
            if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
                let frac = if end { 1.0 } else { 0.0 };
                return crate::scroll::dispatch(list.id, ScrollTo::ToFraction(frac));
            }
        }
        false
    }

    /// Begin a scrollbar drag if `point` lands on a viewport's scrollbar strip.
    /// Returns whether one was grabbed (the shell then routes moves to it).
    pub fn begin_scrollbar_drag(&mut self, point: Offset) -> bool {
        let hits = self.render.hit_test(point);
        for &rid in hits.iter().rev() {
            let local = point - self.render.absolute_offset(rid);
            let size = self.render.size_of(rid);
            if let Some(s) = self.render.object_ref(rid).downcast_ref::<RenderScroll>() {
                if s.scrollbar_hit(local, size) {
                    self.scrollbar_drag = Some(rid);
                    self.update_scrollbar_drag(point);
                    return true;
                }
                return false;
            }
            if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
                if list.scrollbar_hit(local, size) {
                    self.scrollbar_drag = Some(rid);
                    self.update_scrollbar_drag(point);
                    return true;
                }
                return false;
            }
        }
        false
    }

    /// Update the active scrollbar drag to `point`. Returns whether it scrolled.
    pub fn update_scrollbar_drag(&mut self, point: Offset) -> bool {
        let Some(rid) = self.scrollbar_drag else { return false };
        // The scroll view may have unmounted mid-drag (e.g. its overlay closed on a
        // wheel/resize). Drop the stale drag instead of indexing a freed node.
        if !self.render.contains(rid) {
            self.scrollbar_drag = None;
            return false;
        }
        let local = point - self.render.absolute_offset(rid);
        let size = self.render.size_of(rid);
        // Imperative scroll view.
        if let Some(s) = self.render.object_mut(rid).downcast_mut::<RenderScroll>() {
            let changed = s.set_offset_from_point(local, size);
            if changed {
                self.scroll_moved(rid);
            }
            return changed;
        }
        // Controlled list: map to a fraction and route to its offset signal.
        if let Some(list) = self.render.object_ref(rid).downcast_ref::<RenderList>() {
            let frac = list.fraction_at(local);
            return crate::scroll::dispatch(list.id, ScrollTo::ToFraction(frac));
        }
        false
    }

    /// End any active scrollbar drag.
    pub fn end_scrollbar_drag(&mut self) {
        if let Some(rid) = self.scrollbar_drag
            && self.render.contains(rid)
            && let Some(s) = self.render.object_mut(rid).downcast_mut::<RenderScroll>()
        {
            s.end_scroll_activity(); // fire the closing ScrollEvent::End
        }
        self.scrollbar_drag = None;
    }

    /// Whether a scrollbar drag is currently active.
    pub fn scrollbar_dragging(&self) -> bool {
        self.scrollbar_drag.is_some()
    }
}
