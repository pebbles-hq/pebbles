//! # pebbles-testing
//!
//! The headless test harness for Pebbles widgets and apps. It owns the three
//! things every test needs — a [`Ui`], a [`TextEnv`], and a window size — and
//! exposes the frame lifecycle as methods, so a test says what it *means*
//! instead of re-deriving the pipeline:
//!
//! ```ignore
//! use pebbles_testing::Harness;
//!
//! fn counter() -> impl IntoWidget { /* … */ }
//!
//! #[test]
//! fn it_counts() {
//!     let mut h = Harness::new();
//!     h.mount(counter);
//!     h.draw();                          // rebuild → layout → paint (settled)
//!     h.tap(Offset::new(20.0, 20.0));
//!     h.draw();
//!     assert!(h.element_count() > 0);
//! }
//! ```
//!
//! **Why a crate.** Every test used to hand-roll `Ui::new()` + `TextEnv::new()` +
//! the service `init()` calls + a private `fn frame(..)`, so a change to the
//! frame pipeline meant editing every test file. The lifecycle lives here once:
//! [`Harness::draw`] also runs the **corrective-relayout settle loop** (a lazy
//! paint-time measurement can invalidate the geometry layout just computed), so
//! tests can't silently assert against unsettled geometry.
//!
//! The harness is deliberately transparent: [`ui`](Harness::ui) and
//! [`env`](Harness::env) are public, so anything not wrapped here is still
//! reachable.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Color, Offset, Size, palette};
use pebbles_render::{Scene, TextEnv};

/// The default headless window size. Big enough for a real layout, small enough
/// that viewport-culling behavior is exercised.
pub const DEFAULT_WINDOW: Size = Size { width: 800.0, height: 600.0 };

/// A frame's nominal duration (~60 Hz) — the step [`Harness::tick`] advances by.
pub const FRAME: f64 = 1.0 / 60.0;

/// Initialize every global service a mounted tree expects, idempotently. Called
/// by [`Harness::new`]; exposed for tests that build a `Ui` by hand.
pub fn init_services() {
    pebbles_widgets::theme::init();
    pebbles_widgets::overlay::init();
    pebbles_widgets::dialog::init();
    pebbles_widgets::sheet::init();
    pebbles_widgets::text_direction::init();
    pebbles_core::focus::init();
    pebbles_core::animation::reset();
    pebbles_core::keyboard::set_modifiers(false, false, false, false);
}

// ---------------------------------------------------------------------------
// Free-function lifecycle — for tests that own their own `Ui`/`TextEnv`
// (multi-window cases, or anything the `Harness` shape doesn't fit). These are
// the single definition of "a frame": the pipeline lives here, not copied into
// each test file.
// ---------------------------------------------------------------------------

/// Reconcile + lay out, without painting. The cheap frame.
pub fn frame(ui: &mut Ui, env: &mut TextEnv, window: Size) {
    ui.make_current();
    ui.rebuild_if_dirty();
    ui.layout(env, window);
}

/// Reconcile + lay out + paint, looping the corrective-relayout settle (see
/// [`Harness::draw`]) and discarding the scene.
pub fn draw_frame(ui: &mut Ui, env: &mut TextEnv, window: Size) {
    for _ in 0..8 {
        ui.make_current();
        ui.rebuild_if_dirty();
        ui.layout(env, window);
        let mut scene = Scene::new();
        let corrective = ui.paint(env, &mut scene);
        env.finish_frame();
        if !corrective {
            return;
        }
    }
    panic!("layout/paint did not settle in 8 passes — geometry is oscillating");
}

/// A mounted, headless Pebbles tree plus its frame lifecycle.
pub struct Harness {
    /// The element/render tree. Public: use it directly for anything the
    /// harness doesn't wrap.
    pub ui: Ui,
    /// Font + shaped-layout environment (the window's text cache).
    pub env: TextEnv,
    /// The window the tree lays out into.
    pub window: Size,
    /// Background the mounted root paints over.
    background: Color,
    /// The animation clock, advanced by [`tick`](Self::tick) / [`settle`](Self::settle).
    now: f64,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    /// A harness with every service initialized and a [`DEFAULT_WINDOW`] viewport.
    pub fn new() -> Self {
        init_services();
        let ui = Ui::new();
        ui_make_current(&ui);
        Harness { ui, env: TextEnv::new(), window: DEFAULT_WINDOW, background: palette::WHITE, now: 0.0 }
    }

    /// Set the window size (builder form).
    pub fn window(mut self, width: f64, height: f64) -> Self {
        self.window = Size::new(width, height);
        self
    }

    /// Set the root background (builder form; default white).
    pub fn background(mut self, color: Color) -> Self {
        self.background = color;
        self
    }

    // ----- mounting --------------------------------------------------------

    /// Mount a no-props component as the root, wrapped in the `View` +
    /// `OverlayHost` the shell would give it (so overlays, dialogs, tooltips and
    /// popovers work exactly as in a real app).
    pub fn mount<W: IntoWidget + 'static>(&mut self, root: fn() -> W) {
        let wrapped = pebbles_widgets::OverlayHost::wrap(component(root).into_widget());
        self.mount_widget(pebbles_widgets::View::new(self.background, wrapped));
    }

    /// Mount an already-built widget as the root (no `View`/overlay wrapper is
    /// added — you own the tree).
    pub fn mount_widget(&mut self, root: impl IntoWidget) {
        self.ui.make_current();
        self.ui.mount_root(root.into_widget());
    }

    // ----- the frame lifecycle --------------------------------------------

    /// Reconcile + lay out (no paint) — the cheap frame, for tests that only
    /// assert geometry or state.
    pub fn frame(&mut self) {
        self.ui.make_current();
        self.ui.rebuild_if_dirty();
        self.ui.layout(&mut self.env, self.window);
    }

    /// A full frame: reconcile → layout → paint, looping the
    /// **corrective-relayout settle** until the geometry stops moving, and
    /// returning the encoded scene.
    ///
    /// Paint can invalidate the layout it just ran on (a lazily materialized
    /// text line measuring taller than its estimate). Asserting on the first
    /// pass would read unsettled geometry, so this pumps until paint stops
    /// asking — bounded, so a genuinely oscillating tree fails loudly rather
    /// than hanging.
    pub fn draw(&mut self) -> Scene {
        for _ in 0..8 {
            self.ui.make_current();
            self.ui.rebuild_if_dirty();
            self.ui.layout(&mut self.env, self.window);
            let mut scene = Scene::new();
            if !self.ui.paint(&mut self.env, &mut scene) {
                self.env.finish_frame();
                return scene;
            }
            self.env.finish_frame();
        }
        panic!("layout/paint did not settle in 8 passes — geometry is oscillating");
    }

    /// Run `n` full frames (each settled). Useful for corrective/measurement
    /// passes: a virtualized list feeds real extents back over a frame or two.
    pub fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.draw();
        }
    }

    /// Advance the animation clock by `dt` (and the scroll springs), then run a
    /// frame. Returns whether any scroll spring is still moving.
    pub fn tick(&mut self, dt: f64) -> bool {
        self.now += dt;
        pebbles_core::animation::tick(self.now);
        self.ui.make_current();
        let scrolling = self.ui.tick_scrolls(dt);
        self.frame();
        scrolling
    }

    /// Pump frames until every animation and scroll spring goes idle (bounded,
    /// so a legitimately looping animation — a spinner, a shimmer — can't hang
    /// the test).
    pub fn settle(&mut self) {
        for _ in 0..240 {
            let scrolling = self.tick(FRAME);
            if !scrolling && !pebbles_core::animation::active() {
                return;
            }
        }
    }

    /// The current animation-clock time.
    pub fn now(&self) -> f64 {
        self.now
    }

    // ----- input -----------------------------------------------------------

    /// A full click at `at`: pointer-down → tap → pointer-up, then a frame.
    pub fn click(&mut self, at: Offset) {
        self.ui.make_current();
        self.ui.dispatch_pointer_down(at);
        self.ui.dispatch_tap(at);
        self.ui.dispatch_pointer_up(at);
        self.frame();
    }

    /// Just the tap (no press/release) — for widgets that arm on tap alone.
    pub fn tap(&mut self, at: Offset) {
        self.ui.make_current();
        self.ui.dispatch_tap(at);
        self.frame();
    }

    /// A double-click at `at` (word selection, expand toggles).
    pub fn double_click(&mut self, at: Offset) {
        self.ui.make_current();
        self.ui.dispatch_double_tap(at);
        self.frame();
    }

    /// Move the pointer to `at` (hover states, tooltips).
    pub fn hover(&mut self, at: Offset) {
        self.ui.make_current();
        self.ui.dispatch_hover(at);
        self.frame();
    }

    /// Send a keyboard intent to the focused editor, then run a frame.
    pub fn key(&mut self, input: pebbles_core::KeyInput) {
        self.ui.make_current();
        self.ui.dispatch_key(input);
        self.frame();
    }

    /// Wheel-scroll `delta` at `at`, then run a frame. The spring keeps moving
    /// afterwards — follow with [`settle`](Self::settle) to land it.
    pub fn scroll(&mut self, at: Offset, delta: f64) {
        self.ui.make_current();
        self.ui.dispatch_scroll(at, delta);
        self.frame();
    }

    /// Drag from `from` to `to` (select, pan, reorder): pan start → update → end.
    pub fn drag(&mut self, from: Offset, to: Offset) {
        self.ui.make_current();
        if let Some(target) = self.ui.pan_target_at(from) {
            self.ui.dispatch_pan_start(target, from);
            self.ui.dispatch_pan_update(target, to);
            self.ui.dispatch_pan_end(target, to);
        }
        self.frame();
    }

    // ----- queries ---------------------------------------------------------

    /// Live element count (the reconciled widget tree).
    pub fn element_count(&self) -> usize {
        self.ui.element_count()
    }

    /// Live render-node count (the layout/paint tree) — the virtualization
    /// tripwire: it must track the viewport, not the content.
    pub fn render_node_count(&self) -> usize {
        self.ui.render_node_count()
    }

    /// The first render object of type `T`, if the tree has one.
    pub fn find<T: pebbles_render::RenderObject>(&self) -> Option<pebbles_render::RenderId> {
        self.ui.render_tree().find::<T>()
    }

    /// Every render object of type `T`, in insertion order.
    pub fn find_all<T: pebbles_render::RenderObject>(&self) -> Vec<pebbles_render::RenderId> {
        self.ui.render_tree().find_all::<T>()
    }

    /// Read a typed render object out of the tree (panics if `id` is not a `T`).
    pub fn object<T: pebbles_render::RenderObject>(&self, id: pebbles_render::RenderId) -> &T {
        self.ui
            .render_tree()
            .object_ref(id)
            .downcast_ref::<T>()
            .expect("render object is not of the requested type")
    }

    /// The laid-out size of a render node.
    pub fn size_of(&self, id: pebbles_render::RenderId) -> Size {
        self.ui.render_tree().size_of(id)
    }
}

/// `Ui::make_current` takes `&self`; keep the call in one place so `new` can run
/// it before the struct is assembled.
fn ui_make_current(ui: &Ui) {
    ui.make_current();
}
