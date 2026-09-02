//! The desktop app runner: a winit [`ApplicationHandler`] that owns the wgpu
//! surface, the Vello GPU renderer, and the [`Ui`] engine, and drives the
//! reconcile → layout → paint → present loop.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pebbles_core::{IntoWidget, KeyInput, Motion, Ui};
use pebbles_foundation::{Color, Offset, Size, TextDirection, palette};
use pebbles_widgets::{MenuBar, View};
use vello::kurbo::Affine;
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

use pebbles_render::{Cursor, TextEnv};

/// C6 — route a wheel `dy` when an overlay popover is open, so a scroll behind the
/// popover slides it to stay glued to its trigger (and dismisses it when nothing
/// scrolls). Shared by the main window and every secondary window — the overlay
/// signals are per-window, so `ui` must be the current window first (both callers
/// make it current before calling this). Returns whether a repaint is needed.
fn wheel_with_overlay(ui: &mut Ui, cursor: Offset, dy: f64) -> bool {
    use pebbles_widgets::overlay;
    if !overlay::is_open() {
        return ui.dispatch_scroll(cursor, dy);
    }
    if overlay::over_panel(cursor.x, cursor.y) {
        // Wheel over the popover itself → scroll its own content only.
        ui.dispatch_scroll(cursor, dy)
    } else if ui.dispatch_scroll(cursor, dy) {
        // Wheel over the page behind the popover → slide the popover with it.
        overlay::shift(0.0, -dy);
        true
    } else {
        // Nowhere to scroll → dismiss so it never floats detached.
        overlay::hide_overlay();
        true
    }
}

/// E2: whether `PEBBLES_FRAME_STATS=1` (or `true`) is set — checked once. Gates the
/// opt-in per-frame timing print in `render()`.
fn frame_stats_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("PEBBLES_FRAME_STATS").is_ok_and(|v| v == "1" || v == "true")
    })
}

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

/// Map a winit key to a [`ShortcutKey`] token (B2) — `None` for keys outside
/// the shortcut grammar.
fn to_shortcut_key(event: &KeyEvent) -> Option<pebbles_core::ShortcutKey> {
    use pebbles_core::ShortcutKey as SK;
    use winit::keyboard::{Key, NamedKey};
    match event.logical_key.as_ref() {
        Key::Named(n) => match n {
            NamedKey::Enter => Some(SK::Enter),
            NamedKey::Escape => Some(SK::Escape),
            NamedKey::Space => Some(SK::Space),
            NamedKey::Tab => Some(SK::Tab),
            NamedKey::ArrowUp => Some(SK::ArrowUp),
            NamedKey::ArrowDown => Some(SK::ArrowDown),
            NamedKey::ArrowLeft => Some(SK::ArrowLeft),
            NamedKey::ArrowRight => Some(SK::ArrowRight),
            NamedKey::Home => Some(SK::Home),
            NamedKey::End => Some(SK::End),
            NamedKey::Delete => Some(SK::Delete),
            NamedKey::Backspace => Some(SK::Backspace),
            NamedKey::F1 => Some(SK::F(1)),
            NamedKey::F2 => Some(SK::F(2)),
            NamedKey::F3 => Some(SK::F(3)),
            NamedKey::F4 => Some(SK::F(4)),
            NamedKey::F5 => Some(SK::F(5)),
            NamedKey::F6 => Some(SK::F(6)),
            NamedKey::F7 => Some(SK::F(7)),
            NamedKey::F8 => Some(SK::F(8)),
            NamedKey::F9 => Some(SK::F(9)),
            NamedKey::F10 => Some(SK::F(10)),
            NamedKey::F11 => Some(SK::F(11)),
            NamedKey::F12 => Some(SK::F(12)),
            _ => None,
        },
        Key::Character(s) => {
            let mut chars = s.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if !c.is_control() => Some(SK::Char(c.to_ascii_lowercase())),
                _ => None,
            }
        }
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
    min_size: Option<(u32, u32)>,
    max_size: Option<(u32, u32)>,
    position: Option<(i32, i32)>,
    resizable: bool,
    maximized: bool,
    decorations: bool,
    root: Option<pebbles_core::AnyWidget>,
    /// B3 native menu bar spec. Consumed by the shell only when the `native-menus`
    /// feature is on; retained (unused) otherwise so app code compiles either way.
    menu: Option<MenuBar>,
    /// D2 global text direction, applied at mount (default LTR).
    text_direction: TextDirection,
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
            min_size: None,
            max_size: None,
            position: None,
            resizable: true,
            maximized: false,
            decorations: true,
            root: Some(pebbles_widgets::OverlayHost::wrap(root).into_widget()),
            menu: None,
            text_direction: TextDirection::Ltr,
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

    /// The smallest the user can resize the window to (logical px).
    pub fn min_size(mut self, width: u32, height: u32) -> Self {
        self.min_size = Some((width, height));
        self
    }

    /// The largest the user can resize the window to (logical px).
    pub fn max_size(mut self, width: u32, height: u32) -> Self {
        self.max_size = Some((width, height));
        self
    }

    /// The window's initial top-left position (logical px).
    pub fn position(mut self, x: i32, y: i32) -> Self {
        self.position = Some((x, y));
        self
    }

    /// Whether the user can resize the window (default `true`).
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Open the window maximized.
    pub fn maximized(mut self, maximized: bool) -> Self {
        self.maximized = maximized;
        self
    }

    /// Whether the OS draws the title bar / borders (default `true`).
    pub fn decorations(mut self, decorations: bool) -> Self {
        self.decorations = decorations;
        self
    }

    /// Attach a native OS menu bar (B3) — macOS global menu / Windows window menu.
    /// Built from [`menu_bar`](pebbles_widgets::menu_bar); only takes effect when the
    /// `native-menus` feature is enabled (otherwise the spec is retained but unused,
    /// and the in-window [`menubar`](pebbles_widgets::components::menubar) stays the
    /// cross-platform form).
    ///
    /// ```ignore
    /// use pebbles_widgets::{menu, menu_bar};
    /// App::new(root).menu(menu_bar([
    ///     menu("File", [menu_item("Quit").shortcut("Mod+Q").into()]),
    /// ]))
    /// ```
    pub fn menu(mut self, bar: MenuBar) -> Self {
        self.menu = Some(bar);
        self
    }

    /// Set the global text direction (D2). `Rtl` reverses Row child order + mirrors
    /// Start/End alignment, and sets paragraphs' bidi base direction. Applied at mount;
    /// toggle at runtime with [`pebbles_widgets::set_text_direction`].
    pub fn text_direction(mut self, dir: TextDirection) -> Self {
        self.text_direction = dir;
        self
    }

    /// Register a user-supplied font (F4), repeatable. `bytes` is `'static` (embed with
    /// `include_bytes!` or leak an `Arc`) so it outlives every window's font collection.
    /// Every window then resolves the font's family via `style().font_family("…")`.
    /// Registered globally at call time, so call this before [`run`](App::run).
    pub fn font(self, bytes: &'static [u8]) -> Self {
        pebbles_render::register_user_font(bytes);
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
struct Runner {
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
    /// The live native menu (built + attached in `resumed`, drained each turn).
    #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
    native_menu: Option<crate::native_menu::NativeMenus>,

    // gpu
    context: RenderContext,
    renderers: Vec<Option<Renderer>>,
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
    fn new(app: App) -> Self {
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
            #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
            native_menu: None,
            context: RenderContext::new(),
            renderers: Vec::new(),
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
        // Deliver any finished background tasks (network image loads, create_resource
        // fetches, spawn callbacks) — writes their result signals on the UI thread.
        let pending_tasks = pebbles_core::task::pump();

        // D1: apply AT-driven actions (Focus/Click) queued off-thread by the accesskit
        // handler; their signal writes are reconciled by `rebuild_if_dirty` below.
        let window = self.ui.window_id();
        crate::a11y::drain_actions(&mut self.ui, window);

        let Some(active) = self.active.as_mut() else { return };

        let stats = frame_stats_enabled();

        // 1. Reconcile. (May register fresh animation tracks as components render.)
        let t = Instant::now();
        self.ui.rebuild_if_dirty();
        let rebuild = t.elapsed();

        // 2. Layout in logical pixels.
        let scale = active.window.scale_factor();
        let phys = active.window.inner_size();
        if phys.width == 0 || phys.height == 0 {
            return;
        }
        let logical = Size::new(phys.width as f64 / scale, phys.height as f64 / scale);
        pebbles_widgets::overlay::set_window_size(logical.width, logical.height);
        let t = Instant::now();
        self.ui.layout(&mut self.text, logical);
        let layout = t.elapsed();

        // 3. Paint the logical scene, then scale it to physical pixels.
        let t = Instant::now();
        self.scene.reset();
        self.ui.paint(&mut self.scene);
        self.frame.reset();
        self.frame.append(&self.scene, Some(Affine::scale(scale)));
        let encode = t.elapsed();

        // E2: opt-in per-frame CPU-side timing (`PEBBLES_FRAME_STATS=1`). GPU submit
        // below is excluded — this measures rebuild/layout/scene-encode, the parts a
        // damage/relayout optimization would target. Object count is the debug census.
        if stats {
            #[cfg(debug_assertions)]
            let objects = self.ui.render_node_count().to_string();
            #[cfg(not(debug_assertions))]
            let objects = String::from("n/a (debug-only census)");
            eprintln!(
                "[pebbles frame] rebuild={:.2}ms layout={:.2}ms encode={:.2}ms objects={objects}",
                rebuild.as_secs_f64() * 1e3,
                layout.as_secs_f64() * 1e3,
                encode.as_secs_f64() * 1e3,
            );
        }

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
        if scrolling || pending_tasks || pebbles_core::animation::active() {
            active.window.request_redraw();
        }

        // 5. Publish the accessibility tree + focus for this frame (post-layout, so
        // bounds are current). The `active` borrow has ended above.
        let nodes = self.ui.render_tree().semantics_tree();
        let focus = pebbles_core::focus::focused_element_ffi(self.ui.window_id());
        if let Some(a11y) = self.a11y.as_mut() {
            a11y.update(&nodes, focus);
        }
    }

    // ------------------------------------------------------- secondary windows

    /// Create/close secondary windows requested by app code this turn.
    fn pump_windows(&mut self, event_loop: &ActiveEventLoop) {
        for spec in pebbles_widgets::window::take_open_requests() {
            self.open_window(event_loop, spec);
        }
        for id in pebbles_widgets::window::take_close_requests() {
            if let Some(wid) = self.window_by_id.remove(&id)
                && let Some(w) = self.windows.remove(&wid)
                && let Some(f) = &w.on_close
            {
                f();
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
    }

    fn open_window(&mut self, event_loop: &ActiveEventLoop, spec: pebbles_widgets::window::WindowSpec) {
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
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        window.set_ime_allowed(true); // enable IME composition on secondary windows too
        let physical = window.inner_size();
        let surface = pollster::block_on(self.context.create_surface(
            window.clone(),
            physical.width.max(1),
            physical.height.max(1),
            wgpu::PresentMode::AutoVsync,
        ))
        .expect("create surface");
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
                surface,
                ui,
                background: spec.background,
                cursor: Offset::ZERO,
                armed_tap: None,
                pan_target: None,
                current_cursor: Cursor::Default,
                on_close: spec.on_close,
            },
        );
    }

    /// Handle a window event addressed to a secondary window.
    fn secondary_event(&mut self, window_id: WindowId, event: WindowEvent) {
        let Some(mut w) = self.windows.remove(&window_id) else { return };
        // Route window-scoped globals (overlay/dialog signals) to THIS window while we
        // handle its input — event handlers don't otherwise set the current window.
        w.ui.make_current();
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
                if size.width > 0 && size.height > 0 {
                    self.context.resize_surface(&mut w.surface, size.width, size.height);
                    w.window.request_redraw();
                }
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
                        w.pan_target =
                            if claimed { None } else { w.ui.pan_target_at(cursor) };
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
                    // Escape closes this window's open (dismissible) modal dialog first.
                    if event.logical_key == Key::Named(NamedKey::Escape)
                        && pebbles_widgets::dialog::is_open()
                    {
                        pebbles_widgets::dialog::dismiss_top();
                        w.window.request_redraw();
                        if keep {
                            self.windows.insert(window_id, w);
                        }
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
        }
    }

    /// Render one secondary window (mirrors [`render`], reusing the shared scene,
    /// renderers and GPU context).
    fn render_window(&mut self, w: &mut WindowRuntime) {
        let now = self.clock.elapsed().as_secs_f64();
        pebbles_core::animation::tick(now);
        let pending_tasks = pebbles_core::task::pump();

        w.ui.rebuild_if_dirty();
        let scale = w.window.scale_factor();
        let phys = w.window.inner_size();
        if phys.width == 0 || phys.height == 0 {
            return;
        }
        let logical = Size::new(phys.width as f64 / scale, phys.height as f64 / scale);
        // Publish this window's size so its popovers can flip/shift on-screen.
        w.ui.make_current();
        pebbles_widgets::overlay::set_window_size(logical.width, logical.height);
        w.ui.layout(&mut self.text, logical);

        self.scene.reset();
        w.ui.paint(&mut self.scene);
        self.frame.reset();
        self.frame.append(&self.scene, Some(Affine::scale(scale)));

        let surface = &w.surface;
        let device_handle = &self.context.devices[surface.dev_id];
        let renderer = self.renderers[surface.dev_id].as_mut().expect("renderer");
        renderer
            .render_to_texture(
                &device_handle.device,
                &device_handle.queue,
                &self.frame,
                &surface.target_view,
                &RenderParams {
                    base_color: w.background,
                    width: phys.width,
                    height: phys.height,
                    antialiasing_method: AaConfig::Area,
                },
            )
            .expect("vello render");
        let surface_texture = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return,
        };
        let mut encoder = device_handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.window.blit") });
        let target_view =
            surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        surface.blitter.copy(&device_handle.device, &mut encoder, &surface.target_view, &target_view);
        device_handle.queue.submit([encoder.finish()]);
        w.window.pre_present_notify();
        surface_texture.present();

        if pending_tasks || pebbles_core::animation::active() {
            w.window.request_redraw();
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
                Instant::now() + std::time::Duration::from_secs_f64(remaining),
            ));
        }
    }
}
