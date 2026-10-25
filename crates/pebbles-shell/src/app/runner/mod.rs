//! The winit [`ApplicationHandler`] engine behind [`App`](super::App): owns the wgpu
//! surface(s), the `pebbles-gpu` renderer, the [`Ui`] engine(s), and all persistent input
//! state, and drives the reconcile → layout → paint → present loop.
//!
//! Split by concern: [`input`] (event → intent translation), [`render`] (the frame
//! pipeline), [`windows`] (secondary OS windows). Child modules reach the parent's
//! private state directly; the [`ApplicationHandler`] impl stays here and delegates.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use web_time::Instant;

use frame::{FrameOutcome, FramePacket, FrameSink, Handover, Pacer, RenderState, ResizeSync};
use gpu::{HostInit, RenderContext, RenderSurface, Scene, new_context};
use kurbo::Affine;
use pebbles_core::{IntoWidget, KeyInput, Motion, Ui};
use pebbles_foundation::{Color, Offset, Size, TextDirection};
use pebbles_widgets::{MenuBar, View};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, Ime, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::EventLoopProxy;
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{Key, NamedKey};
use winit::window::{CursorIcon, Window, WindowAttributes, WindowId};

use pebbles_render::{Cursor, SystemFonts, TextEnv};

use super::App;

mod frame;
mod input;
mod render;
#[cfg(not(target_family = "wasm"))]
mod render_thread;
mod run_stats;
mod soak_csv;
// The GPU host: a thin, runner-private adapter over pebbles-gpu (X02).
mod gpu;
mod storm;
mod windows;

use input::{editor_chord_intent, to_command, to_shortcut_key, to_winit_cursor, wheel_with_overlay};

/// Max interval between two primary clicks to count as a double-tap.
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
/// How long the primary button must be held to count as a long-press.
const LONG_PRESS: Duration = Duration::from_millis(500);
/// Logical pixels scrolled per wheel notch/line. Matched to browsers (Chrome/Firefox
/// scroll ~100 px per notch); the previous 48 was GTK-ish and felt about half-speed.
const LINE_SCROLL: f64 = 100.0;

/// Wire the OS clipboard into `pebbles_core::clipboard`. Falls back to the core's
/// in-process clipboard if the platform clipboard can't be opened (e.g. headless).
///
/// On wasm and Android there is no `arboard` backend, so this is a no-op and the
/// core's in-process clipboard handles intra-app copy/paste. (Web additionally does
/// copy-OUT through `navigator.clipboard.write_text`; only paste-IN stays app-local.
/// Android's JNI backend is a follow-up.)
#[cfg(not(any(target_family = "wasm", target_os = "android", target_os = "ios")))]
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

/// Android / iOS: no `arboard` backend — the core's in-process fallback keeps
/// intra-app copy/paste working (a JNI / UIPasteboard backend is a follow-up).
#[cfg(all(any(target_os = "android", target_os = "ios"), not(target_family = "wasm")))]
fn install_clipboard() {}

/// Web: writes go to the system clipboard via the async `navigator.clipboard`
/// (so text copied in Pebbles pastes into other apps); reads return the last value
/// Pebbles set — a synchronous read of the OS clipboard isn't possible on the web
/// (`readText` is async + permission-gated), so cross-app *paste into* Pebbles is a
/// follow-up. Intra-app copy/paste works fully. See documentations/web-support.md §4.2.
#[cfg(target_family = "wasm")]
fn install_clipboard() {
    use std::cell::RefCell;
    use std::rc::Rc;
    let last = Rc::new(RefCell::new(String::new()));
    let reader = last.clone();
    pebbles_core::clipboard::install(
        move || reader.borrow().clone(),
        move |text| {
            *last.borrow_mut() = text.to_string();
            // Best-effort push to the OS clipboard (async Promise, fire-and-forget).
            if let Some(win) = web_sys::window() {
                let _ = win.navigator().clipboard().write_text(text);
            }
        },
    );
}

/// A user event delivered into the winit loop from off the event-callback stack.
///
/// The only carrier today is **web GPU bring-up**: requesting the wgpu adapter +
/// device is inherently async and must not block the browser main thread, so on
/// wasm the surface is built on a `spawn_local` task and the finished
/// [`RenderContext`] + [`RenderSurface`] are handed back here (via an
/// [`EventLoopProxy`]) to be installed on the UI turn (see
/// [`Runner::user_event`]). Desktop builds the surface synchronously and never
/// sends one — the variant is dead there.
#[cfg_attr(not(target_family = "wasm"), allow(dead_code))]
// One transient event in flight at a time (a startup handoff / an IME wake), never
// stored in bulk — boxing the big variant would just add an allocation per frame.
#[allow(clippy::large_enum_variant)]
pub(crate) enum PebblesUserEvent {
    /// The async GPU init finished: install the context + first surface.
    SurfaceReady { context: RenderContext, surface: RenderSurface<'static>, window: Arc<Window> },
    /// (web) The hidden-input IME bridge queued edit intents — wake to drain them.
    WebImeInput,
    /// (web) The browser URL changed (Back/Forward/popstate) — the router already
    /// updated its location; wake to re-render at the new route.
    RouteChanged,
    /// (web) An async GPU recovery could not rebuild the context or surface (08 §6.6).
    GpuRecoveryFailed { window: Arc<Window>, error: String },
    /// (native) The render thread finished a packet (08 §17.3 step 5).
    #[cfg(not(target_family = "wasm"))]
    FrameDone { key: frame::WindowKey, frame_id: u64, outcome: FrameOutcome },
    /// (native) The render thread lost its device (08 §17.6): one `reason=…: message` line per
    /// lost device. Recover with `ReplaceGpu`.
    #[cfg(not(target_family = "wasm"))]
    GpuDeviceLost { lost: Vec<String> },
    /// (native) A GPU device was lost — wake and recover (08 §6.8).
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // sent only by the native wgpu waker
    GpuLost,
    /// (web) WebGL context lost on the canvas (08 P04): drop the GPU, wait for the restore.
    GpuContextLost,
    /// (web) WebGL context restored on the canvas (08 P04): rebuild the GPU stack.
    GpuContextRestored,
}

/// The live window + its GPU surface.
struct ActiveWindow {
    window: Arc<Window>,
    /// In-flight pacing (08 §17.3).
    pacer: Pacer,
    /// +1 per packet.
    frame_id: u64,
    /// Live-resize sync (08 §17.4).
    resize_sync: ResizeSync,
    /// Not visible: render nothing until a visibility event (08 §5.4).
    occluded: bool,
}

/// A secondary OS window opened via [`window`](fn@pebbles_widgets::window). It has its
/// own [`Ui`] + surface but shares the GPU context, renderers and — crucially — the
/// reactive runtime with the main window, so cross-window state is just a shared
/// signal. Each `Ui` carries a distinct `window_id`, so components never alias.
struct WindowRuntime {
    id: u64,
    window: Arc<Window>,
    /// In-flight pacing (08 §17.3).
    pacer: Pacer,
    /// +1 per packet.
    frame_id: u64,
    /// Live-resize sync (08 §17.4).
    resize_sync: ResizeSync,
    ui: Ui,
    background: Color,
    cursor: Offset,
    armed_tap: Option<u64>,
    pan_target: Option<u64>,
    current_cursor: Cursor,
    on_close: Option<Rc<dyn Fn()>>,
    /// Hidden by the window system (08 §5.7, P12): render nothing, request no frames.
    occluded: bool,
}

/// The winit application handler. Holds all persistent runtime state.
pub(super) struct Runner {
    /// `App::run` entry (10 §12.1, M23) and whether the first present was logged.
    started_at: Instant,
    first_present_logged: bool,
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
    /// Main-window icon (RGBA8 + dims), applied at creation. Read only on desktop
    /// (winit ignores it on web); `allow(dead_code)` keeps the wasm build quiet.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    icon: Option<pebbles_widgets::window::WindowIcon>,
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
    /// Where frames go (08 §17): inline, the render state with the GPU context, renderers and
    /// every window's surface. `None` before the GPU is up (on web, during the async
    /// surface-creation gap, when no frame renders) and while a recovery rebuilds it.
    sink: Option<FrameSink>,
    /// Closed windows the render thread has not released yet (08 §17.5): dropped after it stops.
    retired_windows: Vec<Arc<Window>>,
    /// (threaded) The lost-device lines the render thread reported, logged by the next reset.
    #[cfg(not(target_family = "wasm"))]
    lost_lines: Vec<String>,
    /// Live-resize frames that were not presented within `RESIZE_SYNC_TIMEOUT` (08 §17.4).
    resize_sync_timeouts: u64,
    /// The shell-wide platform policy (display handle + env-resolved GPU settings), built
    /// once on the first `resumed` and reused by every recovery.
    host_init: Option<HostInit>,
    /// The loop's proxy: (web) the async GPU-init task delivers the finished surface
    /// through it; (native wgpu) the device-lost waker sends `GpuLost` (P11).
    proxy: Option<EventLoopProxy<PebblesUserEvent>>,
    /// (web) A surface request is in flight — don't start a second one if `resumed`
    /// fires again before it lands.
    #[cfg(target_family = "wasm")]
    gpu_pending: bool,
    /// (web) Whether the opt-in hidden-input IME bridge is on (`App::web_ime`).
    #[cfg(target_family = "wasm")]
    web_ime: bool,
    /// No GPU reset starts before this instant (1 s after each started or failed reset;
    /// P19 swaps in the backoff).
    gpu_retry_at: Option<Instant>,
    /// Consecutive failed rebuilds (0 after a success).
    gpu_reset_failures: u32,
    /// A reset started and has not presented a frame yet (08 §6.7, P19).
    gpu_reset_unconfirmed: bool,
    /// A reset was asked for outside `needs_reset()` (a failed rebuild, C19's `ResetGpu`, P20).
    gpu_reset_requested: bool,
    /// 1 at startup, +1 per rebuilt GPU context (logged with every loss/rebuild line).
    gpu_generation: u64,
    /// (web) The window whose surface a failed async recovery released; the retry uses it.
    #[cfg(target_family = "wasm")]
    recovery_window: Option<Arc<Window>>,
    /// (web) The WebGL context is lost: render and recover nothing until it is restored (P04).
    #[cfg(target_family = "wasm")]
    gpu_suspended: bool,
    /// (web) When the in-flight async recovery started (None at startup).
    #[cfg(target_family = "wasm")]
    recovery_started: Option<Instant>,
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
    /// Consecutive clicks on the same target within the multi-click interval: 1 = single,
    /// 2 = double, 3 = triple (then it cycles back to 1). Drives double/triple-tap.
    click_count: u32,
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

    /// Monotonic frame counter — the diagnostic heartbeat (see `render`).
    frame_no: u64,
    /// Last frame's render-object count — a change means the on-screen content
    /// actually changed (e.g. a navigation swapped screens). Logged in dev mode.
    last_objects: usize,

    /// Dev-only synthetic-input monkey (`PEBBLES_INPUT_STORM=1`); `None` in
    /// normal runs. See [`storm`].
    storm: Option<storm::InputStorm>,
    /// Dev-only secondary-window churn (`PEBBLES_WINDOW_CHURN=1`, S16); `None` normally.
    churn: Option<storm::WindowChurn>,
    /// `PEBBLES_SOAK_CSV` sampler (11 §2.9) and the rendered-frame counter.
    soak: Option<soak_csv::SoakCsv>,
    soak_frames: u64,
    /// `PEBBLES_FRAME_STATS` run summary (`[pebbles run]` lines, scripted desktop runs).
    run_stats: run_stats::RunStats,
}

/// 10 §12.1: once per process, on the first successful present (both backends).
pub(super) fn note_first_present(logged: &mut bool, started_at: Instant, presented: bool) {
    if presented && !std::mem::replace(logged, true) {
        pebbles_foundation::log::logf(
            pebbles_foundation::log::Level::Info,
            pebbles_foundation::log::Cat::Perf,
            format_args!(
                "first frame presented in {:.1} ms since App::run",
                started_at.elapsed().as_secs_f64() * 1000.0
            ),
        );
    }
}

impl Runner {
    /// The `App::run` entry instant (10 §12.1).
    pub(super) fn set_started_at(&mut self, at: Instant) {
        self.started_at = at;
    }

    pub(super) fn new(app: App) -> Self {
        Runner {
            started_at: Instant::now(),
            first_present_logged: false,
            title: app.title,
            background: app.background,
            size: app.size,
            min_size: app.min_size,
            max_size: app.max_size,
            position: app.position,
            resizable: app.resizable,
            maximized: app.maximized,
            decorations: app.decorations,
            icon: app.icon,
            pending_root: app.root,
            menu_spec: app.menu,
            text_direction: app.text_direction,
            inspect_mode: false,
            inspect_rect: None,
            #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
            native_menu: None,
            sink: None,
            retired_windows: Vec::new(),
            #[cfg(not(target_family = "wasm"))]
            lost_lines: Vec::new(),
            resize_sync_timeouts: 0,
            host_init: None,
            proxy: None,
            #[cfg(target_family = "wasm")]
            gpu_pending: false,
            #[cfg(target_family = "wasm")]
            web_ime: app.web_ime,
            gpu_retry_at: None,
            gpu_reset_failures: 0,
            gpu_reset_unconfirmed: false,
            gpu_reset_requested: false,
            gpu_generation: 1,
            #[cfg(target_family = "wasm")]
            recovery_window: None,
            #[cfg(target_family = "wasm")]
            gpu_suspended: false,
            #[cfg(target_family = "wasm")]
            recovery_started: None,
            active: None,
            a11y: None,
            ui: Ui::new(),
            // System fonts scan off the UI thread and merge on a later frame (bundled-only
            // on the web).
            text: TextEnv::with_system_fonts(SystemFonts::Background),
            scene: Scene::new(),
            frame: Scene::new(),
            mounted: false,
            cursor: Offset::ZERO,
            last_click: None,
            last_tap_target: None,
            click_count: 0,
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
            frame_no: 0,
            last_objects: 0,
            storm: storm::InputStorm::from_env(),
            churn: storm::WindowChurn::from_env(),
            soak: soak_csv::SoakCsv::from_env(),
            soak_frames: 0,
            run_stats: run_stats::RunStats::default(),
        }
    }

    fn request_redraw(&self) {
        if let Some(active) = self.active.as_ref() {
            active.window.request_redraw();
        }
    }

    /// (web) Store the proxy the async GPU-init task uses to hand the finished
    /// surface back into the loop. Called once by [`App::run`](super::App::run)
    /// before `spawn_app` (web), or before `run_app` for the device-lost waker (native wgpu).
    pub(super) fn set_proxy(&mut self, proxy: EventLoopProxy<PebblesUserEvent>) {
        self.proxy = Some(proxy);
    }

    /// Build the OS window (or, on web, the canvas) from the app config. Desktop
    /// creates it hidden so the AccessKit adapter can attach before it is shown;
    /// web appends a `<canvas>` to the document body (there is no OS window).
    fn create_window(&self, event_loop: &ActiveEventLoop) -> Arc<Window> {
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
        // Main-window icon (desktop only; winit ignores it on web). Mirrors the
        // secondary-window path — a malformed RGBA buffer is dropped, keeping the OS default.
        #[cfg(not(target_family = "wasm"))]
        if let Some(icon) = &self.icon
            && let Ok(i) = winit::window::Icon::from_rgba(icon.rgba.clone(), icon.width, icon.height)
        {
            attrs = attrs.with_window_icon(Some(i));
        }
        // Desktop: create hidden so the AccessKit adapter can attach before the
        // window is shown (it panics if the window is already visible).
        #[cfg(not(target_family = "wasm"))]
        {
            attrs = attrs.with_visible(false);
        }
        // Web: append the winit-owned canvas to the document body. The Trunk
        // `index.html` sizes it to the viewport with CSS.
        #[cfg(target_family = "wasm")]
        {
            use winit::platform::web::WindowAttributesExtWebSys;
            attrs = attrs.with_append(true);
        }
        Arc::new(event_loop.create_window(attrs).expect("create window"))
    }

    /// Install the GPU state for the first window and mount the widget tree — the
    /// tail shared by the synchronous desktop path (`resumed`) and the async web
    /// path ([`user_event`](Self::user_event)). `self.sink` must be `Some` here.
    fn on_gpu_ready(&mut self, window: Arc<Window>, surface: RenderSurface<'static>) {
        #[cfg(target_family = "wasm")]
        {
            self.gpu_pending = false;
            // One-time browser bridges — install only on the FIRST gpu-ready, not on every
            // resurface / device-loss recovery (a second `web_router::enable` would register a
            // duplicate `popstate` listener and re-seed history to a single entry).
            if !self.mounted {
                // The opt-in hidden-input IME bridge.
                if self.web_ime
                    && let Some(proxy) = self.proxy.clone()
                {
                    crate::web_ime::enable(proxy);
                }
                // The router ↔ browser-URL bridge (history + Back/Forward + deep links).
                // Harmless if the app never navigates.
                if let Some(proxy) = self.proxy.clone() {
                    crate::web_router::enable(proxy);
                }
                // WebGL context loss / restore on this canvas (08 P04), once per canvas.
                if let Some(proxy) = self.proxy.clone() {
                    crate::web_gpu::install_context_loss_listeners(&window, proxy);
                }
            }
        }
        // Adopt the surface and make sure its device has a renderer.
        if let Some(state) = self.render_state_mut() {
            let idx = surface.device_index();
            state.adopt(window.id(), surface);
            state.renderer(idx);
        }

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
        self.active = Some(ActiveWindow {
            window,
            pacer: Pacer::default(),
            frame_id: 0,
            resize_sync: ResizeSync::default(),
            occluded: false,
        });

        // B3: build + attach the native menu bar now that the window exists.
        #[cfg(all(feature = "native-menus", any(target_os = "macos", target_os = "windows")))]
        self.install_native_menu();
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

    /// Dev-mode: a short description of the widget stack under `point` — the
    /// deepest widget's name (+ its parent + element id) — for the input trace,
    /// so the log says WHAT the user clicked, not just where.
    fn hit_desc(&self, point: Offset) -> String {
        let chain = pebbles_render::inspect_at(self.ui.render_tree(), point);
        let Some(deepest) = chain.last() else { return "(empty)".to_string() };
        let src = deepest.source.map(|s| format!("#{s}")).unwrap_or_default();
        let parent =
            if chain.len() >= 2 { format!("{} › ", chain[chain.len() - 2].name) } else { String::new() };
        format!("{parent}{}{src}", deepest.name)
    }

    /// The pointer press/release pipeline — the single source of truth for
    /// scrollbar grab, long-press, content/pan drag, and tap/double-tap
    /// resolution. Called by the real `MouseInput` handler AND by the synthetic
    /// input storm, so the storm exercises the exact real path. Returns whether
    /// the UI consumed the event (the caller requests a redraw if so).
    pub(super) fn dispatch_pointer(
        &mut self,
        cursor: Offset,
        button: MouseButton,
        state: ElementState,
    ) -> bool {
        match (button, state) {
            (MouseButton::Left, ElementState::Pressed) if self.ui.begin_scrollbar_drag(cursor) => {
                // Grabbed a scrollbar — it captures the pointer until release.
                true
            }
            (MouseButton::Left, ElementState::Pressed) => {
                // Blur first: a press clears focus, then the widget under the
                // pointer (button/field) re-grabs it in its own press handler.
                pebbles_core::focus::set_focus(None);
                // Arm the tap (for tap vs. cancel) and the long-press.
                self.armed_tap = self.ui.tap_target_at(cursor);
                self.lp_target = self.ui.long_press_target_at(cursor);
                self.lp_active = false;
                self.press_deadline = self.lp_target.map(|_| Instant::now() + LONG_PRESS);
                if let Some(t) = self.lp_target {
                    self.ui.dispatch_long_press_down(t, cursor);
                }
                // A drag-scroll viewport claims the drag first (A4); a pan-hungry
                // descendant under the pointer wins instead.
                let claimed = self.ui.begin_content_drag(cursor);
                self.pan_target = if claimed { None } else { self.ui.pan_target_at(cursor) };
                let panned = self.pan_target.is_some_and(|t| self.ui.dispatch_pan_start(t, cursor));
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
                } else if self.press_deadline.is_some()
                    && let Some(t) = self.lp_target.take()
                {
                    self.ui.dispatch_long_press_cancel(t);
                }
                self.press_deadline = None;
                let up = self.ui.dispatch_pointer_up(cursor);
                let up_target = self.ui.tap_target_at(cursor);
                let armed = self.armed_tap.take();
                let result = if up_target.is_some() && up_target == armed {
                    // Released over the same widget → tap / double-tap / triple-tap.
                    let now = Instant::now();
                    let within = self.last_tap_target == up_target
                        && self.last_click.is_some_and(|t| now.duration_since(t) <= DOUBLE_CLICK);
                    // 1 → 2 → 3 → 1 for consecutive in-window clicks on the same target.
                    self.click_count = if within { (self.click_count % 3) + 1 } else { 1 };
                    self.last_click = Some(now);
                    self.last_tap_target = up_target;
                    let multi = match self.click_count {
                        3 => self.ui.dispatch_triple_tap(cursor),
                        2 => self.ui.dispatch_double_tap(cursor),
                        _ => false,
                    };
                    // Fall back to a plain tap when the multi-tap isn't handled.
                    if multi { true } else { self.ui.dispatch_tap(cursor) }
                } else if let Some(a) = armed {
                    // Released off the armed widget → cancel.
                    self.ui.dispatch_tap_cancel(a)
                } else {
                    false
                };
                up || result || drag_ended
            }
            (MouseButton::Right, ElementState::Pressed) => {
                self.secondary_down_handled = self.ui.dispatch_secondary_tap_down(cursor);
                self.secondary_down_handled
            }
            (MouseButton::Right, ElementState::Released) => {
                let up = self.ui.dispatch_secondary_tap_up(cursor);
                let tap = self.ui.dispatch_secondary_tap(cursor);
                if !up && !tap && !self.secondary_down_handled {
                    // Nothing claimed the right-click — open the global menu.
                    pebbles_widgets::global_menu::show(cursor.x, cursor.y);
                    self.request_redraw();
                }
                up || tap
            }
            (MouseButton::Middle, ElementState::Pressed) => self.ui.dispatch_tertiary_down(cursor),
            (MouseButton::Middle, ElementState::Released) => self.ui.dispatch_tertiary_up(cursor),
            // The desktop Back/Forward mouse buttons drive router history. `back`/`forward`
            // self-guard on `can_back`/`can_forward` and return whether they moved, so we
            // only report the press as consumed (→ redraw) when history actually advanced.
            (MouseButton::Back, ElementState::Pressed) => pebbles_core::router::back(),
            (MouseButton::Forward, ElementState::Pressed) => pebbles_core::router::forward(),
            _ => false,
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
}

impl Runner {
    /// Soak CSV (11 §2.9): count a rendered frame; every `EVERY_FRAMES` append a row.
    pub(super) fn soak_frame(&mut self) {
        if self.soak.is_none() {
            return;
        }
        self.soak_frames += 1;
        if self.soak_frames.is_multiple_of(soak_csv::EVERY_FRAMES) {
            self.soak_sample();
        }
    }

    /// Append one soak row now.
    fn soak_sample(&mut self) {
        let state = self.render_state();
        let device = match (state, self.active.as_ref()) {
            (Some(state), Some(a)) => state.device_of(a.window.id()),
            _ => 0,
        };
        let gpu = state.and_then(|s| s.ctx.allocator_report(device));
        let census =
            state.and_then(|s| s.renderers.get(device)).and_then(Option::as_ref).and_then(gpu::census_values);
        #[cfg(debug_assertions)]
        let ui = Some([
            self.ui.element_count(),
            self.ui.render_node_count(),
            pebbles_core::reactive::census_signals(),
            pebbles_core::reactive::census_subscriptions(),
        ]);
        #[cfg(not(debug_assertions))]
        let ui = None;
        let sample = soak_csv::Sample {
            unix_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis()),
            frame: self.soak_frames,
            uptime_s: self.started_at.elapsed().as_secs_f64(),
            rss_kib: soak_csv::rss_kib(),
            gpu,
            ui,
            census,
        };
        if let Some(s) = &mut self.soak {
            s.write(&sample);
        }
    }
}

impl ApplicationHandler<PebblesUserEvent> for Runner {
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        if self.soak.is_some() {
            self.soak_sample();
        }
        // 08 §17.7: stop the render thread (it drops every surface) before any window goes (T2).
        self.sink = None;
        self.retired_windows.clear();
    }

    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.active.is_some() {
            return;
        }
        // (web) A surface request is already in flight from a prior `resumed` —
        // don't kick off a second one.
        #[cfg(target_family = "wasm")]
        if self.gpu_pending {
            return;
        }

        let window = self.create_window(event_loop);
        window.set_ime_allowed(true); // enable IME composition (CJK, dead keys, etc.)
        self.a11y = Some(crate::a11y::Bridge::new(event_loop, &window));
        #[cfg(not(target_family = "wasm"))]
        window.set_visible(true);

        let physical = window.inner_size();
        let (w, h) = (physical.width.max(1), physical.height.max(1));

        // Desktop: the adapter/device request resolves instantly, so block on it and
        // install the surface right here — behavior identical to before.
        #[cfg(not(target_family = "wasm"))]
        {
            if self.host_init.is_none() {
                #[allow(unused_mut)]
                let mut init = HostInit::new(event_loop);
                // 08 §6.8 (P11): a device lost while idle wakes the loop (the waker only sends
                // an event; the runner recovers on its own thread).
                if let Some(p) = self.proxy.clone() {
                    init.gpu = init.gpu.clone().with_device_lost_waker(Arc::new(move || {
                        let _ = p.send_event(PebblesUserEvent::GpuLost);
                    }));
                }
                self.host_init = Some(init);
            }
            let init = self.host_init.get_or_insert_with(|| HostInit::new(event_loop));
            let mut context = pollster::block_on(new_context(init));
            let surface = match pollster::block_on(context.create_surface(window.clone(), w, h)) {
                Ok(s) => s,
                Err(e) => {
                    // The #1 first-run failure on Linux/Windows: no compatible GPU/driver.
                    // Exit with a readable message pointing at `pebbles doctor`, not a
                    // one-word `expect` panic + backtrace.
                    eprintln!("{}", gpu::startup_failure_text(&e));
                    std::process::exit(1);
                }
            };
            self.sink = Some(FrameSink::Inline(RenderState::new(context)));
            self.on_gpu_ready(window, surface);
            self.thread_sink_if_configured();
        }

        // Web: the GPU device request is async and MUST NOT block the browser main
        // thread. Move the context into a `spawn_local` task; when the surface is
        // ready, hand context + surface back via the proxy — `user_event` installs
        // them on the next UI turn. See documentations/web-support.md §4.1.
        #[cfg(target_family = "wasm")]
        {
            self.gpu_pending = true;
            let init = self.host_init.get_or_insert_with(|| HostInit::new(event_loop)).clone();
            let proxy = self.proxy.clone().expect("event loop proxy set in App::run");
            wasm_bindgen_futures::spawn_local(async move {
                let mut context = new_context(&init).await;
                // 08 P05: neither WebGPU nor WebGL2 could start. No panic: log it and say so on
                // the page. No event is sent, so `gpu_pending` stays set and nothing retries.
                match context.create_surface(window.clone(), w, h).await {
                    Ok(surface) => {
                        let _ = proxy.send_event(PebblesUserEvent::SurfaceReady { context, surface, window });
                    }
                    Err(e) => {
                        pebbles_core::log::error(
                            pebbles_core::log::Cat::Gpu,
                            format!("GPU init failed: {e}"),
                        );
                        crate::web_gpu::show_gpu_error(&format!(
                            "This app needs WebGPU or WebGL2, and neither could start.\n{}",
                            gpu::error_text(&e)
                        ));
                    }
                }
            });
        }
    }

    /// Deliver an off-stack event. Today the only one is web GPU bring-up finishing
    /// (see [`PebblesUserEvent`]); desktop never sends one.
    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: PebblesUserEvent) {
        match event {
            PebblesUserEvent::SurfaceReady { context, surface, window } => {
                self.sink = Some(FrameSink::Inline(RenderState::new(context)));
                self.on_gpu_ready(window, surface);
                // After an async recovery (not the first bring-up): bump the generation, log.
                #[cfg(target_family = "wasm")]
                if let Some(started) = self.recovery_started.take() {
                    self.gpu_generation += 1;
                    let line = format!(
                        "GPU stack rebuilt (generation={}, {} ms)",
                        self.gpu_generation,
                        started.elapsed().as_millis()
                    );
                    gpu_log(pebbles_foundation::log::Level::Warn, &line);
                }
            }
            PebblesUserEvent::GpuRecoveryFailed { window, error } => {
                #[cfg(target_family = "wasm")]
                {
                    self.gpu_pending = false;
                    self.recovery_started = None;
                    self.recovery_window = Some(window);
                    self.gpu_reset_requested = true;
                    let line = format!(
                        "GPU stack rebuild failed (attempt {}, next retry by the 08 §6.7 backoff): {error}",
                        self.gpu_reset_failures + 1
                    );
                    gpu_log(pebbles_foundation::log::Level::Error, &line);
                }
                #[cfg(not(target_family = "wasm"))]
                let _ = (window, error);
            }
            PebblesUserEvent::GpuContextLost =>
            {
                #[cfg(target_family = "wasm")]
                if !self.gpu_suspended {
                    self.gpu_suspended = true;
                    self.sink = None;
                    if let Some(window) = self.active.take().map(|a| a.window) {
                        self.recovery_window = Some(window);
                    }
                    gpu_log(pebbles_foundation::log::Level::Warn, "WebGL context lost; waiting for restore");
                }
            }
            PebblesUserEvent::GpuContextRestored => {
                #[cfg(target_family = "wasm")]
                if std::mem::replace(&mut self.gpu_suspended, false)
                    && let Some(init) = self.host_init.clone()
                {
                    self.start_web_recovery(init, Instant::now());
                }
            }
            // (web) The hidden-input IME bridge queued edit intents; drain them into
            // the focused editor. Never sent off wasm.
            PebblesUserEvent::WebImeInput => {
                #[cfg(target_family = "wasm")]
                {
                    self.ui.make_current();
                    let mut handled = false;
                    for key in crate::web_ime::drain() {
                        handled |= self.ui.dispatch_key(key);
                    }
                    if handled {
                        self.request_redraw();
                    }
                }
            }
            #[cfg(not(target_family = "wasm"))]
            PebblesUserEvent::FrameDone { key, frame_id: _, outcome } => self.frame_done(key, outcome),
            #[cfg(not(target_family = "wasm"))]
            PebblesUserEvent::GpuDeviceLost { lost } => {
                self.lost_lines = lost;
                self.gpu_reset_requested = true;
                self.request_redraw();
                for w in self.windows.values() {
                    w.window.request_redraw();
                }
            }
            PebblesUserEvent::GpuLost => {
                self.request_redraw();
                for w in self.windows.values() {
                    w.window.request_redraw();
                }
            }
            PebblesUserEvent::RouteChanged => {
                // The router's location signal was already set by the popstate
                // handler; make this window current and repaint so the dirty
                // route-reading components rebuild at the new location.
                self.ui.make_current();
                self.request_redraw();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
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
                    // The next packet carries the new size; `render_packet` configures (08 §17.3 b)
                    // and, on the render thread, the UI waits for it (08 §17.4).
                    active.occluded = false;
                    active.resize_sync.pending = true;
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
                let scrolled = wheel_with_overlay(&mut self.ui, cursor, dy);
                if pebbles_core::log::dev_mode() {
                    pebbles_core::log::trace(
                        pebbles_core::log::Cat::Input,
                        format!("wheel dy={dy:.0} at {:.0},{:.0} → scrolled={scrolled}", cursor.x, cursor.y),
                    );
                }
                if scrolled {
                    self.request_redraw();
                }
            }

            WindowEvent::MouseInput { state, button, .. } => {
                let cursor = self.cursor;
                if pebbles_core::log::dev_mode() {
                    let kind = match state {
                        ElementState::Pressed => "down",
                        ElementState::Released => "up",
                    };
                    pebbles_core::log::debug(
                        pebbles_core::log::Cat::Input,
                        format!(
                            "pointer {kind} {button:?} at {:.0},{:.0} → {}",
                            cursor.x,
                            cursor.y,
                            self.hit_desc(cursor)
                        ),
                    );
                }
                // F2: in inspect mode a click prints the render chain instead of hitting the UI.
                if self.inspect_mode && button == MouseButton::Left && state == ElementState::Pressed {
                    let chain = pebbles_render::inspect_at(self.ui.render_tree(), cursor);
                    eprint!("{}", pebbles_render::format_chain(&chain));
                    return;
                }
                let handled = self.dispatch_pointer(cursor, button, state);
                if handled {
                    self.request_redraw();
                }
            }

            // Touch (Android/iOS): map the primary finger onto the SAME pointer
            // model the mouse uses, so every widget — buttons, scroll, drag,
            // text selection — works under touch with no per-widget change. A
            // touch-down is a left press at the touch point; moves drive pan /
            // content-drag / long-press; up/cancel is a left release. Multi-touch
            // (id-based pinch/rotate) is a follow-up — see
            // documentations/android-support.md §4.4.
            WindowEvent::Touch(touch) => {
                use winit::event::TouchPhase;
                let scale = self.active.as_ref().map_or(1.0, |a| a.window.scale_factor());
                let cursor = Offset::new(touch.location.x / scale, touch.location.y / scale);
                self.cursor = cursor;
                match touch.phase {
                    TouchPhase::Started => {
                        if self.dispatch_pointer(cursor, MouseButton::Left, ElementState::Pressed) {
                            self.request_redraw();
                        }
                    }
                    TouchPhase::Moved => {
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
                        if self.lp_active
                            && let Some(t) = self.lp_target
                            && self.ui.dispatch_long_press_move(t, cursor)
                        {
                            self.request_redraw();
                        }
                    }
                    TouchPhase::Ended | TouchPhase::Cancelled => {
                        if self.dispatch_pointer(cursor, MouseButton::Left, ElementState::Released) {
                            self.request_redraw();
                        }
                    }
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
                // (web) When the IME bridge is on and a text editor is focused, the
                // hidden <input> owns keystrokes — skip winit's handling to avoid
                // double entry. Non-editor keys (nothing focused) still flow here.
                #[cfg(target_family = "wasm")]
                if self.web_ime && pebbles_core::focus::focused_is_editor() {
                    return;
                }
                if event.state == ElementState::Pressed {
                    let mods = pebbles_core::Mods {
                        shift: self.shift_down,
                        ctrl: self.ctrl_down,
                        alt: self.alt_down,
                        meta: self.meta_down,
                    };
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
                    // Mod+Shift+D: dump the FULL render tree (every rendered object
                    // with its size + window-space position) to the log — "capture
                    // everything that is rendered". Also dumps the UI event log.
                    if (self.ctrl_down || self.meta_down)
                        && self.shift_down
                        && to_shortcut_key(&event) == Some(pebbles_core::ShortcutKey::Char('d'))
                    {
                        let dump = self.ui.render_tree().debug_dump();
                        let count = self.ui.render_tree().node_count();
                        pebbles_core::log::info(
                            pebbles_core::log::Cat::Layout,
                            format!("render tree ({count} objects):\n{dump}"),
                        );
                        pebbles_core::log::dump("Mod+Shift+D render-tree capture");
                        return;
                    }
                    // Shift+Alt+F: format the focused code editor via its format provider.
                    if to_shortcut_key(&event).and_then(|sk| editor_chord_intent(mods, sk, false))
                        == Some(KeyInput::Format)
                        && self.ui.dispatch_key(KeyInput::Format)
                    {
                        self.request_redraw();
                        return;
                    }
                    // Escape, topmost-first. Transient overlays (menus, popovers,
                    // dropdowns, context menus, the date/time pickers) float ABOVE
                    // modals, so close the topmost one before touching any modal —
                    // otherwise Escape would close the sheet/dialog underneath and
                    // orphan the menu above it.
                    if event.logical_key == Key::Named(NamedKey::Escape) {
                        if pebbles_widgets::overlay::is_open() {
                            pebbles_widgets::overlay::hide_overlay();
                            self.request_redraw();
                            return;
                        }
                        // Otherwise close only the TOPMOST modal (dialog vs sheet by
                        // open order). While any modal is open, consume Escape so it
                        // can't reach the page behind — even if the top modal is
                        // non-dismissible (then this is a no-op but still consumes).
                        if pebbles_widgets::dialog::is_open() || pebbles_widgets::sheet::is_open() {
                            pebbles_widgets::overlay::dismiss_topmost_modal();
                            self.request_redraw();
                            return;
                        }
                    }
                    // B2 precedence: focused editor → shortcuts → Tab → activation.
                    // The editing-command modifier is Ctrl everywhere, plus ⌘ (Super) on
                    // macOS — otherwise ⌘C/⌘V/⌘X/⌘Z fall through to `Insert` and type a
                    // literal "c"/"v"/… The shortcut layer above already accepts ctrl||meta;
                    // this brings the editor call site in line.
                    let cmd = self.ctrl_down || (cfg!(target_os = "macos") && self.meta_down);
                    let intent = to_command(&event, cmd, self.shift_down);
                    // Track WHICH stage claimed the key, for the dev trace.
                    let mut stage = "unhandled";
                    let mut handled = if intent.is_some_and(|ki| self.ui.dispatch_key(ki)) {
                        stage = "editor";
                        true
                    } else if to_shortcut_key(&event)
                        .is_some_and(|sk| pebbles_core::shortcuts::dispatch(self.ui.window_id(), mods, sk))
                    {
                        stage = "shortcut";
                        true
                    } else if event.logical_key == Key::Named(NamedKey::Tab) {
                        // A focused code editor consumes Tab/Shift+Tab for indent/outdent;
                        // anywhere else (incl. single-line text fields) Tab traverses focus.
                        match editor_chord_intent(
                            mods,
                            pebbles_core::ShortcutKey::Tab,
                            pebbles_core::focus::focused_wants_tab(),
                        ) {
                            Some(ki) => {
                                stage = "editor-indent";
                                self.ui.dispatch_key(ki)
                            }
                            None => {
                                stage = "focus-move";
                                self.ui.focus_move(!self.shift_down)
                            }
                        }
                    } else if matches!(
                        event.logical_key.as_ref(),
                        Key::Named(NamedKey::Enter | NamedKey::Space) | Key::Character(" ")
                    ) && self.ui.dispatch_activate()
                    {
                        stage = "activate";
                        true
                    } else {
                        false
                    };
                    // If nothing else claimed the key, use it to scroll the view
                    // under the pointer (PageUp/Down, Home/End, arrows, Space).
                    if !handled {
                        handled = self.scroll_key(&event);
                        if handled {
                            stage = "scroll";
                        }
                    }
                    if pebbles_core::log::dev_mode() {
                        pebbles_core::log::debug(
                            pebbles_core::log::Cat::Input,
                            format!(
                                "key {:?} mods(c={} s={} a={} m={}) → {stage}",
                                event.logical_key,
                                self.ctrl_down as u8,
                                self.shift_down as u8,
                                self.alt_down as u8,
                                self.meta_down as u8,
                            ),
                        );
                    }
                    if handled {
                        self.request_redraw();
                    }
                }
            }

            // IME composition (CJK, dead keys). Preedit = the in-progress underline;
            // Commit = the finished text, inserted like a normal keystroke. The candidate window
            // follows the focused editor's caret (`place_ime`, after each paint).
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

            // DPI changed (e.g. dragging across mixed-DPI monitors). Anchored popovers shift,
            // and the swapchain must track the new physical size — resize to the window's current
            // inner size and repaint. Layout picks up the new scale via `window.scale_factor()`.
            // (winit follows this with a `Resized` in most cases, but handling it here avoids a
            // blurry/mis-scaled frame in the ones where it doesn't.)
            WindowEvent::ScaleFactorChanged { .. } => {
                pebbles_widgets::overlay::hide_overlay();
                if let Some(active) = self.active.as_mut() {
                    active.occluded = false;
                    active.resize_sync.pending = true;
                    active.window.request_redraw();
                }
            }

            WindowEvent::RedrawRequested => {
                let started = Instant::now();
                self.render();
                if render::frame_stats_enabled() {
                    self.run_stats.ui_frame(started.elapsed().as_secs_f64() * 1e3);
                }
            }

            // 08 §5.4: stop rendering while hidden; redraw when visible again.
            WindowEvent::Occluded(occluded) => {
                if let Some(active) = self.active.as_mut() {
                    active.occluded = occluded;
                    if !occluded {
                        active.window.request_redraw();
                    }
                }
            }
            WindowEvent::Focused(true) => {
                if let Some(active) = self.active.as_mut()
                    && active.occluded
                {
                    active.occluded = false;
                    active.window.request_redraw();
                }
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // (web) Raise/lower the hidden IME input to match whether a text editor is
        // focused (soft keyboard + composition appear exactly when editing).
        #[cfg(target_family = "wasm")]
        if self.web_ime {
            crate::web_ime::sync_focus();
        }
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
        // A poisoned GPU (or a pending retry): wake when the retry is due, then redraw so
        // `recover_gpu_if_poisoned` runs — an idle `Wait` loop would sit on a dead device.
        if self.gpu_reset_requested || self.render_state().is_some_and(|s| s.ctx.needs_reset()) {
            match self.gpu_retry_at {
                Some(t) if t > Instant::now() => wake_by(event_loop, t),
                _ => {
                    // (web) A failed async recovery left no active window to redraw.
                    #[cfg(target_family = "wasm")]
                    if self.active.is_none() && !self.gpu_pending {
                        self.recover_gpu_if_poisoned();
                    }
                    self.request_redraw();
                    for w in self.windows.values() {
                        w.window.request_redraw();
                    }
                }
            }
        }
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
                wake_by(event_loop, deadline);
            }
        }
        // Wake exactly when the next pending timeout (tooltip / hover-card delays,
        // create_timeout callbacks) is due — a fully still mouse must still show
        // the tooltip. No pending timers means plain `Wait` above.
        let now_secs = self.clock.elapsed().as_secs_f64();
        if let Some(at) = pebbles_core::animation::next_deadline(now_secs) {
            let remaining = (at - now_secs).max(0.0);
            wake_by(event_loop, Instant::now() + Duration::from_secs_f64(remaining));
        }
        // Dev monkey-tester: hammer the UI with synthetic input and keep the loop hot.
        if self.storm.is_some() && self.mounted {
            self.storm_tick();
            wake_by(event_loop, Instant::now() + Duration::from_millis(10));
        }
        // `[pebbles run]` summary (PEBBLES_FRAME_STATS): once a second, idle or not.
        self.run_stats_tick();
        if render::frame_stats_enabled() {
            wake_by(event_loop, Instant::now() + Duration::from_secs(1));
        }
        // Dev window churn: one tick per 16 ms until its cycles are done.
        if self.churn.as_ref().is_some_and(storm::WindowChurn::running) && self.mounted {
            self.churn_tick();
            wake_by(event_loop, Instant::now() + storm::WindowChurn::TICK);
        }
    }
}

/// Wake no later than `t`, keeping an earlier deadline already set this turn (a GPU retry, a
/// long-press, an animation): the earliest wins, so no wake-up is pushed back (08 §6.7).
fn wake_by(event_loop: &ActiveEventLoop, t: Instant) {
    match event_loop.control_flow() {
        ControlFlow::Poll => {}
        ControlFlow::WaitUntil(existing) if existing <= t => {}
        _ => event_loop.set_control_flow(ControlFlow::WaitUntil(t)),
    }
}

/// Log one GPU recovery line (11 §4.1). On wasm `pebbles_foundation::log` does not echo, so
/// the line also goes to the browser console (C20 step 3b).
pub(super) fn gpu_log(level: pebbles_foundation::log::Level, line: &str) {
    use pebbles_foundation::log::{Cat, Level, logf};
    logf(level, Cat::Gpu, format_args!("{line}"));
    #[cfg(target_family = "wasm")]
    {
        let value = wasm_bindgen::JsValue::from_str(line);
        match level {
            Level::Error => web_sys::console::error_1(&value),
            _ => web_sys::console::warn_1(&value),
        }
    }
    #[cfg(not(target_family = "wasm"))]
    let _ = Level::Error;
}

/// Put the OS IME candidate window under `caret` (window coordinates, logical px): winit's
/// cursor area on desktop, the hidden input on the web (03b §3.10, E17).
fn place_ime(window: &Window, caret: kurbo::Rect, scale: f64) {
    let pos = PhysicalPosition::new(caret.x0 * scale, caret.y0 * scale);
    let size =
        winit::dpi::PhysicalSize::new((caret.width() * scale).max(1.0), (caret.height() * scale).max(1.0));
    window.set_ime_cursor_area(pos, size);
    #[cfg(target_family = "wasm")]
    crate::web_ime::place(caret);
}
