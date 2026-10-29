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
    let _ = ui.publish_bounds();
}

/// Reconcile + lay out + paint, looping the corrective-relayout settle (see
/// [`Harness::draw`]) and discarding the scene.
///
/// # Panics
/// Panics if layout/paint does not settle within 8 passes — geometry is oscillating (a
/// bug in a widget that never reaches a stable size). Fails loudly rather than hanging.
pub fn draw_frame(ui: &mut Ui, env: &mut TextEnv, window: Size) {
    for _ in 0..8 {
        ui.make_current();
        ui.rebuild_if_dirty();
        ui.layout(env, window);
        // New bounds (use_bounds, layout_builder) rebuild their readers: run another pass.
        let moved = ui.publish_bounds();
        let mut scene = Scene::new();
        let corrective = ui.paint(env, &mut scene);
        env.finish_frame();
        if !corrective && !moved {
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

/// Tear the tree down like the shell does when a window closes: component cleanups run,
/// focus/scroll registrations are released. Without this a dropped harness left its
/// focused node as the global focus, so the next harness's `autofocus` (which only grabs
/// when nothing holds focus) never fired and no focus ring rendered.
impl Drop for Harness {
    fn drop(&mut self) {
        self.ui.dispose();
    }
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    /// A harness with every service initialized and a [`DEFAULT_WINDOW`] viewport.
    /// `TextEnv::new()` = bundled + App::font fonts only (deterministic on every OS).
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
        self.publish_window();
        self.ui.rebuild_if_dirty();
        self.ui.layout(&mut self.env, self.window);
        // Like the shell: publish `use_bounds` rects after layout (layout_builder, …).
        let _ = self.ui.publish_bounds();
    }

    /// Publish the window size to the reactive window/media-query signals, exactly as
    /// the shell does each render frame. Without this, `window_size()` /
    /// `media_query().size` / `breakpoint()` / popover on-screen clamping all read
    /// their `(0, 0)` default in tests — so a component that sizes itself from the
    /// window (a fixed-height panel, a responsive breakpoint) collapses under test.
    /// `set_window_size` only writes on an actual change, so calling it every frame is
    /// cheap and matches the shell's per-frame call.
    fn publish_window(&self) {
        pebbles_widgets::overlay::set_window_size(self.window.width, self.window.height);
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
    ///
    /// # Panics
    /// Panics if the geometry does not settle within 8 layout/paint passes (an
    /// oscillating tree) — by design, so the bug surfaces instead of hanging the test.
    pub fn draw(&mut self) -> Scene {
        self.publish_window();
        for _ in 0..8 {
            self.ui.make_current();
            self.ui.rebuild_if_dirty();
            self.ui.layout(&mut self.env, self.window);
            // New bounds (use_bounds, layout_builder) rebuild their readers: run another pass.
            let moved = self.ui.publish_bounds();
            let mut scene = Scene::new();
            if !self.ui.paint(&mut self.env, &mut scene) && !moved {
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
            // Pending timers (hover delays) aren't animations any more, but settling
            // still waits for them (bounded) so "hover then settle" shows the tooltip.
            if !scrolling
                && !pebbles_core::animation::active()
                && !pebbles_core::animation::has_pending_timeouts()
            {
                return;
            }
        }
    }

    /// Advance the clock by `secs` in [`FRAME`] steps (timers — hover delays, toast
    /// dismissals — fire when due), then [`settle`](Self::settle). Flutter's `pump(duration)`.
    pub fn advance(&mut self, secs: f64) {
        let mut left = secs;
        while left > 1e-9 {
            let dt = left.min(FRAME);
            self.tick(dt);
            left -= dt;
        }
        self.settle();
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

    /// A triple-click at `at` (line selection in a text/code editor).
    pub fn triple_click(&mut self, at: Offset) {
        self.ui.make_current();
        self.ui.dispatch_triple_tap(at);
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
    #[cfg(debug_assertions)] // pebbles-core exposes the count only in debug builds
    pub fn element_count(&self) -> usize {
        self.ui.element_count()
    }

    /// Live render-node count (the layout/paint tree) — the virtualization
    /// tripwire: it must track the viewport, not the content.
    #[cfg(debug_assertions)] // pebbles-core exposes the count only in debug builds
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

    /// Read a typed render object out of the tree.
    ///
    /// # Panics
    /// Panics if `id` is not a live node (a stale `RenderId`) or if its render object is
    /// not of type `T` (wrong-type downcast).
    pub fn object<T: pebbles_render::RenderObject>(&self, id: pebbles_render::RenderId) -> &T {
        self.ui
            .render_tree()
            .object_ref(id)
            .downcast_ref::<T>()
            .expect("render object is not of the requested type")
    }

    /// The laid-out size of a render node.
    ///
    /// # Panics
    /// Panics if `id` is not a live node (a stale `RenderId`).
    pub fn size_of(&self, id: pebbles_render::RenderId) -> Size {
        self.ui.render_tree().size_of(id)
    }

    // ----- resize / focus / accessibility ----------------------------------

    /// Re-layout the tree at a new window size — a real window resize / device
    /// rotation. Mirrors the [`window`](Self::window) builder followed by a settled
    /// [`draw`](Self::draw), so callers observe geometry at the new size.
    pub fn resize(&mut self, w: f64, h: f64) {
        self.window = Size::new(w, h);
        self.draw();
    }

    /// Move keyboard focus forward one stop — the harness analog of pressing Tab in
    /// the shell where nothing is a code editor (there, Tab traverses focus; a focused
    /// code editor would instead indent, which this deliberately does not model). With
    /// nothing focused yet this lands on the first focusable, matching the shell's
    /// `Ui::focus_move(true)` call for Tab.
    pub fn press_tab(&mut self) {
        self.ui.make_current();
        self.ui.focus_move(true);
        self.frame();
    }

    /// The accessibility (semantics) tree a screen reader sees: every
    /// [`SemanticsNode`](pebbles_render::SemanticsNode) with its role, label, and
    /// window-space bounds. The read side of what [`find_text`](Self::find_text) queries.
    pub fn semantics_tree(&self) -> Vec<pebbles_render::SemanticsNode> {
        self.ui.render_tree().semantics_tree()
    }

    /// Whether `text` is on screen — Flutter's `find.text`: true if any rendered
    /// `Text` / rich text paragraph's full content is exactly `text`, or any semantics
    /// node's accessible label is (a button's text, a labeled field).
    pub fn find_text(&self, text: &str) -> bool {
        self.find_paragraphs(text).next().is_some()
            || self.semantics_tree().iter().any(|n| n.props.label == text)
    }

    /// Every rendered text paragraph (`Text`, rich text) whose full content is exactly
    /// `text` — the render ids, e.g. to read their [`size_of`](Self::size_of).
    pub fn find_all_text(&self, text: &str) -> Vec<pebbles_render::RenderId> {
        self.find_paragraphs(text).collect()
    }

    fn find_paragraphs<'a>(&'a self, text: &'a str) -> impl Iterator<Item = pebbles_render::RenderId> + 'a {
        let tree = self.ui.render_tree();
        tree.find_all::<pebbles_render::RenderParagraph>().into_iter().filter(move |&id| {
            tree.object_ref(id)
                .downcast_ref::<pebbles_render::RenderParagraph>()
                .is_some_and(|p| p.text == text)
        })
    }

    /// Feed a run of text the way an IME commit does — compose it as preedit
    /// (underlined), clear the composition, then commit it as an insert: exactly the
    /// [`Preedit`](pebbles_core::KeyInput::Preedit) → `Preedit("")` →
    /// [`Insert`](pebbles_core::KeyInput::Insert) sequence the shell forwards from
    /// `Ime::Preedit` / `Ime::Commit`. Use this for CJK / dead-key input so the
    /// preedit path is exercised, not just a raw `key(Insert(..))`.
    pub fn ime(&mut self, text: &str) {
        self.key(pebbles_core::KeyInput::Preedit(text.to_string()));
        self.key(pebbles_core::KeyInput::Preedit(String::new()));
        self.key(pebbles_core::KeyInput::Insert(text.to_string()));
    }
}

/// `Ui::make_current` takes `&self`; keep the call in one place so `new` can run
/// it before the struct is assembled.
fn ui_make_current(ui: &Ui) {
    ui.make_current();
}

#[cfg(test)]
mod tests {
    //! The harness asserting things about ITSELF (§3 blocker): a mounted tree draws
    //! and settles, `click` / `key` / `scroll` actually reach the mounted widgets, and
    //! an oscillating tree makes `draw()` panic loudly instead of hanging. Without
    //! this, nothing proved the harness plumbs input or catches non-settling layout —
    //! the reason the suite under-adopted it.

    use std::cell::{Cell, RefCell};

    use pebbles_core::{IntoWidget, KeyInput, RenderWidget, Signal, component, create_signal};
    use pebbles_foundation::{Offset, Size, palette};
    use pebbles_render::{BoxConstraints, LayoutCx, PaintCx, RenderObject};
    use pebbles_widgets::{
        ScrollController, button, column, container, gesture_detector, span, text, text_field, text_rich,
        use_scroll_controller,
    };

    use super::Harness;

    // ----- draw settles ----------------------------------------------------

    fn plain() -> impl IntoWidget {
        container().width(100.0).height(40.0).child(text("hi"))
    }

    #[test]
    fn draw_settles_and_builds_a_tree() {
        let mut h = Harness::new();
        h.mount(plain);
        h.draw(); // must return (settle), not panic
        assert!(h.element_count() > 0, "a mounted tree has live elements");
    }

    // ----- layout_builder sees its size headless --------------------------

    thread_local! {
        static SEEN: Cell<f64> = const { Cell::new(0.0) };
    }

    fn sized_probe() -> impl IntoWidget {
        pebbles_widgets::layout_builder(|size| {
            SEEN.with(|s| s.set(size.width));
            container().height(20.0)
        })
    }

    /// Regression: the harness never published `use_bounds` rects after layout (only the
    /// shell did), so `layout_builder` built against `Size::ZERO` in every headless render.
    #[test]
    fn layout_builder_sees_its_laid_out_size() {
        SEEN.with(|s| s.set(0.0));
        let mut h = Harness::new();
        h.mount(sized_probe);
        h.draw();
        assert_eq!(SEEN.with(Cell::get), super::DEFAULT_WINDOW.width, "builder got the window width");
    }

    // ----- a hover-delayed tooltip opens headless -------------------------

    fn tooltip_probe() -> impl IntoWidget {
        pebbles_widgets::tooltip("Hint", button("Hover"))
    }

    /// Regression: hover enter/exit went only to the innermost hover listener, so a tooltip
    /// around a (hover-aware) button never armed its delay. Flutter's MouseTracker notifies
    /// every region under the pointer.
    #[test]
    fn hover_then_settle_opens_a_tooltip() {
        let mut h = Harness::new();
        h.mount(tooltip_probe);
        h.draw();
        h.hover(Offset::new(20.0, 15.0));
        h.settle();
        assert!(pebbles_widgets::overlay::passive_is_open(), "tooltip chip shown after its hover delay");
    }

    /// Flutter's `Tooltip` exposes its message as the trigger's semantics tooltip, so a
    /// screen reader announces it without hovering: the button node carries it as its
    /// description (label unchanged).
    #[test]
    fn tooltip_message_is_the_trigger_description() {
        let mut h = Harness::new();
        h.mount(tooltip_probe);
        h.draw();
        let tree = h.semantics_tree();
        let trigger = tree.iter().find(|n| n.props.label == "Hover").expect("the trigger button node");
        assert_eq!(trigger.props.description.as_deref(), Some("Hint"));
        assert!(tree.iter().all(|n| n.props.label != "Hint"), "no extra node for the message");
    }

    fn bare_tooltip_probe() -> impl IntoWidget {
        pebbles_widgets::tooltip("Info", pebbles_widgets::gap_w(24.0))
    }

    /// A trigger with no semantics of its own still exposes the message.
    #[test]
    fn tooltip_on_a_non_semantic_child_gets_its_own_node() {
        let mut h = Harness::new();
        h.mount(bare_tooltip_probe);
        h.draw();
        let tree = h.semantics_tree();
        assert!(tree.iter().any(|n| n.props.description.as_deref() == Some("Info")), "{tree:?}");
    }

    // ----- autofocus is deterministic across harnesses ---------------------

    fn autofocus_probe() -> impl IntoWidget {
        button("First").autofocus()
    }

    #[test]
    fn autofocus_fires_in_a_fresh_harness_after_another_one() {
        for _ in 0..2 {
            let mut h = Harness::new();
            h.mount(autofocus_probe);
            h.settle();
            h.draw();
            let win = h.ui.window_id();
            assert!(
                pebbles_core::focus::focused_element_ffi(win).is_some(),
                "the autofocus button holds focus in every harness"
            );
        }
    }

    // ----- click reaches a widget -----------------------------------------

    thread_local! {
        static TAPS: Cell<u32> = const { Cell::new(0) };
    }

    fn tap_probe() -> impl IntoWidget {
        gesture_detector(container().width(120.0).height(120.0).color(palette::BLUE))
            .on_tap(|| TAPS.with(|t| t.set(t.get() + 1)))
    }

    #[test]
    fn click_reaches_the_mounted_widget() {
        TAPS.with(|t| t.set(0));
        let mut h = Harness::new();
        h.mount(tap_probe);
        h.draw();
        h.click(Offset::new(20.0, 20.0));
        assert_eq!(TAPS.with(Cell::get), 1, "a click dispatched through the harness reaches on_tap");
    }

    // ----- key reaches the focused editor ---------------------------------

    thread_local! {
        static KEY_VAL: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
    }

    fn key_bound() -> Signal<String> {
        KEY_VAL.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = Some(create_signal(String::new()));
            }
            c.unwrap()
        })
    }

    fn key_probe() -> impl IntoWidget {
        text_field().bind(key_bound()).autofocus().width(200.0)
    }

    #[test]
    fn key_reaches_the_focused_editor() {
        KEY_VAL.with(|c| *c.borrow_mut() = None);
        let mut h = Harness::new();
        h.mount(|| component(key_probe));
        h.draw(); // autofocus lands
        h.key(KeyInput::Insert("x".into()));
        assert_eq!(key_bound().get(), "x", "a key dispatched through the harness reaches the editor");
    }

    // ----- scroll reaches a scrollable ------------------------------------

    thread_local! {
        static SCROLL: RefCell<Option<ScrollController>> = const { RefCell::new(None) };
    }

    fn scroll_probe() -> impl IntoWidget {
        let ctrl = use_scroll_controller();
        SCROLL.with(|c| {
            if c.borrow().is_none() {
                *c.borrow_mut() = Some(ctrl);
            }
        });
        container().height(200.0).child(
            pebbles_widgets::ListView::builder(200, 40.0, |i| text(format!("row {i}"))).controller(ctrl),
        )
    }

    #[test]
    fn scroll_reaches_the_scrollable() {
        SCROLL.with(|c| *c.borrow_mut() = None);
        let mut h = Harness::new();
        h.mount(|| component(scroll_probe));
        h.draw();
        h.scroll(Offset::new(100.0, 100.0), 500.0);
        h.settle();
        let offset = SCROLL.with(|c| c.borrow().as_ref().map(ScrollController::offset).unwrap_or(0.0));
        assert!(offset > 0.0, "a scroll dispatched through the harness moves the scrollable, got {offset}");
    }

    // ----- oscillation panics loudly --------------------------------------

    /// A leaf render object that requests a corrective relayout on EVERY paint — it
    /// never settles, so the harness's bounded settle loop must give up and panic.
    struct AlwaysCorrective;

    impl RenderObject for AlwaysCorrective {
        fn layout(&mut self, _cx: &mut LayoutCx<'_>, constraints: BoxConstraints) -> Size {
            constraints.constrain(Size::new(20.0, 20.0))
        }
        fn paint(&self, cx: &mut PaintCx<'_>, _offset: Offset) {
            cx.request_relayout();
        }
    }

    #[derive(Clone)]
    struct Oscillator;

    impl RenderWidget for Oscillator {
        fn create_render_object(&self) -> Box<dyn RenderObject> {
            Box::new(AlwaysCorrective)
        }
        fn update_render_object(&self, _object: &mut dyn RenderObject) {}
    }

    pebbles_core::render_widget!(Oscillator);

    #[test]
    #[should_panic(expected = "oscillating")]
    fn oscillating_layout_panics_instead_of_hanging() {
        let mut h = Harness::new();
        h.mount_widget(Oscillator);
        h.draw(); // never settles → bounded loop panics
    }

    // ----- resize re-lays-out at the new size ------------------------------

    #[test]
    fn resize_relays_out_at_the_new_window_size() {
        let mut h = Harness::new();
        h.mount(plain);
        h.draw();
        h.resize(320.0, 240.0);
        assert_eq!(h.window, Size::new(320.0, 240.0), "resize updates the window size");
        assert!(h.element_count() > 0, "the tree survives a resize + re-layout");
    }

    // ----- semantics / find_text -------------------------------------------

    fn labeled_button() -> impl IntoWidget {
        button("Save")
    }

    #[test]
    fn semantics_tree_exposes_labels_and_find_text_matches() {
        let mut h = Harness::new();
        h.mount(|| component(labeled_button));
        h.draw();
        assert!(!h.semantics_tree().is_empty(), "a mounted button produces a semantics node");
        assert!(h.find_text("Save"), "find_text matches the button's accessible label");
        assert!(!h.find_text("Cancel"), "find_text does not match an absent label");
    }

    fn plain_and_rich_text() -> impl IntoWidget {
        column(vec![
            text("Hello world").into_widget(),
            text_rich(vec![span("Rich "), span("text")]).into_widget(),
        ])
    }

    #[test]
    fn find_text_matches_plain_rendered_text_like_flutter() {
        // Regression: find_text only matched accessibility labels, so a plain `text()`
        // was invisible to it.
        let mut h = Harness::new();
        h.mount(|| component(plain_and_rich_text));
        h.draw();
        assert!(h.find_text("Hello world"), "plain Text content");
        assert!(h.find_text("Rich text"), "RichText full content");
        assert!(!h.find_text("Hello"), "exact match, not substring");
        assert_eq!(h.find_all_text("Hello world").len(), 1);
    }

    // ----- press_tab moves focus -------------------------------------------

    thread_local! {
        static TAB_VAL: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
    }

    fn tab_bound() -> Signal<String> {
        TAB_VAL.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = Some(create_signal(String::new()));
            }
            c.unwrap()
        })
    }

    fn tab_probe() -> impl IntoWidget {
        // No autofocus: focus is empty until press_tab lands it on this field.
        text_field().bind(tab_bound()).width(200.0)
    }

    #[test]
    fn press_tab_moves_focus_onto_the_first_field() {
        TAB_VAL.with(|c| *c.borrow_mut() = None);
        let mut h = Harness::new();
        h.mount(|| component(tab_probe));
        h.draw(); // nothing focused yet
        h.press_tab(); // focus lands on the first focusable
        h.key(KeyInput::Insert("z".into()));
        assert_eq!(tab_bound().get(), "z", "press_tab focused the field, so typing reaches it");
    }

    // ----- ime feeds the preedit → commit path -----------------------------

    thread_local! {
        static IME_VAL: RefCell<Option<Signal<String>>> = const { RefCell::new(None) };
    }

    fn ime_bound() -> Signal<String> {
        IME_VAL.with(|c| {
            let mut c = c.borrow_mut();
            if c.is_none() {
                *c = Some(create_signal(String::new()));
            }
            c.unwrap()
        })
    }

    fn ime_probe() -> impl IntoWidget {
        text_field().bind(ime_bound()).autofocus().width(200.0)
    }

    #[test]
    fn ime_commits_composed_text_into_the_editor() {
        IME_VAL.with(|c| *c.borrow_mut() = None);
        let mut h = Harness::new();
        h.mount(|| component(ime_probe));
        h.draw(); // autofocus lands
        h.ime("あ");
        assert_eq!(ime_bound().get(), "あ", "ime() composes then commits the text into the field");
    }

    // ----- raw key events (Focus.onKeyEvent) -------------------------------

    thread_local! {
        static RAW_LOG: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// A focusable that consumes PageUp/PageDown (down and up) and ignores the rest.
    fn raw_key_probe() -> impl IntoWidget {
        use pebbles_core::keyboard::NamedKey;
        use pebbles_core::{KeyEventResult, LogicalKey};
        let node = pebbles_core::create_focus();
        node.register(std::rc::Rc::new(|| {}), None, true);
        node.on_key_event(std::rc::Rc::new(|e| {
            RAW_LOG.with(|l| l.borrow_mut().push(format!("{:?} {:?}", e.kind, e.logical)));
            match e.logical {
                LogicalKey::Named(NamedKey::PageUp | NamedKey::PageDown) => KeyEventResult::Handled,
                _ => KeyEventResult::Ignored,
            }
        }));
        container().width(50.0).height(50.0)
    }

    /// Regression (Gravel terminal): a focused widget had no way to see raw keys —
    /// PageUp/PageDown/Insert/F-keys never reached it, only the editing-intent subset.
    #[test]
    fn focused_widget_receives_raw_key_events_first() {
        use pebbles_core::keyboard::NamedKey;
        use pebbles_core::{KeyEvent, KeyEventKind, KeyEventResult, LogicalKey, PhysicalKey};
        RAW_LOG.with(|l| l.borrow_mut().clear());
        let mut h = Harness::new();
        h.mount(raw_key_probe);
        h.draw();
        let page_up = KeyEvent::down(LogicalKey::Named(NamedKey::PageUp), PhysicalKey::new("PageUp"));
        assert_eq!(h.ui.dispatch_key_event(&page_up), KeyEventResult::Handled);
        let up = KeyEvent { kind: KeyEventKind::Up, ..page_up };
        assert_eq!(h.ui.dispatch_key_event(&up), KeyEventResult::Handled);
        let f5 = KeyEvent::down(LogicalKey::Named(NamedKey::F(5)), PhysicalKey::new("F5"));
        assert_eq!(h.ui.dispatch_key_event(&f5), KeyEventResult::Ignored, "ignored keys fall through");
        RAW_LOG.with(|l| {
            assert_eq!(
                *l.borrow(),
                ["Down Named(PageUp)", "Up Named(PageUp)", "Down Named(F(5))"].map(String::from).to_vec()
            )
        });
        // Nothing focused → nothing to offer the event to.
        pebbles_core::focus::set_focus(None);
        assert_eq!(h.ui.dispatch_key_event(&f5), KeyEventResult::Ignored);
    }

    // ----- physical shortcuts ---------------------------------------------

    thread_local! {
        static TOGGLES: Cell<u32> = const { Cell::new(0) };
    }

    fn physical_shortcut_probe() -> impl IntoWidget {
        pebbles_core::create_shortcut(pebbles_core::Shortcut::physical("Ctrl+Shift+`"), || {
            TOGGLES.with(|t| t.set(t.get() + 1));
        });
        container().width(10.0).height(10.0)
    }

    /// Regression (Gravel terminal toggle): Ctrl+Shift+` only matched layouts whose
    /// Backquote key produces '`'. A physical binding matches the key position on any
    /// layout; the logical character is irrelevant.
    #[test]
    fn physical_shortcut_matches_on_any_layout() {
        use pebbles_core::{KeyEvent, LogicalKey, Mods, PhysicalKey};
        TOGGLES.with(|t| t.set(0));
        let mut h = Harness::new();
        h.mount(physical_shortcut_probe);
        h.draw();
        let window = h.ui.window_id();
        let cs = Mods { ctrl: true, shift: true, ..Default::default() };
        // US: '~' (shifted backquote); German: dead '^'; French: '²'.
        for logical in [LogicalKey::char("~"), LogicalKey::Dead(Some('^')), LogicalKey::char("²")] {
            let e = KeyEvent { mods: cs, ..KeyEvent::down(logical, PhysicalKey::new("Backquote")) };
            assert!(pebbles_core::shortcuts::dispatch_key_event(window, &e));
        }
        assert_eq!(TOGGLES.with(Cell::get), 3);
        // Wrong modifiers or another key position never match.
        let plain = KeyEvent::down(LogicalKey::char("`"), PhysicalKey::new("Backquote"));
        assert!(!pebbles_core::shortcuts::dispatch_key_event(window, &plain));
        let other =
            KeyEvent { mods: cs, ..KeyEvent::down(LogicalKey::char("`"), PhysicalKey::new("IntlBackslash")) };
        assert!(!pebbles_core::shortcuts::dispatch_key_event(window, &other));
        assert_eq!(TOGGLES.with(Cell::get), 3);
    }
}
