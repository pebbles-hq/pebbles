//! Secondary OS windows (F5): open/close/command pumping, per-window event
//! handling, all sharing the GPU context and the reactive runtime with the main
//! window.

#[allow(clippy::wildcard_imports)]
use super::*;

thread_local! {
    /// Last router title pushed to the OS window, so the per-frame poll only calls
    /// `set_title` on an actual change.
    static TITLE_APPLIED: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

impl Runner {
    /// Create/close secondary windows requested by app code this turn.
    pub(super) fn pump_windows(&mut self, event_loop: &ActiveEventLoop) {
        for spec in pebbles_widgets::window::take_open_requests() {
            self.open_window(event_loop, spec);
        }
        for id in pebbles_widgets::window::take_close_requests() {
            if let Some(wid) = self.window_by_id.remove(&id)
                && let Some(w) = self.windows.remove(&wid)
            {
                if let Some(f) = &w.on_close {
                    f();
                }
                self.release_window(w.window);
            }
        }
        // Runtime window changes (set_title / maximize / minimize / move / …).
        for cmd in pebbles_widgets::window::take_window_commands() {
            use pebbles_widgets::window::WindowCommand::*;
            let target = |id| self.window_by_id.get(&id).and_then(|wid| self.windows.get(wid));
            match cmd {
                SetTitle(id, t) => {
                    if let Some(w) = target(id) {
                        w.window.set_title(&t);
                    }
                }
                SetResizable(id, r) => {
                    if let Some(w) = target(id) {
                        w.window.set_resizable(r);
                    }
                }
                SetMaximized(id, m) => {
                    if let Some(w) = target(id) {
                        w.window.set_maximized(m);
                    }
                }
                Minimize(id) => {
                    if let Some(w) = target(id) {
                        w.window.set_minimized(true);
                    }
                }
                SetPosition(id, x, y) => {
                    if let Some(w) = target(id) {
                        w.window.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
                    }
                }
                SetSize(id, width, height) => {
                    if let Some(w) = target(id) {
                        let _ = w.window.request_inner_size(LogicalSize::new(width, height));
                    }
                }
                Focus(id) => {
                    if let Some(w) = target(id) {
                        w.window.focus_window();
                    }
                }
            }
        }

        // Apply the router's per-route title (`router::set_title`) to the MAIN window
        // (`self.active`) — the previous lookup searched `self.windows`, the *secondary*
        // map, which never holds the main window, so the title never applied. Poll-and-diff
        // so we only touch it on a real change. On desktop winit sets the OS title; on web
        // winit's `set_title` writes the canvas `alt`, not the tab title, so we set
        // `document.title` directly instead.
        let title = pebbles_core::router::title();
        let changed = TITLE_APPLIED.with(|last| {
            let mut last = last.borrow_mut();
            if *last != title {
                *last = title.clone();
                true
            } else {
                false
            }
        });
        if changed && let Some(t) = &title {
            #[cfg(not(target_family = "wasm"))]
            if let Some(active) = self.active.as_ref() {
                active.window.set_title(t);
            }
            #[cfg(target_family = "wasm")]
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                doc.set_title(t);
            }
        }
    }

    pub(super) fn open_window(
        &mut self,
        event_loop: &ActiveEventLoop,
        spec: pebbles_widgets::window::WindowSpec,
    ) {
        // Secondary OS windows are desktop-only: web has a single canvas and mobile
        // a single activity/scene, and the surface bring-up here blocks (which the
        // browser main thread forbids). Degrade to a logged no-op rather than hang —
        // see PLATFORMS.md.
        #[cfg(target_family = "wasm")]
        {
            let _ = (event_loop, spec);
            pebbles_core::log::warn(
                pebbles_core::log::Cat::General,
                "window() (secondary windows) is desktop-only — ignored on web".to_string(),
            );
        }

        #[cfg(not(target_family = "wasm"))]
        {
            self.open_window_native(event_loop, spec);
        }
    }

    /// The real secondary-window bring-up (desktop only; see [`open_window`]).
    #[cfg(not(target_family = "wasm"))]
    fn open_window_native(
        &mut self,
        event_loop: &ActiveEventLoop,
        spec: pebbles_widgets::window::WindowSpec,
    ) {
        let mut attrs = WindowAttributes::default()
            .with_title(spec.title.clone())
            .with_inner_size(LogicalSize::new(spec.width, spec.height))
            .with_resizable(spec.resizable)
            .with_maximized(spec.maximized)
            .with_decorations(spec.decorations);
        if let Some((w, h)) = spec.min_size {
            attrs = attrs.with_min_inner_size(LogicalSize::new(w, h));
        }
        if let Some((w, h)) = spec.max_size {
            attrs = attrs.with_max_inner_size(LogicalSize::new(w, h));
        }
        if let Some((x, y)) = spec.position {
            attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x, y));
        }
        if let Some(icon) = &spec.icon
            && let Ok(i) = winit::window::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height)
        {
            attrs = attrs.with_window_icon(Some(i));
        }
        // A secondary window failing must NOT take down the app (the main window is fine) —
        // log and skip opening it, unlike the fatal main-window path.
        let window = match event_loop.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                pebbles_core::log::warn(
                    pebbles_core::log::Cat::General,
                    format!("could not open a secondary window: {e}"),
                );
                return;
            }
        };
        window.set_ime_allowed(true); // enable IME composition on secondary windows too
        let physical = window.inner_size();
        // 08 §17.5: the surface is created here (the UI thread) and adopted by the sink.
        let Some(sink) = self.sink.as_mut() else { return };
        let size = (physical.width.max(1), physical.height.max(1));
        let added = sink
            .instance()
            .create_surface(Arc::clone(&window))
            .map_err(|e| e.to_string())
            .and_then(|surface| sink.add_window(window.id(), surface, size));
        if let Err(e) = added {
            pebbles_core::log::warn(
                pebbles_core::log::Cat::General,
                format!("could not create a GPU surface for a secondary window: {e}"),
            );
            return;
        }
        // A fresh Ui → a fresh window_id in the shared runtime. Wrap the root in an
        // OverlayHost so this window has its own popover/menu/dialog layer (the overlay
        // + dialog signals are namespaced per window id).
        let mut ui = Ui::new();
        ui.make_current(); // so lazily-created per-window overlay signals key to this window
        let root = pebbles_widgets::OverlayHost::wrap(spec.root).into_widget();
        ui.mount_root(View::new(spec.background, root).into_widget());
        let wid = window.id();
        window.request_redraw();
        self.window_by_id.insert(spec.id, wid);
        self.windows.insert(
            wid,
            WindowRuntime {
                id: spec.id,
                window,
                pacer: Pacer::default(),
                frame_id: 0,
                resize_sync: ResizeSync::default(),
                ui,
                background: spec.background,
                cursor: Offset::ZERO,
                armed_tap: None,
                pan_target: None,
                current_cursor: Cursor::Default,
                on_close: spec.on_close,
                occlusion: frame::Occlusion::default(),
            },
        );
    }

    /// Handle a window event addressed to a secondary window.
    pub(super) fn secondary_event(&mut self, window_id: WindowId, event: WindowEvent) {
        let Some(mut w) = self.windows.remove(&window_id) else { return };
        // Route window-scoped globals (overlay/dialog signals) to THIS window while we
        // handle its input — event handlers don't otherwise set the current window.
        w.ui.make_current();
        if input::is_user_input(&event) {
            w.pacer.note_input();
        }
        let mut keep = true;
        match event {
            WindowEvent::CloseRequested => {
                self.window_by_id.remove(&w.id);
                if let Some(f) = &w.on_close {
                    f();
                }
                // Tear the window's tree out of the shared runtime (cleanups stop its
                // loops/timers; its signals are freed) and drop its per-window
                // overlay/dialog/sheet/toast state. Window ids are never reused, so
                // this is the only chance — skipping it leaked the whole tree per
                // open/close, and any surviving `create_loop` (a spinner, a focused
                // field's caret) kept every remaining window redrawing at full rate
                // forever.
                w.ui.dispose();
                pebbles_widgets::window::drop_window_state(w.ui.window_id());
                keep = false;
            }
            WindowEvent::Resized(size) => {
                // The next packet carries the new size; `render_packet` configures (08 §17.3 b).
                if size.width > 0 && size.height > 0 {
                    w.occlusion.shown(w.frame_id);
                    w.resize_sync.pending = true;
                    w.window.request_redraw();
                }
            }
            // P12: the main window's scale / occlusion / focus handling, for this window.
            WindowEvent::ScaleFactorChanged { .. } => {
                pebbles_widgets::overlay::hide_overlay();
                w.occlusion.shown(w.frame_id);
                w.resize_sync.pending = true;
                w.window.request_redraw();
            }
            WindowEvent::Occluded(occluded) => {
                if w.occlusion.system(occluded, w.frame_id) {
                    w.window.request_redraw();
                }
            }
            WindowEvent::Focused(true) => {
                w.occlusion.shown(w.frame_id);
                w.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = w.window.scale_factor();
                w.cursor = Offset::new(position.x / scale, position.y / scale);
                if w.ui.dispatch_hover(w.cursor) {
                    w.window.request_redraw();
                }
                let moved = if w.ui.content_drag_active() {
                    w.ui.update_content_drag(w.cursor)
                } else if let Some(t) = w.pan_target {
                    w.ui.dispatch_pan_update(t, w.cursor)
                } else {
                    false
                };
                if moved {
                    w.window.request_redraw();
                }
                let want = w.ui.cursor_at(w.cursor).unwrap_or(Cursor::Default);
                if want != w.current_cursor {
                    w.current_cursor = want;
                    w.window.set_cursor(to_winit_cursor(want));
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -(y as f64) * LINE_SCROLL,
                    MouseScrollDelta::PixelDelta(p) => -p.y / w.window.scale_factor(),
                };
                // C6: secondary windows now follow the popover on wheel (was main-only).
                if wheel_with_overlay(&mut w.ui, w.cursor, dy) {
                    w.window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button: MouseButton::Left, .. } => {
                let cursor = w.cursor;
                let handled = match state {
                    ElementState::Pressed => {
                        pebbles_core::focus::set_focus(None);
                        w.armed_tap = w.ui.tap_target_at(cursor);
                        // A drag-scroll viewport claims the drag first (A4); a
                        // pan-hungry descendant under the pointer wins instead.
                        let claimed = w.ui.begin_content_drag(cursor);
                        w.pan_target = if claimed { None } else { w.ui.pan_target_at(cursor) };
                        let panned = w.pan_target.is_some_and(|t| w.ui.dispatch_pan_start(t, cursor));
                        w.ui.dispatch_pointer_down(cursor) || panned || claimed
                    }
                    ElementState::Released => {
                        let drag_ended = w.ui.end_content_drag(cursor);
                        if let Some(t) = w.pan_target.take() {
                            w.ui.dispatch_pan_end(t, cursor);
                        }
                        let up = w.ui.dispatch_pointer_up(cursor);
                        let up_target = w.ui.tap_target_at(cursor);
                        let armed = w.armed_tap.take();
                        let tapped = if up_target.is_some() && up_target == armed {
                            w.ui.dispatch_tap(cursor)
                        } else if let Some(a) = armed {
                            w.ui.dispatch_tap_cancel(a)
                        } else {
                            false
                        };
                        up || tapped || drag_ended
                    }
                };
                if handled {
                    w.window.request_redraw();
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_down = modifiers.state().shift_key();
                self.ctrl_down = modifiers.state().control_key();
                self.alt_down = modifiers.state().alt_key();
                self.meta_down = modifiers.state().super_key();
                pebbles_core::keyboard::set_modifiers(
                    self.shift_down,
                    self.ctrl_down,
                    self.alt_down,
                    self.meta_down,
                );
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    // Escape, topmost-first (mirrors the main window): a transient
                    // overlay (menu/popover/dropdown/date-time picker) closes before
                    // any modal, then the topmost modal (dialog vs sheet) — so Escape
                    // can't close a modal underneath a floating menu.
                    if event.logical_key == Key::Named(NamedKey::Escape) {
                        let mut consumed = false;
                        if pebbles_widgets::overlay::is_open() {
                            pebbles_widgets::overlay::hide_overlay();
                            consumed = true;
                        } else if pebbles_widgets::dialog::is_open() || pebbles_widgets::sheet::is_open() {
                            pebbles_widgets::overlay::dismiss_topmost_modal();
                            consumed = true;
                        }
                        if consumed {
                            w.window.request_redraw();
                            if keep {
                                self.windows.insert(window_id, w);
                            }
                            return;
                        }
                    }
                    // B2 precedence: focused editor → shortcuts → Tab → activation.
                    let intent = to_command(&event, self.ctrl_down, self.shift_down);
                    let mods = pebbles_core::Mods {
                        shift: self.shift_down,
                        ctrl: self.ctrl_down,
                        alt: self.alt_down,
                        meta: self.meta_down,
                    };
                    // Ordered precedence: each branch's condition has an essential side
                    // effect (dispatch), so the identical `true` arms must stay separate.
                    #[allow(clippy::if_same_then_else)]
                    let handled = if intent.is_some_and(|ki| w.ui.dispatch_key(ki)) {
                        true
                    } else if to_shortcut_key(&event)
                        .is_some_and(|sk| pebbles_core::shortcuts::dispatch(w.ui.window_id(), mods, sk))
                    {
                        true
                    } else if event.logical_key == Key::Named(NamedKey::Tab) {
                        w.ui.focus_move(!self.shift_down)
                    } else {
                        matches!(
                            event.logical_key.as_ref(),
                            Key::Named(NamedKey::Enter | NamedKey::Space) | Key::Character(" ")
                        ) && w.ui.dispatch_activate()
                    };
                    if handled {
                        w.window.request_redraw();
                    }
                }
            }
            WindowEvent::Ime(ime) => {
                let handled = match ime {
                    Ime::Preedit(text, _cursor) => w.ui.dispatch_key(KeyInput::Preedit(text)),
                    Ime::Commit(text) => w.ui.dispatch_key(KeyInput::Insert(text)),
                    Ime::Enabled | Ime::Disabled => false,
                };
                if handled {
                    w.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => self.render_window(&mut w),
            _ => {}
        }
        if keep {
            self.windows.insert(window_id, w);
        } else {
            self.release_window(w.window);
        }
    }

    /// 08 §17.5 close: the surface (it holds a window clone) goes before the window (T2). If the
    /// render thread does not acknowledge in time, the window is kept until shutdown.
    fn release_window(&mut self, window: Arc<Window>) {
        let released = self.sink.as_mut().is_none_or(|sink| sink.remove_window(window.id()));
        if !released {
            self.retired_windows.push(window);
        }
    }
}
