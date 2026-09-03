//! The winit [`ApplicationHandler`] engine behind [`App`](super::App): owns the wgpu
//! surface(s), the Vello renderer, the [`Ui`] engine(s), and all persistent input
//! state, and drives the reconcile → layout → paint → present loop.
//!
//! Split by concern: [`input`] (event → intent translation), [`render`] (the frame
//! pipeline), [`windows`] (secondary OS windows). Child modules reach the parent's
//! private state directly; the [`ApplicationHandler`] impl stays here and delegates.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pebbles_core::{IntoWidget, KeyInput, Motion, Ui};
use pebbles_foundation::{Color, Offset, Size, TextDirection};
use pebbles_widgets::{MenuBar, View};
use vello::kurbo::Affine;
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

use pebbles_render::{Cursor, TextEnv};

use super::App;

mod input;
mod render;
mod windows;

use input::{to_command, to_shortcut_key, to_winit_cursor, wheel_with_overlay};

/// Max interval between two primary clicks to count as a double-tap.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How long the primary button must be held to count as a long-press.
const LONG_PRESS: Duration = Duration::from_millis(500);
/// Logical pixels scrolled per wheel line.
const LINE_SCROLL: f64 = 48.0;

/// Global count of uncaptured GPU errors — the render loop compares it per
/// frame and REBUILDS the poisoned GPU state (vello renderer + surface target)
/// when it moved. See [`install_error_handler`].
pub(super) static GPU_ERRORS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// A GUI shell must not die on a transient GPU validation hiccup: some
/// driver/compositor startup races (seen on Linux/Vulkan) surface as spurious
/// wgpu validation errors, which wgpu's DEFAULT uncaptured-error handler turns
/// into a process panic. Worse, a failed resource can stay cached inside the
/// renderer's pool, poisoning every subsequent frame. So: count the errors
/// (throttled log) and let the render loop rebuild the renderer + surface
/// target whenever the count moves — a clean pool the very next frame.
pub(super) fn install_error_handler(device: &wgpu::Device) {
    device.on_uncaptured_error(Arc::new(|e: wgpu::Error| {
        let n = GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n <= 3 || n.is_multiple_of(50) {
            eprintln!("pebbles: wgpu error #{n} — rebuilding the GPU state: {e}");
            if std::env::var("PEBBLES_GPU_TRACE").is_ok() {
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
            }
        }
    }));
}

/// Build a vello renderer for `device` (initial setup and error recovery).
pub(super) fn new_renderer(device: &wgpu::Device, _queue: &wgpu::Queue) -> Renderer {
    Renderer::new(
        device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::all(),
            num_init_threads: None,
            pipeline_cache: None,
        },
    )
    .expect("create vello renderer")
}

/// Wire the OS clipboard into `pebbles_core::clipboard`. Falls back to the core's
/// in-process clipboard if the platform clipboard can't be opened (e.g. headless).
fn install_clipboard() {
    use std::cell::RefCell;
    use std::rc::Rc;
    match arboard::Clipboard::new() {
        Ok(cb) => {
            let cb = Rc::new(RefCell::new(cb));
            let reader = cb.clone();
            let writer = cb;
            pebbles_core::clipboard::install(
                move || reader.borrow_mut().get_text().unwrap_or_default(),
                move |text| {
                    let _ = writer.borrow_mut().set_text(text.to_string());
                },
            );
        }
        Err(err) => eprintln!("pebbles: system clipboard unavailable ({err}); using in-app clipboard"),
    }
}

/// The live window + its GPU surface.
struct ActiveWindow {
    window: Arc<Window>,
    surface: RenderSurface<'static>,
}

/// A secondary OS window opened via [`window`](pebbles_widgets::window). It has its
/// own [`Ui`] + surface but shares the GPU context, renderers and — crucially — the
/// reactive runtime with the main window, so cross-window state is just a shared
/// signal. Each `Ui` carries a distinct `window_id`, so components never alias.
struct WindowRuntime {
    id: u64,
    window: Arc<Window>,
    surface: RenderSurface<'static>,
    ui: Ui,
    background: Color,
    cursor: Offset,
    armed_tap: Option<u64>,
    pan_target: Option<u64>,
    current_cursor: Cursor,
    on_close: Option<Rc<dyn Fn()>>,
}

/// The winit application handler. Holds all persistent runtime state.
pub(super) struct Runner {
    // configuration
    title: String,
    background: Color,
    size: (u32, u32),
    min_size: Option<(u32, u32)>,
    max_size: Option<(u32, u32)>,
    position: Option<(i32, i32)>,
    resizable: bool,
    maximized: bool,
    decorations: bool,
    pending_root: Option<pebbles_core::AnyWidget>,
    /// B3 native menu bar spec, held until the window exists (see `resumed`). Read
    /// only when the native menu path is actually compiled (feature on + macOS/Windows).
    #[cfg_attr(
        not(all(feature = "native-menus", any(target_os = "macos", target_os = "windows"))),
        allow(dead_code)
    )]
    menu_spec: Option<MenuBar>,
    /// D2 global text direction, applied once at mount.
    text_direction: TextDirection,
    /// F2 widget inspector: toggled by Mod+Shift+I; when on, the hovered render object's
    /// window-space rect (for the outline drawn each frame).
    inspect_mode: bool,
    inspect_rect: Option<pebbles_foundation::Rect>,
    /// The live native menu (built + attached in `resumed`, drained each turn).
    #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
    native_menu: Option<crate::native_menu::NativeMenus>,

    // gpu
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
    /// The [`GPU_ERRORS`] count already recovered from (see `recover_gpu_if_poisoned`).
    gpu_errors_seen: u64,
    /// When the last full GPU reset ran (recovery is throttled to ~1/second).
    last_gpu_reset: Option<Instant>,
    active: Option<ActiveWindow>,
    /// AccessKit platform bridge for the main window (accessibility tree + focus).
    a11y: Option<crate::a11y::Bridge>,

    // ui
    ui: Ui,
    text: TextEnv,
    scene: Scene,
    frame: Scene,
    mounted: bool,

    // input
    cursor: Offset,
    last_click: Option<Instant>,
    /// The tap target of the previous click — a double-tap only counts if the two
    /// clicks land on the same widget.
    last_tap_target: Option<u64>,
    /// Whether the right-button PRESS was claimed by a widget (its own context
    /// menu, a blocker) — the release must not open the global menu then.
    secondary_down_handled: bool,
    current_cursor: Cursor,
    /// Deadline at which a held primary button becomes a long-press.
    press_deadline: Option<Instant>,
    shift_down: bool,
    ctrl_down: bool,
    alt_down: bool,
    meta_down: bool,
    /// The primary-tap target armed at pointer-down (for tap vs. cancel on release).
    armed_tap: Option<u64>,
    /// The long-press target armed at pointer-down.
    lp_target: Option<u64>,
    /// Whether a long press has been recognized (past the threshold).
    lp_active: bool,
    /// The drag (pan) target armed at pointer-down; receives move/end until release.
    pan_target: Option<u64>,
    /// Monotonic clock start — the time base for animations.
    clock: Instant,
    /// Elapsed seconds at the previous frame, for the per-frame scroll-spring `dt`.
    last_frame_t: f64,

    // secondary windows (keyed by winit WindowId; app-facing ids map through)
    windows: HashMap<WindowId, WindowRuntime>,
    window_by_id: HashMap<u64, WindowId>,
}

impl Runner {
    pub(super) fn new(app: App) -> Self {
        Runner {
            title: app.title,
            background: app.background,
            size: app.size,
            min_size: app.min_size,
            max_size: app.max_size,
            position: app.position,
            resizable: app.resizable,
            maximized: app.maximized,
            decorations: app.decorations,
            pending_root: app.root,
            menu_spec: app.menu,
            text_direction: app.text_direction,
            inspect_mode: false,
            inspect_rect: None,
            #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
            native_menu: None,
            context: RenderContext::new(),
            renderers: Vec::new(),
            gpu_errors_seen: 0,
            last_gpu_reset: None,
            active: None,
            a11y: None,
            ui: Ui::new(),
            text: TextEnv::new(),
            scene: Scene::new(),
            frame: Scene::new(),
            mounted: false,
            cursor: Offset::ZERO,
            last_click: None,
            last_tap_target: None,
            secondary_down_handled: false,
            current_cursor: Cursor::Default,
            press_deadline: None,
            shift_down: false,
            ctrl_down: false,
            alt_down: false,
            meta_down: false,
            armed_tap: None,
            lp_target: None,
            lp_active: false,
            pan_target: None,
            clock: Instant::now(),
            last_frame_t: 0.0,
            windows: HashMap::new(),
            window_by_id: HashMap::new(),
        }
    }

    fn request_redraw(&self) {
        if let Some(active) = self.active.as_ref() {
            active.window.request_redraw();
        }
    }

    /// B3 — build the native menu from the spec (once) and attach it to the window.
    /// Compiled only on macOS/Windows (where `muda` integrates with winit cleanly);
    /// on Linux the in-window `menubar(..)` remains the form.
    #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
    fn install_native_menu(&mut self) {
        if self.native_menu.is_some() {
            return;
        }
        let Some(spec) = self.menu_spec.as_ref() else {
            return;
        };
        let menus = crate::native_menu::NativeMenus::build(spec);
        if let Some(active) = self.active.as_ref() {
            menus.attach(&active.window);
        }
        self.native_menu = Some(menus);
    }

    /// Route an unclaimed key press to scroll the view under the pointer.
    fn scroll_key(&mut self, event: &KeyEvent) -> bool {
        let cursor = self.cursor;
        match event.logical_key.as_ref() {
            Key::Named(NamedKey::PageDown) => self.ui.scroll_page(cursor, 1.0),
            Key::Named(NamedKey::PageUp) => self.ui.scroll_page(cursor, -1.0),
            Key::Named(NamedKey::Home) => self.ui.scroll_to_end(cursor, false),
            Key::Named(NamedKey::End) => self.ui.scroll_to_end(cursor, true),
            Key::Named(NamedKey::ArrowDown) => self.ui.scroll_line(cursor, 1.0),
            Key::Named(NamedKey::ArrowUp) => self.ui.scroll_line(cursor, -1.0),
            Key::Named(NamedKey::Space) => {
                self.ui.scroll_page(cursor, if self.shift_down { -1.0 } else { 1.0 })
            }
            _ => false,
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.active.is_some() {
            return;
        }
        let mut attrs = WindowAttributes::default()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(self.size.0, self.size.1))
            .with_resizable(self.resizable)
            .with_maximized(self.maximized)
            .with_decorations(self.decorations);
        if let Some((w, h)) = self.min_size {
            attrs = attrs.with_min_inner_size(LogicalSize::new(w, h));
        }
        if let Some((w, h)) = self.max_size {
            attrs = attrs.with_max_inner_size(LogicalSize::new(w, h));
        }
        if let Some((x, y)) = self.position {
            attrs = attrs.with_position(winit::dpi::LogicalPosition::new(x, y));
        }
        // Create hidden so the AccessKit adapter can attach before the window is shown
        // (the adapter panics if the window is already visible).
        attrs = attrs.with_visible(false);
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        window.set_ime_allowed(true); // enable IME composition (CJK, dead keys, etc.)
        self.a11y = Some(crate::a11y::Bridge::new(event_loop, &window));
        window.set_visible(true);

        let physical = window.inner_size();
        let surface = pollster::block_on(self.context.create_surface(
            window.clone(),
            physical.width.max(1),
            physical.height.max(1),
            wgpu::PresentMode::AutoVsync,
        ))
        .expect("create surface");

        // Ensure a renderer exists for this surface's device.
        self.renderers.resize_with(self.context.devices.len(), || None);
        install_error_handler(&self.context.devices[surface.dev_id].device);
        let dh = &self.context.devices[surface.dev_id];
        self.renderers[surface.dev_id].get_or_insert_with(|| new_renderer(&dh.device, &dh.queue));

        // Mount the widget tree once, wrapped in the root View.
        if !self.mounted {
            pebbles_core::focus::init(); // create the global focus signal (before any component)
            pebbles_widgets::overlay::init(); // create the global overlay signal too
            pebbles_widgets::dialog::init(); // and the global modal-dialog signal
            pebbles_widgets::sheet::init(); // and the global sheet/drawer signal
            pebbles_widgets::theme::init(); // and the global reactive theme signal
            pebbles_widgets::text_direction::init(); // D2: the reactive direction signal
            pebbles_widgets::set_text_direction(self.text_direction); // apply the app's direction
            install_clipboard(); // wire the system clipboard for Ctrl+C/X/V
            let root = self.pending_root.take().expect("root widget");
            self.ui.mount_root(View::new(self.background, root).into_widget());
            self.mounted = true;
        }

        window.request_redraw();
        self.active = Some(ActiveWindow { window, surface });

        // B3: build + attach the native menu bar now that the window exists.
        #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
        self.install_native_menu();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // Route events for a secondary window to its own handler.
        if self.windows.contains_key(&window_id) {
            self.secondary_event(window_id, event);
            self.pump_windows(event_loop);
            return;
        }
        // Route window-scoped globals (overlay/dialog signals) to the main window while
        // we handle its input (event handlers don't otherwise set the current window).
        self.ui.make_current();
        // Feed the event to the accessibility adapter before the app handles it.
        if let Some(win) = self.active.as_ref().map(|a| a.window.clone())
            && let Some(a11y) = self.a11y.as_mut()
        {
            a11y.process_event(&win, &event);
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(size) => {
                // A resize invalidates any popover's anchored position — dismiss it.
                pebbles_widgets::overlay::hide_overlay();
                if let Some(active) = self.active.as_mut()
                    && size.width > 0
                    && size.height > 0
                {
                    self.context.resize_surface(&mut active.surface, size.width, size.height);
                    active.window.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                if let Some(active) = self.active.as_ref() {
                    let scale = active.window.scale_factor();
                    let PhysicalPosition { x, y } = position;
                    self.cursor = Offset::new(x / scale, y / scale);
                }
                let cursor = self.cursor;
                // F2: in inspect mode, outline the deepest render object under the cursor.
                if self.inspect_mode {
                    let chain = pebbles_render::inspect_at(self.ui.render_tree(), cursor);
                    self.inspect_rect = chain.last().map(|n| n.bounds);
                    self.request_redraw();
                    return;
                }
                // A scrollbar drag captures the pointer: skip hover/pan/long-press.
                if self.ui.scrollbar_dragging() {
                    if self.ui.update_scrollbar_drag(cursor) {
                        self.request_redraw();
                    }
                    return;
                }
                if self.ui.dispatch_hover(cursor) {
                    self.request_redraw();
                }
                // Feed movement into an active content drag (A4 drag-scroll) or pan.
                let moved = if self.ui.content_drag_active() {
                    self.ui.update_content_drag(cursor)
                } else if let Some(t) = self.pan_target {
                    self.ui.dispatch_pan_update(t, cursor)
                } else {
                    false
                };
                if moved {
                    self.request_redraw();
                }
                // Feed movement into an active long press.
                if self.lp_active
                    && let Some(t) = self.lp_target
                    && self.ui.dispatch_long_press_move(t, cursor)
                {
                    self.request_redraw();
                }
                // Update the OS cursor to match the widget under the pointer.
                let want = self.ui.cursor_at(cursor).unwrap_or(Cursor::Default);
                if want != self.current_cursor {
                    self.current_cursor = want;
                    if let Some(active) = self.active.as_ref() {
                        active.window.set_cursor(to_winit_cursor(want));
                    }
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -(y as f64) * LINE_SCROLL,
                    MouseScrollDelta::PixelDelta(p) => {
                        let scale = self.active.as_ref().map_or(1.0, |a| a.window.scale_factor());
                        -p.y / scale
                    }
                };
                let cursor = self.cursor;
                if wheel_with_overlay(&mut self.ui, cursor, dy) {
                    self.request_redraw();
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let cursor = self.cursor;
                // F2: in inspect mode a click prints the render chain instead of hitting the UI.
                if self.inspect_mode
                    && button == MouseButton::Left
                    && state == ElementState::Pressed
                {
                    let chain = pebbles_render::inspect_at(self.ui.render_tree(), cursor);
                    eprint!("{}", pebbles_render::format_chain(&chain));
                    return;
                }
                let handled = match (button, state) {
                    (MouseButton::Left, ElementState::Pressed)
                        if self.ui.begin_scrollbar_drag(cursor) =>
                    {
                        // Grabbed a scrollbar — it captures the pointer until release.
                        true
                    }
                    (MouseButton::Left, ElementState::Pressed) => {
                        // Blur first: a press clears focus, then the widget under the
                        // pointer (button/field) re-grabs it in its own press handler.
                        // A press on empty space (or a non-focusable widget) stays
                        // blurred — the standard "tap outside unfocuses" behavior.
                        pebbles_core::focus::set_focus(None);
                        // Arm the tap (for tap vs. cancel) and the long-press.
                        self.armed_tap = self.ui.tap_target_at(cursor);
                        self.lp_target = self.ui.long_press_target_at(cursor);
                        self.lp_active = false;
                        self.press_deadline =
                            self.lp_target.map(|_| Instant::now() + LONG_PRESS);
                        if let Some(t) = self.lp_target {
                            self.ui.dispatch_long_press_down(t, cursor);
                        }
                        // A drag-scroll viewport claims the drag first (A4); a
                        // pan-hungry descendant under the pointer wins instead.
                        let claimed = self.ui.begin_content_drag(cursor);
                        self.pan_target =
                            if claimed { None } else { self.ui.pan_target_at(cursor) };
                        let panned =
                            self.pan_target.is_some_and(|t| self.ui.dispatch_pan_start(t, cursor));
                        self.ui.dispatch_pointer_down(cursor) || panned || claimed
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        // End any scrollbar / content drag.
                        self.ui.end_scrollbar_drag();
                        let drag_ended = self.ui.end_content_drag(cursor);
                        if let Some(t) = self.pan_target.take() {
                            self.ui.dispatch_pan_end(t, cursor);
                        }
                        // Resolve the long-press gesture.
                        if self.lp_active {
                            if let Some(t) = self.lp_target.take() {
                                self.ui.dispatch_long_press_end(t, cursor);
                            }
                            self.lp_active = false;
                        } else if self.press_deadline.is_some() {
                            if let Some(t) = self.lp_target.take() {
                                self.ui.dispatch_long_press_cancel(t);
                            }
                        }
                        self.press_deadline = None;
                        let up = self.ui.dispatch_pointer_up(cursor);
                        let up_target = self.ui.tap_target_at(cursor);
                        let armed = self.armed_tap.take();
                        let result = if up_target.is_some() && up_target == armed {
                            // Released over the same widget → tap / double-tap. A
                            // double-tap requires both clicks on the SAME target.
                            let now = Instant::now();
                            let is_double = self.last_tap_target == up_target
                                && self
                                    .last_click
                                    .is_some_and(|t| now.duration_since(t) <= DOUBLE_CLICK);
                            self.last_click = Some(now);
                            self.last_tap_target = up_target;
                            if is_double && self.ui.dispatch_double_tap(cursor) {
                                true
                            } else {
                                self.ui.dispatch_tap(cursor)
                            }
                        } else if let Some(a) = armed {
                            // Released off the armed widget → cancel.
                            self.ui.dispatch_tap_cancel(a)
                        } else {
                            false
                        };
                        up || result || drag_ended
                    }
                    (MouseButton::Right, ElementState::Pressed) => {
                        self.secondary_down_handled =
                            self.ui.dispatch_secondary_tap_down(cursor);
                        self.secondary_down_handled
                    }
                    (MouseButton::Right, ElementState::Released) => {
                        let up = self.ui.dispatch_secondary_tap_up(cursor);
                        let tap = self.ui.dispatch_secondary_tap(cursor);
                        if !up && !tap && !self.secondary_down_handled {
                            // Nothing claimed the right-click (no widget context
                            // menu, no blocker) — open the global menu.
                            pebbles_widgets::global_menu::show(cursor.x, cursor.y);
                            self.request_redraw();
                        }
                        up || tap
                    }
                    (MouseButton::Middle, ElementState::Pressed) => {
                        self.ui.dispatch_tertiary_down(cursor)
                    }
                    (MouseButton::Middle, ElementState::Released) => {
                        self.ui.dispatch_tertiary_up(cursor)
                    }
                    _ => false,
                };
                if handled {
                    self.request_redraw();
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
                    // F2: Mod+Shift+I toggles the widget inspector (devtools v1).
                    if (self.ctrl_down || self.meta_down)
                        && self.shift_down
                        && to_shortcut_key(&event) == Some(pebbles_core::ShortcutKey::Char('i'))
                    {
                        self.inspect_mode = !self.inspect_mode;
                        self.inspect_rect = None;
                        self.request_redraw();
                        return;
                    }
                    // Escape closes an open (dismissible) sheet or modal dialog first.
                    if event.logical_key == Key::Named(NamedKey::Escape)
                        && (pebbles_widgets::sheet::is_open() || pebbles_widgets::dialog::is_open())
                    {
                        pebbles_widgets::sheet::dismiss_top();
                        pebbles_widgets::dialog::dismiss_top();
                        self.request_redraw();
                        return;
                    }
                    // B2 precedence: focused editor → shortcuts → Tab → activation.
                    let intent = to_command(&event, self.ctrl_down, self.shift_down);
                    let mods = pebbles_core::Mods {
                        shift: self.shift_down,
                        ctrl: self.ctrl_down,
                        alt: self.alt_down,
                        meta: self.meta_down,
                    };
                    let mut handled = if intent.is_some_and(|ki| self.ui.dispatch_key(ki)) {
                        true
                    } else if to_shortcut_key(&event)
                        .is_some_and(|sk| pebbles_core::shortcuts::dispatch(self.ui.window_id(), mods, sk))
                    {
                        true
                    } else if event.logical_key == Key::Named(NamedKey::Tab) {
                        self.ui.focus_move(!self.shift_down)
                    } else {
                        matches!(
                            event.logical_key.as_ref(),
                            Key::Named(NamedKey::Enter | NamedKey::Space) | Key::Character(" ")
                        ) && self.ui.dispatch_activate()
                    };
                    // If nothing else claimed the key, use it to scroll the view
                    // under the pointer (PageUp/Down, Home/End, arrows, Space).
                    if !handled {
                        handled = self.scroll_key(&event);
                    }
                    if handled {
                        self.request_redraw();
                    }
                }
            }

            // IME composition (CJK, dead keys). Preedit = the in-progress underline;
            // Commit = the finished text, inserted like a normal keystroke.
            WindowEvent::Ime(ime) => {
                let handled = match ime {
                    Ime::Preedit(text, _cursor) => self.ui.dispatch_key(KeyInput::Preedit(text)),
                    Ime::Commit(text) => self.ui.dispatch_key(KeyInput::Insert(text)),
                    Ime::Enabled | Ime::Disabled => false,
                };
                if handled {
                    self.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => self.render(),

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // B3: deliver native-menu clicks on the UI thread (make_current so callback
        // signal writes mark the right components dirty), then repaint if anything ran.
        #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
        {
            self.ui.make_current();
            let fired = self.native_menu.as_mut().map(|m| m.drain()).unwrap_or(false);
            if fired {
                self.request_redraw();
                for w in self.windows.values() {
                    w.window.request_redraw();
                }
            }
        }
        // B4: deliver global-hotkey presses the same way.
        #[cfg(feature = "global-hotkeys")]
        {
            self.ui.make_current();
            if crate::hotkeys::drain() {
                self.request_redraw();
                for w in self.windows.values() {
                    w.window.request_redraw();
                }
            }
        }
        // F5: refresh the monitor snapshot (set_monitors is a no-op when unchanged).
        {
            let primary = event_loop.primary_monitor();
            let list: Vec<pebbles_widgets::MonitorInfo> = event_loop
                .available_monitors()
                .map(|m| {
                    let pos = m.position();
                    let size = m.size();
                    pebbles_widgets::MonitorInfo {
                        primary: primary.as_ref() == Some(&m),
                        name: m.name().unwrap_or_default(),
                        position: (pos.x, pos.y),
                        size: (size.width, size.height),
                        scale: m.scale_factor(),
                    }
                })
                .collect();
            pebbles_widgets::set_monitors(list);
        }
        // GPU errors landed this turn? Wake the render loop so the recovery
        // reset actually runs (an idle `Wait` loop would otherwise sit on a
        // poisoned device until the next input event).
        if GPU_ERRORS.load(std::sync::atomic::Ordering::Relaxed) != self.gpu_errors_seen {
            self.request_redraw();
            for w in self.windows.values() {
                w.window.request_redraw();
            }
        }
        // Open/close any secondary windows requested since the last turn.
        self.pump_windows(event_loop);
        // A reactive write (from an effect, timer, etc.) requests a new frame — on the
        // main window and every secondary window (they share the reactive runtime, so
        // a signal written in one repaints the others).
        if pebbles_core::reactive::frame_requested() {
            self.request_redraw();
            for w in self.windows.values() {
                w.window.request_redraw();
            }
        }
        // Sleep until the next OS event instead of winit 0.30's DEFAULT `Poll`, which
        // busy-spins this callback at 100% of a core for the app's entire lifetime.
        // Nothing is lost: input and `request_redraw` wake a waiting loop, `render()`
        // re-requests frames while animations/scroll springs/background tasks are
        // live, and a pending long-press overrides this with `WaitUntil` below.
        event_loop.set_control_flow(ControlFlow::Wait);
        // Recognize a long press once held past the deadline: fires on_long_press +
        // on_long_press_start, then the gesture becomes "active" (moves/end follow).
        if let Some(deadline) = self.press_deadline {
            if Instant::now() >= deadline {
                self.press_deadline = None;
                let cursor = self.cursor;
                if let Some(t) = self.lp_target {
                    self.lp_active = true;
                    self.ui.make_current(); // long-press handlers may open a popover
                    if self.ui.dispatch_long_press_begin(t, cursor) {
                        self.request_redraw();
                    }
                }
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        }
        // Wake exactly when the next pending timeout (tooltip / hover-card delays,
        // create_timeout callbacks) is due — a fully still mouse must still show
        // the tooltip. No pending timers means plain `Wait` above.
        let now_secs = self.clock.elapsed().as_secs_f64();
        if let Some(at) = pebbles_core::animation::next_deadline(now_secs) {
            let remaining = (at - now_secs).max(0.0);
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_secs_f64(remaining),
            ));
        }
    }
}
