//! The desktop app runner: a winit [`ApplicationHandler`] that owns the wgpu
//! surface, the Vello GPU renderer, and the [`Ui`] engine, and drives the
//! reconcile → layout → paint → present loop.

use std::sync::Arc;
use std::time::{Duration, Instant};

use pebbles_core::{IntoWidget, KeyInput, Motion, Ui, WidgetExt};
use pebbles_foundation::{Color, Offset, Size, palette};
use pebbles_widgets::View;
use vello::kurbo::Affine;
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

use pebbles_render::{Cursor, TextEnv};

fn to_winit_cursor(cursor: Cursor) -> CursorIcon {
    match cursor {
        Cursor::Default => CursorIcon::Default,
        Cursor::Pointer => CursorIcon::Pointer,
        Cursor::Text => CursorIcon::Text,
        Cursor::Grab => CursorIcon::Grab,
        Cursor::Grabbing => CursorIcon::Grabbing,
        Cursor::ColResize => CursorIcon::ColResize,
        Cursor::RowResize => CursorIcon::RowResize,
        Cursor::NotAllowed => CursorIcon::NotAllowed,
    }
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

/// Translate a winit key press (+ Ctrl/Shift) into an editing command, or `None`
/// if it isn't one the focused editor cares about. Shift extends selections; Ctrl
/// switches arrows to word motion and enables the clipboard/undo/select-all set.
fn to_command(event: &KeyEvent, ctrl: bool, shift: bool) -> Option<KeyInput> {
    use KeyInput::*;
    use Motion::*;
    let mv = |m: Motion| Some(Move { motion: m, extend: shift });
    match event.logical_key.as_ref() {
        Key::Named(NamedKey::Backspace) => Some(if ctrl { DeleteWordBack } else { Backspace }),
        Key::Named(NamedKey::Delete) => Some(if ctrl { DeleteWordForward } else { Delete }),
        Key::Named(NamedKey::ArrowLeft) => mv(if ctrl { WordLeft } else { Left }),
        Key::Named(NamedKey::ArrowRight) => mv(if ctrl { WordRight } else { Right }),
        Key::Named(NamedKey::ArrowUp) => mv(Up),
        Key::Named(NamedKey::ArrowDown) => mv(Down),
        Key::Named(NamedKey::Home) => mv(if ctrl { DocStart } else { LineStart }),
        Key::Named(NamedKey::End) => mv(if ctrl { DocEnd } else { LineEnd }),
        Key::Named(NamedKey::Enter) => Some(Enter),
        Key::Named(NamedKey::Escape) => Some(Escape),
        Key::Named(NamedKey::Space) if !ctrl => Some(Insert(" ".to_string())),
        Key::Character(s) if ctrl => match s.to_lowercase().as_str() {
            "a" => Some(SelectAll),
            "c" => Some(Copy),
            "x" => Some(Cut),
            "v" => Some(Paste),
            "z" => Some(if shift { Redo } else { Undo }),
            "y" => Some(Redo),
            _ => None,
        },
        Key::Character(s) if s.chars().all(|ch| !ch.is_control()) => Some(Insert(s.to_string())),
        _ => None,
    }
}

/// Max interval between two primary clicks to count as a double-tap.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How long the primary button must be held to count as a long-press.
const LONG_PRESS: Duration = Duration::from_millis(500);
/// Logical pixels scrolled per wheel line.
const LINE_SCROLL: f64 = 48.0;

/// A Pebbles desktop application. Configure it fluently, then [`run`](App::run).
///
/// ```ignore
/// App::new(my_root_widget())
///     .title("Counter")
///     .size(480, 320)
///     .run()?;
/// ```
pub struct App {
    title: String,
    background: Color,
    size: (u32, u32),
    root: Option<pebbles_core::AnyWidget>,
}

impl App {
    /// Create an app with `root` as its top-level widget. The root is wrapped in an
    /// [`OverlayHost`](pebbles_widgets::OverlayHost) so dropdowns/menus/popovers can
    /// paint above everything.
    pub fn new(root: impl IntoWidget) -> Self {
        App {
            title: "Pebbles".to_owned(),
            background: palette::WHITE,
            size: (800, 600),
            root: Some(pebbles_widgets::OverlayHost::wrap(root).into_widget()),
        }
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// The window background color (also the root `View`'s fill).
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    pub fn size(mut self, width: u32, height: u32) -> Self {
        self.size = (width, height);
        self
    }

    /// Open the window and run the event loop until the window closes.
    pub fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let event_loop = EventLoop::new()?;
        let mut runner = Runner::new(self);
        event_loop.run_app(&mut runner)?;
        Ok(())
    }
}

/// The live window + its GPU surface.
struct ActiveWindow {
    window: Arc<Window>,
    surface: RenderSurface<'static>,
}

/// The winit application handler. Holds all persistent runtime state.
struct Runner {
    // configuration
    title: String,
    background: Color,
    size: (u32, u32),
    pending_root: Option<pebbles_core::AnyWidget>,

    // gpu
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
    active: Option<ActiveWindow>,

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
    current_cursor: Cursor,
    /// Deadline at which a held primary button becomes a long-press.
    press_deadline: Option<Instant>,
    shift_down: bool,
    ctrl_down: bool,
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
}

impl Runner {
    fn new(app: App) -> Self {
        Runner {
            title: app.title,
            background: app.background,
            size: app.size,
            pending_root: app.root,
            context: RenderContext::new(),
            renderers: Vec::new(),
            active: None,
            ui: Ui::new(),
            text: TextEnv::new(),
            scene: Scene::new(),
            frame: Scene::new(),
            mounted: false,
            cursor: Offset::ZERO,
            last_click: None,
            last_tap_target: None,
            current_cursor: Cursor::Default,
            press_deadline: None,
            shift_down: false,
            ctrl_down: false,
            armed_tap: None,
            lp_target: None,
            lp_active: false,
            pan_target: None,
            clock: Instant::now(),
            last_frame_t: 0.0,
        }
    }

    fn request_redraw(&self) {
        if let Some(active) = self.active.as_ref() {
            active.window.request_redraw();
        }
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

    /// Reconcile any dirty subtrees, lay out to the window, paint, and present.
    fn render(&mut self) {
        // Advance animations for this frame before reconciling, so interpolated
        // signal writes mark their components dirty and get picked up below.
        let now = self.clock.elapsed().as_secs_f64();
        let dt = now - self.last_frame_t;
        self.last_frame_t = now;
        pebbles_core::animation::tick(now);
        // Advance scroll-spring physics (smooth wheel / keyboard momentum).
        let scrolling = self.ui.tick_scrolls(dt);

        let Some(active) = self.active.as_mut() else { return };

        // 1. Reconcile. (May register fresh animation tracks as components render.)
        self.ui.rebuild_if_dirty();

        // 2. Layout in logical pixels.
        let scale = active.window.scale_factor();
        let phys = active.window.inner_size();
        if phys.width == 0 || phys.height == 0 {
            return;
        }
        let logical = Size::new(phys.width as f64 / scale, phys.height as f64 / scale);
        pebbles_widgets::overlay::set_window_size(logical.width, logical.height);
        self.ui.layout(&mut self.text, logical);

        // 3. Paint the logical scene, then scale it to physical pixels.
        self.scene.reset();
        self.ui.paint(&mut self.scene);
        self.frame.reset();
        self.frame.append(&self.scene, Some(Affine::scale(scale)));

        // 4. Render to the offscreen target and blit to the surface.
        let surface = &active.surface;
        let device_handle = &self.context.devices[surface.dev_id];
        let renderer = self.renderers[surface.dev_id]
            .as_mut()
            .expect("renderer initialized for this device");

        renderer
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                &self.frame,
                &surface.target_view,
                &RenderParams {
                    base_color: self.background,
                    width: phys.width,
                    height: phys.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("vello render");

        let surface_texture = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            // Timeout / occluded / outdated / lost — skip this frame and try again.
            _ => return,
        };
        let mut encoder =
            device_handle.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("pebbles.blit"),
            });
        let target_view =
            surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        surface.blitter.copy(
            &device_handle.device,
            &mut encoder,
            &surface.target_view,
            &target_view,
        );
        device_handle.queue.submit([encoder.finish()]);
        active.window.pre_present_notify();
        surface_texture.present();

        // Keep the frames coming while any animation or scroll spring is running.
        if scrolling || pebbles_core::animation::active() {
            active.window.request_redraw();
        }
    }
}

impl ApplicationHandler for Runner {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.active.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title(self.title.clone())
            .with_inner_size(LogicalSize::new(self.size.0, self.size.1));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

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
        self.renderers[surface.dev_id].get_or_insert_with(|| {
            Renderer::new(
                &self.context.devices[surface.dev_id].device,
                RendererOptions {
                    use_cpu: false,
                    antialiasing_support: AaSupport::all(),
                    num_init_threads: None,
                    pipeline_cache: None,
                },
            )
            .expect("create vello renderer")
        });

        // Mount the widget tree once, wrapped in the root View.
        if !self.mounted {
            pebbles_core::focus::init(); // create the global focus signal (before any component)
            pebbles_widgets::overlay::init(); // create the global overlay signal too
            install_clipboard(); // wire the system clipboard for Ctrl+C/X/V
            let root = self.pending_root.take().expect("root widget");
            self.ui.mount_root(View::new(self.background, root).boxed());
            self.mounted = true;
        }

        window.request_redraw();
        self.active = Some(ActiveWindow { window, surface });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
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
                // Feed movement into an active drag.
                if let Some(t) = self.pan_target
                    && self.ui.dispatch_pan_update(t, cursor)
                {
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
                // Scrolling dismisses any open popover (standard dropdown behavior),
                // so it never gets "left behind" the trigger.
                if pebbles_widgets::overlay::is_open() {
                    pebbles_widgets::overlay::hide_overlay();
                    self.request_redraw();
                }
                let cursor = self.cursor;
                if self.ui.dispatch_scroll(cursor, dy) {
                    self.request_redraw();
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let cursor = self.cursor;
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
                        // Arm and begin a drag if a pan listener is under the pointer.
                        self.pan_target = self.ui.pan_target_at(cursor);
                        let panned =
                            self.pan_target.is_some_and(|t| self.ui.dispatch_pan_start(t, cursor));
                        self.ui.dispatch_pointer_down(cursor) || panned
                    }
                    (MouseButton::Left, ElementState::Released) => {
                        // End any scrollbar / content drag.
                        self.ui.end_scrollbar_drag();
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
                        up || result
                    }
                    (MouseButton::Right, ElementState::Pressed) => {
                        self.ui.dispatch_secondary_tap_down(cursor)
                    }
                    (MouseButton::Right, ElementState::Released) => {
                        let up = self.ui.dispatch_secondary_tap_up(cursor);
                        up | self.ui.dispatch_secondary_tap(cursor)
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
                pebbles_core::keyboard::set_modifiers(self.shift_down, self.ctrl_down);
            }

            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    // Tab always moves focus. Otherwise, translate to an edit intent
                    // and route to the focused text editor; if none consumes it,
                    // fall back to Enter/Space activation.
                    let mut handled = if event.logical_key == Key::Named(NamedKey::Tab) {
                        self.ui.focus_move(!self.shift_down)
                    } else {
                        let intent = to_command(&event, self.ctrl_down, self.shift_down);
                        let consumed = intent.is_some_and(|ki| self.ui.dispatch_key(ki));
                        consumed
                            || matches!(
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

            WindowEvent::RedrawRequested => self.render(),

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // A reactive write (from an effect, timer, etc.) requests a new frame.
        if pebbles_core::reactive::frame_requested() {
            self.request_redraw();
        }
        // Recognize a long press once held past the deadline: fires on_long_press +
        // on_long_press_start, then the gesture becomes "active" (moves/end follow).
        if let Some(deadline) = self.press_deadline {
            if Instant::now() >= deadline {
                self.press_deadline = None;
                let cursor = self.cursor;
                if let Some(t) = self.lp_target {
                    self.lp_active = true;
                    if self.ui.dispatch_long_press_begin(t, cursor) {
                        self.request_redraw();
                    }
                }
            } else {
                event_loop.set_control_flow(ControlFlow::WaitUntil(deadline));
            }
        }
    }
}
