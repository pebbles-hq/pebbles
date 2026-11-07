//! Secondary OS windows (F5): open/close/command pumping, per-window event
//! handling, all sharing the GPU context and the reactive runtime with the main
//! window.

use super::lifecycle::Lifecycle;
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
            if id == pebbles_widgets::window::MAIN_WINDOW {
                // `confirm_exit()`: a programmatic exit, past the exit handler.
                event_loop.exit();
                continue;
            }
            if let Some(wid) = self.window_by_id.get(&id).copied() {
                self.close_secondary(wid);
            } else {
                // Never brought up (or already gone): still report it closed.
                pebbles_widgets::window::window_closed(id);
            }
        }
        // Runtime window changes (set_title / maximize / minimize / move / …).
        let mut handed_off = false;
        for cmd in pebbles_widgets::window::take_window_commands() {
            use pebbles_widgets::window::WindowCommand::*;
            let main = self.active.as_ref().map(|a| &a.window);
            // MAIN_WINDOW (0) addresses the app's main window; other ids the secondaries.
            let target = |id| {
                if id == pebbles_widgets::window::MAIN_WINDOW {
                    main
                } else {
                    self.window_by_id.get(&id).and_then(|wid| self.windows.get(wid)).map(|w| &w.window)
                }
            };
            let is_main = |id| id == pebbles_widgets::window::MAIN_WINDOW;
            match cmd {
                SetTitle(id, t) => {
                    if let Some(w) = target(id) {
                        w.set_title(&t);
                    }
                }
                SetResizable(id, r) => {
                    if let Some(w) = target(id) {
                        w.set_resizable(r);
                    }
                }
                SetMaximized(id, m) => {
                    if let Some(w) = target(id) {
                        w.set_maximized(m);
                    }
                }
                ToggleMaximized(id) => {
                    if let Some(w) = target(id) {
                        w.set_maximized(!w.is_maximized());
                    }
                }
                Minimize(id) => {
                    if let Some(w) = target(id) {
                        w.set_minimized(true);
                    }
                }
                SetPosition(id, x, y) => {
                    if let Some(w) = target(id) {
                        w.set_outer_position(winit::dpi::LogicalPosition::new(x, y));
                    }
                }
                SetSize(id, width, height) => {
                    if let Some(w) = target(id) {
                        let _ = w.request_inner_size(LogicalSize::new(width, height));
                    }
                }
                Focus(id) => {
                    if let Some(w) = target(id) {
                        w.focus_window();
                    }
                }
                SetFullscreen(id, on) => {
                    if let Some(win) = target(id) {
                        win.set_fullscreen(on.then(|| winit::window::Fullscreen::Borderless(None)));
                    }
                }
                SetDecorations(id, on) => {
                    if let Some(w) = target(id) {
                        w.set_decorations(on);
                    }
                }
                DragMove(id) => {
                    if let Some(w) = target(id) {
                        let _ = w.drag_window();
                        handed_off |= is_main(id);
                    }
                }
                DragResize(id, edge) => {
                    if let Some(w) = target(id) {
                        let _ = w.drag_resize_window(resize_direction(edge));
                        handed_off |= is_main(id);
                    }
                }
                ShowWindowMenu(id, x, y) => {
                    if let Some(w) = target(id) {
                        w.show_window_menu(winit::dpi::LogicalPosition::new(x, y));
                    }
                }
                StartFileDrag(id, paths) => {
                    #[cfg(any(
                        target_os = "linux",
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "netbsd",
                        target_os = "dragonfly"
                    ))]
                    if let Some(w) = target(id)
                        && crate::os_drag::start(w, paths)
                    {
                        // The system drag owns the pointer now; its release won't reach us.
                        handed_off = true;
                    }
                    #[cfg(not(any(
                        target_os = "linux",
                        target_os = "freebsd",
                        target_os = "openbsd",
                        target_os = "netbsd",
                        target_os = "dragonfly"
                    )))]
                    let _ = (id, paths);
                }
            }
        }
        // The window manager now owns the press (move/resize grab) and usually swallows
        // the release: forget the press so the next one starts clean.
        if handed_off {
            self.forget_press();
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
        attrs = with_app_id(attrs, self.app_id.as_deref());
        // A secondary window failing must NOT take down the app (the main window is fine) —
        // log and skip opening it, unlike the fatal main-window path.
        let window = match event_loop.create_window(attrs) {
            Ok(w) => {
                #[cfg(any(
                    target_os = "linux",
                    target_os = "freebsd",
                    target_os = "openbsd",
                    target_os = "netbsd",
                    target_os = "dragonfly"
                ))]
                crate::os_drag::window_created(&w);
                Arc::new(w)
            }
            Err(e) => {
                pebbles_core::log::warn(
                    pebbles_core::log::Cat::General,
                    format!("could not open a secondary window: {e}"),
                );
                open_failed(&spec);
                return;
            }
        };
        window.set_ime_allowed(true); // enable IME composition on secondary windows too
        let physical = window.inner_size();
        // 08 §17.5: the surface is created here (the UI thread) and adopted by the sink.
        let Some(sink) = self.sink.as_mut() else {
            open_failed(&spec);
            return;
        };
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
            open_failed(&spec);
            return;
        }
        // A fresh Ui → a fresh window_id in the shared runtime. Wrap the root in an
        // OverlayHost so this window has its own popover/menu/dialog layer (the overlay
        // + dialog signals are namespaced per window id).
        let mut ui = Ui::new();
        ui.make_current(); // so lazily-created per-window overlay signals key to this window
        // The first build sees this window's real metrics.
        let (scale, phys) = (window.scale_factor(), window.inner_size());
        if let Some(l) = lifecycle::logical_size((phys.width, phys.height), scale) {
            pebbles_widgets::overlay::set_window_size(l.width, l.height);
            pebbles_widgets::set_device_pixel_ratio(scale);
        }
        ui.mount_root(pebbles_widgets::window::window_root(spec.background, spec.root));
        let wid = window.id();
        publish_bounds(spec.id, &window);
        publish_state(spec.id, &window);
        redraw::request(&window);
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
                pan: pebbles_core::pan::PanGesture::default(),
                current_cursor: Cursor::Default,
                on_close: spec.on_close,
                occlusion: frame::Occlusion::default(),
                buttons: 0,
                move_target: None,
                click_count: 0,
                last_click: None,
                last_tap_target: None,
                secondary_down_handled: false,
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
        if let Some(repaint) = file_drop_event(&event, w.cursor) {
            if repaint {
                redraw::request(&w.window);
            }
            self.windows.insert(window_id, w);
            return;
        }
        match event {
            WindowEvent::CloseRequested if !pebbles_widgets::window::close_requested(w.id) => {
                // Vetoed / deferred by the window's `on_close_requested` handler.
                redraw::request(&w.window);
            }
            WindowEvent::CloseRequested => {
                teardown(&mut w);
                self.window_by_id.remove(&w.id);
                keep = false;
            }
            // P12: the main window's size / scale / occlusion / focus rules, for this window.
            WindowEvent::Resized(size) => {
                secondary_lifecycle(&mut w, Lifecycle::Resized(size.width, size.height));
                publish_bounds(w.id, &w.window);
                publish_state(w.id, &w.window);
            }
            WindowEvent::Moved(_) => publish_bounds(w.id, &w.window),
            // The OS light/dark setting changed (ThemeMode::System follows it).
            WindowEvent::ThemeChanged(t) => {
                pebbles_widgets::theme::set_platform_brightness(platform_brightness(Some(t)));
                redraw::request(&w.window);
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                secondary_lifecycle(&mut w, Lifecycle::ScaleFactorChanged)
            }
            WindowEvent::Occluded(occluded) => secondary_lifecycle(&mut w, Lifecycle::Occluded(occluded)),
            WindowEvent::Focused(focused) => {
                secondary_lifecycle(&mut w, Lifecycle::Focused(focused));
                publish_state(w.id, &w.window);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = w.window.scale_factor();
                let prev = w.cursor;
                w.cursor = Offset::new(position.x / scale, position.y / scale);
                if pebbles_widgets::file_drop::move_to(w.cursor) {
                    redraw::request(&w.window);
                }
                // A scrollbar drag captures the pointer (as on the main window).
                if w.ui.scrollbar_dragging() {
                    if w.ui.update_scrollbar_drag(w.cursor) {
                        redraw::request(&w.window);
                    }
                    self.windows.insert(window_id, w);
                    return;
                }
                if let Some(t) = w.move_target
                    && w.ui.dispatch_pointer_move(t, w.cursor, prev, w.buttons)
                {
                    redraw::request(&w.window);
                }
                if w.ui.dispatch_hover(w.cursor) {
                    redraw::request(&w.window);
                }
                let content = w.ui.content_drag_active();
                let cursor = w.cursor;
                let step = w.pan.moved(&mut w.ui, cursor);
                let mut moved = step.handled;
                // Past the slop the press is a drag: the release must not click.
                if step.crossed_slop
                    && let Some(a) = w.armed_tap.take()
                {
                    moved |= w.ui.dispatch_tap_cancel(a);
                }
                if content {
                    moved |= w.ui.update_content_drag(cursor);
                }
                if moved {
                    redraw::request(&w.window);
                }
                sync_window_cursor(&mut w);
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let signal = scroll_signal(delta, w.window.scale_factor(), w.cursor);
                log_wheel(&delta, phase, w.window.scale_factor(), &signal);
                // Pointer signal first; then C6: secondary windows follow the popover
                // on wheel (was main-only).
                if w.ui.dispatch_pointer_signal(w.cursor, signal)
                    || wheel_with_overlay(&mut w.ui, w.cursor, signal.delta)
                {
                    redraw::request(&w.window);
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if secondary_pointer(&mut w, button, state) {
                    redraw::request(&w.window);
                }
                if state == ElementState::Released {
                    sync_window_cursor(&mut w);
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
                let mods = pebbles_core::Mods {
                    shift: self.shift_down,
                    ctrl: self.ctrl_down,
                    alt: self.alt_down,
                    meta: self.meta_down,
                };
                let raw = to_key_event(&event, mods);
                if event.state == ElementState::Released {
                    // Releases reach only the focused widget's raw handler.
                    if w.ui.dispatch_key_event(&raw) == pebbles_core::KeyEventResult::Handled {
                        redraw::request(&w.window);
                    }
                } else if event.state == ElementState::Pressed
                    && route_key_press(&mut w.ui, &event, mods, w.cursor).is_some()
                {
                    redraw::request(&w.window);
                }
            }
            WindowEvent::Ime(ime) => {
                let handled = match ime {
                    Ime::Preedit(text, _cursor) => w.ui.dispatch_key(KeyInput::Preedit(text)),
                    Ime::Commit(text) => w.ui.dispatch_key(KeyInput::Insert(text)),
                    Ime::Enabled | Ime::Disabled => false,
                };
                if handled {
                    redraw::request(&w.window);
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

    /// Close secondary window `wid` now (programmatic close / app exit): its `on_close`
    /// runs, its tree leaves the shared runtime, its per-window state is dropped.
    fn close_secondary(&mut self, wid: WindowId) {
        if let Some(mut w) = self.windows.remove(&wid) {
            self.window_by_id.remove(&w.id);
            teardown(&mut w);
            self.release_window(w.window);
        }
    }

    /// Close every secondary window (the app is exiting).
    pub(super) fn close_all_secondaries(&mut self) {
        let ids: Vec<WindowId> = self.windows.keys().copied().collect();
        for wid in ids {
            self.close_secondary(wid);
        }
    }

    /// 08 §17.5 close: the surface (it holds a window clone) goes before the window (T2). If the
    /// render thread does not acknowledge in time, the window is kept until shutdown.
    fn release_window(&mut self, window: Arc<Window>) {
        redraw::forget(window.id());
        let released = self.sink.as_mut().is_none_or(|sink| sink.remove_window(window.id()));
        if !released {
            self.retired_windows.push(window);
        }
    }
}

/// [`Runner::sync_cursor`] for a secondary window.
/// A lifecycle event on a secondary window (the caller made its `Ui` current).
fn secondary_lifecycle(w: &mut WindowRuntime, event: Lifecycle) {
    let fx = lifecycle::apply(&mut w.occlusion, &mut w.resize_sync, w.frame_id, event);
    if fx.hide_overlay {
        pebbles_widgets::overlay::hide_overlay();
    }
    if fx.redraw {
        redraw::request(&w.window);
    }
}

fn sync_window_cursor(w: &mut WindowRuntime) {
    let captured = w.pan.active_target().is_some() || w.ui.content_drag_active();
    if let Some(want) = cursor_update(w.current_cursor, w.ui.cursor_at(w.cursor), captured) {
        w.current_cursor = want;
        w.window.set_cursor(to_winit_cursor(want));
    }
}

/// Our resize edge as winit's direction.
fn resize_direction(edge: pebbles_widgets::window::ResizeEdge) -> winit::window::ResizeDirection {
    use pebbles_widgets::window::ResizeEdge as E;
    use winit::window::ResizeDirection as D;
    match edge {
        E::North => D::North,
        E::South => D::South,
        E::East => D::East,
        E::West => D::West,
        E::NorthEast => D::NorthEast,
        E::NorthWest => D::NorthWest,
        E::SouthEast => D::SouthEast,
        E::SouthWest => D::SouthWest,
    }
}

/// Everything a closing secondary window releases, in order: `on_close` (bounds are still
/// readable there), the tree's disposal (cleanups stop its loops/timers; window ids are
/// never reused, so skipping this leaks the tree and keeps every window redrawing), the
/// per-window overlay/dialog/sheet/toast state, then the open flag / bounds / handler.
fn teardown(w: &mut WindowRuntime) {
    w.ui.make_current();
    if let Some(f) = w.on_close.take() {
        f();
    }
    w.ui.dispose();
    pebbles_widgets::window::drop_window_state(w.ui.window_id());
    pebbles_widgets::window::window_closed(w.id);
}

/// A window that never came up still "closes", so app state waiting on it resets.
#[cfg(not(target_family = "wasm"))]
fn open_failed(spec: &pebbles_widgets::window::WindowSpec) {
    if let Some(f) = &spec.on_close {
        f();
    }
    pebbles_widgets::window::window_closed(spec.id);
}

/// Publish a window's position + inner size (logical px) for apps to save.
#[cfg(not(target_family = "wasm"))]
fn publish_bounds(id: u64, window: &Window) {
    let scale = window.scale_factor();
    let size = window.inner_size().to_logical::<f64>(scale);
    let pos = window.outer_position().map(|p| p.to_logical::<f64>(scale)).unwrap_or_default();
    if size.width < 1.0 || size.height < 1.0 {
        return; // minimised: keep the last real size
    }
    pebbles_widgets::window::set_window_bounds(
        id,
        pebbles_widgets::window::WindowBounds {
            x: pos.x.round() as i32,
            y: pos.y.round() as i32,
            width: size.width.round() as u32,
            height: size.height.round() as u32,
        },
    );
}
#[cfg(target_family = "wasm")]
fn publish_bounds(_id: u64, _window: &Window) {}

/// Publish a secondary window's maximised / full-screen / minimised / focused state
/// (`window::window_state(id)`), as the main window does — app-drawn title bars read it.
fn publish_state(id: u64, w: &Window) {
    pebbles_widgets::window::set_window_state(
        id,
        pebbles_widgets::window::WindowState {
            maximized: w.is_maximized(),
            fullscreen: w.fullscreen().is_some(),
            minimized: w.is_minimized().unwrap_or(false),
            focused: w.has_focus(),
        },
    );
}

/// A mouse button on a secondary window, with the main window's semantics: raw button
/// listeners for every button, scrollbar grabs, pan / content drag, tap / double /
/// triple tap, secondary (context-menu) and tertiary taps.
fn secondary_pointer(w: &mut WindowRuntime, button: MouseButton, state: ElementState) -> bool {
    let cursor = w.cursor;
    let pressed = state == ElementState::Pressed;
    if button == MouseButton::Left && pressed && !w.ui.over_scrollbar(cursor) {
        pebbles_core::focus::set_focus(None);
    }
    // Any press dismisses a tooltip / hover card (as in the main window).
    if pressed && pebbles_widgets::overlay::passive_is_open() {
        pebbles_widgets::hide_passive();
    }
    let raw = pointer_button(button).is_some_and(|b| {
        let bit = pebbles_render::PointerEvent::bit(b);
        if pressed {
            if w.buttons == 0 {
                w.move_target = w.ui.pointer_move_target_at(cursor);
            }
            w.buttons |= bit;
            w.ui.dispatch_button_down(cursor, b, w.buttons)
        } else {
            w.buttons &= !bit;
            if w.buttons == 0 {
                w.move_target = None;
            }
            w.ui.dispatch_button_up(cursor, b, w.buttons)
        }
    });
    let gesture = match (button, state) {
        (MouseButton::Left, ElementState::Pressed) if w.ui.begin_scrollbar_drag(cursor) => true,
        (MouseButton::Left, ElementState::Pressed) => {
            w.armed_tap = w.ui.tap_target_at(cursor);
            let claimed = w.ui.begin_content_drag(cursor);
            let target = if claimed { None } else { w.ui.pan_target_at(cursor) };
            w.pan.press(target, cursor, pebbles_core::pan::MOUSE_PAN_SLOP);
            w.ui.dispatch_pointer_down(cursor) || claimed
        }
        (MouseButton::Left, ElementState::Released) => {
            w.ui.end_scrollbar_drag();
            let drag_ended = w.ui.end_content_drag(cursor);
            w.pan.release(&mut w.ui, cursor);
            let up = w.ui.dispatch_pointer_up(cursor);
            let up_target = w.ui.tap_target_at(cursor);
            let armed = w.armed_tap.take();
            let tapped = if up_target.is_some() && up_target == armed {
                let now = Instant::now();
                let within = w.last_tap_target == up_target
                    && w.last_click.is_some_and(|t| now.duration_since(t) <= DOUBLE_CLICK);
                w.click_count = if within { (w.click_count % 3) + 1 } else { 1 };
                w.last_click = Some(now);
                w.last_tap_target = up_target;
                let multi = match w.click_count {
                    3 => w.ui.dispatch_triple_tap(cursor),
                    2 => w.ui.dispatch_double_tap(cursor),
                    _ => false,
                };
                multi || w.ui.dispatch_tap(cursor)
            } else if let Some(a) = armed {
                w.ui.dispatch_tap_cancel(a)
            } else {
                false
            };
            up || tapped || drag_ended
        }
        (MouseButton::Right, ElementState::Pressed) => {
            w.secondary_down_handled = w.ui.dispatch_secondary_tap_down(cursor);
            w.secondary_down_handled
        }
        (MouseButton::Right, ElementState::Released) => {
            let up = w.ui.dispatch_secondary_tap_up(cursor);
            let tap = w.ui.dispatch_secondary_tap(cursor);
            up || tap
        }
        (MouseButton::Middle, ElementState::Pressed) => w.ui.dispatch_tertiary_down(cursor),
        (MouseButton::Middle, ElementState::Released) => w.ui.dispatch_tertiary_up(cursor),
        _ => false,
    };
    gesture || raw
}
