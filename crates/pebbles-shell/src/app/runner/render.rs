//! The frame pipeline: reconcile → layout → bounds publish → paint → GPU present,
//! for the main window ([`Runner::render`]) and secondary windows
//! ([`Runner::render_window`]). E2 frame stats live here too.

#[allow(clippy::wildcard_imports)]
use super::*;

use super::gpu_log;

use kurbo::Stroke;
use peniko::Brush;

/// E2: whether `PEBBLES_FRAME_STATS=1` (or `true`) is set — checked once. Gates the
/// opt-in per-frame timing print in `render()`.
fn frame_stats_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("PEBBLES_FRAME_STATS").is_ok_and(|v| v == "1" || v == "true"))
}

/// X.6 — synthetic device-loss injection: `PEBBLES_SYNTH_GPU_LOSS=N` (on the web the
/// `?pebbles-synth-gpu-loss=N` query, 08 P03) poisons the
/// GPU error counter every N frames, driving the SAME full-stack recovery a real
/// lost device triggers (fresh instance/adapter/device, every surface and
/// renderer rebuilt). Soak tool only — unset in normal runs.
fn synth_gpu_loss_every() -> u64 {
    use std::sync::OnceLock;
    static EVERY: OnceLock<u64> = OnceLock::new();
    *EVERY.get_or_init(|| {
        #[cfg(target_family = "wasm")]
        let every = crate::web_gpu::query_param("pebbles-synth-gpu-loss");
        #[cfg(not(target_family = "wasm"))]
        let every = std::env::var("PEBBLES_SYNTH_GPU_LOSS").ok();
        every.and_then(|v| v.parse().ok()).unwrap_or(0)
    })
}

impl Runner {
    /// The render state, when this thread owns it (inline mode, 08 §17.8).
    pub(super) fn render_state(&self) -> Option<&RenderState> {
        self.sink.as_ref().and_then(FrameSink::state)
    }

    pub(super) fn render_state_mut(&mut self) -> Option<&mut RenderState> {
        self.sink.as_mut().and_then(FrameSink::state_mut)
    }

    /// Rebuild the WHOLE GPU stack — instance, adapter, device, every surface, every renderer —
    /// when a device is lost or poisoned (08 §6.5). A renderer-level rebuild is not enough: a
    /// lost device hands out invalid resources forever. UI state survives (no remount).
    /// Retries are spaced by [`reset_backoff`]. Returns whether a reset ran (the caller re-renders).
    pub(super) fn recover_gpu_if_poisoned(&mut self) -> bool {
        use pebbles_foundation::log::Level;
        // (web) An async recovery is already in flight, or the WebGL context is lost (P04:
        // nothing can be rebuilt until the browser restores it).
        #[cfg(target_family = "wasm")]
        if self.gpu_pending || self.gpu_suspended {
            return false;
        }
        // 1. Poisoned?
        let poisoned = self.gpu_reset_requested || self.render_state().is_some_and(|s| s.ctx.needs_reset());
        if !poisoned {
            return false;
        }
        // 2. Throttle.
        if self.gpu_retry_at.is_some_and(|t| t > Instant::now()) {
            return false;
        }
        let Some(init) = self.host_init.clone() else { return false };
        self.gpu_reset_requested = false;
        let started = Instant::now();
        // 08 §6.7: a reset that never presented counts as failed; the wait doubles from 1 s to a
        // 10 s ceiling, and the process never exits over a lost GPU.
        if self.gpu_reset_unconfirmed {
            self.gpu_reset_failures += 1;
            if self.gpu_reset_failures == 5 {
                gpu_log(
                    Level::Error,
                    &format!(
                        "GPU resets keep failing ({} in a row); retrying every 10 s. {}",
                        self.gpu_reset_failures,
                        gpu::repeated_reset_guidance()
                    ),
                );
            }
        }
        self.gpu_reset_unconfirmed = true;
        self.gpu_retry_at = Some(started + reset_backoff(self.gpu_reset_failures));
        // 3. One line per lost device.
        let g = self.gpu_generation;
        for lost in self.render_state().map(|s| s.ctx.lost_devices()).unwrap_or_default() {
            gpu_log(Level::Error, &format!("GPU device lost (generation={g}, {lost})"));
        }
        // 4. Drop every renderer and surface (old swapchains go before new ones are configured).
        self.sink = None;
        #[cfg(target_family = "wasm")]
        {
            self.start_web_recovery(init, started);
            false
        }
        #[cfg(not(target_family = "wasm"))]
        {
            self.rebuild_gpu_native(&init, started);
            true
        }
    }

    /// 08 §6.5 steps 5–7 (native): a fresh context, every window's surface, redraw.
    #[cfg(not(target_family = "wasm"))]
    fn rebuild_gpu_native(&mut self, init: &HostInit, started: Instant) {
        use pebbles_foundation::log::Level;
        let mut ctx = pollster::block_on(new_context(init));
        let mut failure: Option<String> = None;
        let windows = self.active.iter().map(|a| &a.window).chain(self.windows.values().map(|w| &w.window));
        let mut surfaces = Vec::new();
        for window in windows {
            let phys = window.inner_size();
            match pollster::block_on(ctx.create_surface(
                window.clone(),
                phys.width.max(1),
                phys.height.max(1),
            )) {
                Ok(surface) => surfaces.push((window.id(), surface)),
                Err(e) => failure = failure.or(Some(gpu::error_text(&e))),
            }
        }
        let mut state = RenderState::new(ctx);
        for (key, surface) in surfaces {
            state.adopt(key, surface);
        }
        self.sink = Some(FrameSink::Inline(state));
        if let Some(error) = failure {
            self.gpu_reset_requested = true;
            let n = self.gpu_reset_failures + 1;
            let wait = reset_backoff(n).as_secs();
            gpu_log(
                Level::Error,
                &format!("GPU stack rebuild failed (attempt {n}, retry in {wait} s): {error}"),
            );
        } else {
            self.gpu_generation += 1;
            let ms = started.elapsed().as_millis();
            gpu_log(Level::Warn, &format!("GPU stack rebuilt (generation={}, {ms} ms)", self.gpu_generation));
        }
        self.request_redraw();
        for w in self.windows.values() {
            w.window.request_redraw();
        }
    }

    /// 08 §6.6 (web): never block the browser thread — rebuild on a `spawn_local` task and hand
    /// the result back through the event-loop proxy (`SurfaceReady` / `GpuRecoveryFailed`).
    #[cfg(target_family = "wasm")]
    pub(super) fn start_web_recovery(&mut self, init: HostInit, started: Instant) {
        use pebbles_foundation::log::Level;
        let window = self.active.take().map(|a| a.window).or_else(|| self.recovery_window.take());
        let (Some(window), Some(proxy)) = (window, self.proxy.clone()) else {
            gpu_log(
                Level::Error,
                "GPU stack rebuild failed (attempt 1, retry in 1 s): no window or event-loop proxy",
            );
            self.gpu_reset_requested = true;
            return;
        };
        self.gpu_pending = true;
        self.sink = None;
        self.recovery_started = Some(started);
        let phys = window.inner_size();
        let (w, h) = (phys.width.max(1), phys.height.max(1));
        wasm_bindgen_futures::spawn_local(async move {
            let mut context = new_context(&init).await;
            let event = match context.create_surface(window.clone(), w, h).await {
                Ok(surface) => PebblesUserEvent::SurfaceReady { context, surface, window },
                Err(e) => PebblesUserEvent::GpuRecoveryFailed { window, error: gpu::error_text(&e) },
            };
            let _ = proxy.send_event(event);
        });
    }

    pub(super) fn render(&mut self) {
        // (web) The WebGL context is lost: nothing to draw with until it is restored (P04).
        #[cfg(target_family = "wasm")]
        if self.gpu_suspended {
            return;
        }
        use pebbles_core::log;
        // Frame heartbeat: bump the counter and log a periodic pulse (and every
        // slow frame). If the log stops pulsing, the loop froze — and the LAST
        // line tells you exactly which stage it died in.
        self.frame_no += 1;
        let frame_start = Instant::now();
        let fno = self.frame_no;
        log::trace(log::Cat::Frame, format!("frame {fno} begin"));
        pebbles_render::stats::reset_frame();
        pebbles_core::reactive_stats::reset();
        // X.6: inject a synthetic device loss on schedule (soak tool; no-op unset).
        let synth = synth_gpu_loss_every();
        if synth > 0 && self.frame_no.is_multiple_of(synth) {
            eprintln!("pebbles: SYNTH device loss injected (frame {fno})");
            // stderr is invisible in a browser: echo to the console too.
            #[cfg(target_family = "wasm")]
            web_sys::console::warn_1(&format!("pebbles: SYNTH device loss injected (frame {fno})").into());
            if let Some(state) = self.render_state() {
                state.ctx.inject_synthetic_loss();
            }
        }
        if self.recover_gpu_if_poisoned() {
            // Render fresh state this same frame; also repaint secondary windows.
            for w in self.windows.values() {
                w.window.request_redraw();
            }
        }
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

        // GC an overlay whose opener unmounted (navigation while a dropdown was up)
        // BEFORE reconciling — its content must never re-render against disposed
        // signals.
        self.ui.make_current();
        pebbles_widgets::overlay::gc_dead();

        let Some(active) = self.active.as_mut() else { return };
        // 08 §5.4: an occluded window renders nothing and requests no frames until a
        // visibility event clears the flag (otherwise macOS spins on instant occluded acquires).
        if !should_render(active.occluded) {
            return;
        }

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
        // One font poll per frame, on the main window: a merged system-font scan re-lays out
        // every window.
        if self.text.poll_fonts() {
            self.ui.invalidate_layout();
            for w in self.windows.values_mut() {
                w.ui.invalidate_layout();
                w.window.request_redraw();
            }
        }
        let t = Instant::now();
        self.ui.layout(&mut self.text, logical);
        let layout = t.elapsed();

        // NaN tripwire (dev mode): a non-finite size/offset becomes a NaN path coordinate that
        // the renderer drops (hiding a layout bug). Throttled to every 8th frame — the full-tree
        // scan is dev-only diagnostics, not something every frame should pay for.
        if log::dev_mode()
            && fno.is_multiple_of(8)
            && let Some(bad) = self.ui.render_tree().nan_report()
        {
            log::error(log::Cat::Layout, format!("NON-FINITE layout (NaN/∞): {bad}"));
        }

        // 2b. Publish laid-out rects for components using `use_bounds()` (C2 tooltip
        // focus positioning, …); GC keys whose element unmounted.
        {
            let win = self.ui.window_id();
            let tree = self.ui.render_tree();
            for (w, src) in pebbles_core::bounds::wanted_bounds() {
                if w != win {
                    continue;
                }
                match tree.find_by_source(src) {
                    Some(rid) => {
                        let o = tree.absolute_offset(rid);
                        let s = tree.size_of(rid);
                        pebbles_core::bounds::publish_bounds(
                            w,
                            src,
                            pebbles_foundation::Rect::new(o.x, o.y, o.x + s.width, o.y + s.height),
                        );
                    }
                    None => pebbles_core::bounds::forget_bounds(w, src),
                }
            }
            // Publish the focused element's rect (C2 tooltip show_on_focus).
            let focus_rect = pebbles_core::focus::focused_element_ffi(win)
                .and_then(|id| tree.find_by_source(id))
                .map(|rid| {
                    let o = tree.absolute_offset(rid);
                    let s = tree.size_of(rid);
                    pebbles_foundation::Rect::new(o.x, o.y, o.x + s.width, o.y + s.height)
                });
            pebbles_core::bounds::set_focus_bounds(focus_rect);
        }

        // 3. Paint the logical scene, then scale it to physical pixels.
        let t = Instant::now();
        self.scene.reset();
        let corrective = self.ui.paint(&mut self.text, &mut self.scene);
        // The focused editor's caret moved: put the OS IME candidate window under it (03b E17).
        if let Some(caret) = pebbles_render::text_edit::take_ime_caret_update() {
            place_ime(&active.window, caret, scale);
        }
        // F2: overlay the inspector outline (logical space, before the DPI scale).
        if self.inspect_mode
            && let Some(r) = self.inspect_rect
        {
            let accent = Color::from_rgba8(56, 189, 248, 255); // sky-400
            // Draw through a Painter so this works on both backends (the op-list Scene
            // has no direct `stroke`; only the seam's Painter does).
            pebbles_render::paint::Painter::new(&mut self.scene).stroke(
                &Stroke::new(1.0),
                Affine::IDENTITY,
                &Brush::Solid(accent),
                None,
                &r,
            );
        }
        self.frame.reset();
        self.frame.append(&self.scene, Some(Affine::scale(scale)));
        let encode = t.elapsed();
        // Advance the shaped-text cache generation (evicts only under pressure).
        self.text.finish_frame();

        // E2: opt-in per-frame CPU-side timing (`PEBBLES_FRAME_STATS=1`). GPU submit
        // below is excluded — this measures rebuild/layout/scene-encode, the parts a
        // damage/relayout optimization would target. Object count is the debug census.
        if stats {
            #[cfg(debug_assertions)]
            let objects = self.ui.render_node_count().to_string();
            #[cfg(not(debug_assertions))]
            let objects = String::from("n/a (debug-only census)");
            use pebbles_render::stats;
            eprintln!(
                "[pebbles frame] rebuild={:.2}ms layout={:.2}ms encode={:.2}ms objects={objects} \
                 layouts={} skips={} painted={} culled={} glyph_runs={}",
                rebuild.as_secs_f64() * 1e3,
                layout.as_secs_f64() * 1e3,
                encode.as_secs_f64() * 1e3,
                stats::layout_calls(),
                stats::layout_skips(),
                stats::painted_nodes(),
                stats::culled_nodes(),
                stats::glyph_runs(),
            );
        }
        // Reactive-runtime counters (opt-in): what this frame's writes actually cost.
        if std::env::var("PEBBLES_REACTIVE_STATS").is_ok_and(|v| v == "1" || v == "true") {
            eprintln!("[pebbles reactive] {}", pebbles_core::reactive_stats::summary());
        }

        // 4. Hand the packet to the sink (08 §17.3). NOTHING here may panic — a failed frame is
        // skipped and retried; a poisoned device reaches the reset via `needs_reset()`.
        let key = active.window.id();
        let Some(outcome) = submit_frame(
            &mut self.sink,
            key,
            &active.window,
            &mut active.pacer,
            &mut active.frame_id,
            &mut self.frame,
            self.background,
        ) else {
            return;
        };
        if stats && let Some(line) = self.render_state().and_then(|s| stats_line(s, key)) {
            eprintln!("[pebbles gpu] {line}");
        }
        let Some(active) = self.active.as_mut() else { return };
        note_first_present(
            &mut self.first_present_logged,
            self.started_at,
            outcome == FrameOutcome::Presented,
        );
        if outcome == FrameOutcome::Presented {
            // 08 §6.7: the first present after a reset confirms it.
            self.gpu_reset_unconfirmed = false;
            self.gpu_reset_failures = 0;
        } else {
            follow_up(
                outcome,
                &mut self.sink,
                &mut self.gpu_reset_requested,
                &active.window,
                &mut active.occluded,
            );
            return;
        }

        // Keep the frames coming while any animation or scroll spring is running —
        // or while a lazy paint-time measurement requested a corrective relayout
        // (P5.2 estimate-then-measure settle).
        if scrolling || pending_tasks || corrective || pebbles_core::animation::active() {
            active.window.request_redraw();
        }

        // 5. Publish the accessibility tree + focus for this frame (post-layout, so
        // bounds are current). The `active` borrow has ended above. Collected ONLY
        // when an assistive-technology adapter is live — the full-tree walk is
        // wasted work otherwise.
        // 11 §2.9: the soak sampler counts presented frames (the `active` borrow has ended).
        self.soak_frame();

        if self.a11y.is_some() {
            let nodes = self.ui.render_tree().semantics_tree();
            let focus = pebbles_core::focus::focused_element_ffi(self.ui.window_id());
            if let Some(a11y) = self.a11y.as_mut() {
                a11y.update(&nodes, focus);
            }
        }

        // Heartbeat: pulse every 120 frames, and flag any frame slower than 32ms
        // (a dropped frame at 60fps) so jank/stalls are visible at DEBUG. At TRACE,
        // emit the full per-stage breakdown for EVERY frame — step-by-step detail.
        let total = frame_start.elapsed();
        let line = || {
            format!(
                "frame {fno} done in {:.1}ms (rebuild {:.1} layout {:.1} encode {:.1} objects {})",
                total.as_secs_f64() * 1e3,
                rebuild.as_secs_f64() * 1e3,
                layout.as_secs_f64() * 1e3,
                encode.as_secs_f64() * 1e3,
                self.ui.render_tree().node_count(),
            )
        };
        if fno.is_multiple_of(120) || total.as_millis() > 32 {
            log::debug(log::Cat::Frame, line());
        } else if log::dev_mode() {
            // The per-frame breakdown string only exists where TRACE can print it.
            log::trace(log::Cat::Frame, line());
        }
        // Content-change tripwire: when the render-object count changes, the
        // on-screen content actually changed this frame (e.g. a navigation swapped
        // screens). If you navigate and this NEVER fires, the content isn't
        // updating — the reconcile isn't producing a new tree.
        let objects = self.ui.render_tree().node_count();
        if objects != self.last_objects {
            log::debug(
                log::Cat::Frame,
                format!("content changed: {} → {objects} objects (frame {fno})", self.last_objects),
            );
            self.last_objects = objects;
        }
    }

    /// Render one secondary window (mirrors [`render`], reusing the shared scene,
    /// renderers and GPU context).
    pub(super) fn render_window(&mut self, w: &mut WindowRuntime) {
        // 08 §5.7 (P12): a hidden window renders nothing and requests no frames.
        if !should_render(w.occluded) {
            return;
        }
        // NOTE: `w` is detached from `self.windows` during dispatch, so a reset
        // here cannot refresh its surface in the map — recreate it directly (native only:
        // the web rebuild is async, 08 §6.6).
        #[cfg(not(target_family = "wasm"))]
        if self.recover_gpu_if_poisoned()
            && let Some(state) = self.render_state_mut()
        {
            let phys = w.window.inner_size();
            if let Ok(surface) = pollster::block_on(state.ctx.create_surface(
                w.window.clone(),
                phys.width.max(1),
                phys.height.max(1),
            )) {
                state.adopt(w.window.id(), surface);
            }
        }
        let now = self.clock.elapsed().as_secs_f64();
        pebbles_core::animation::tick(now);
        let pending_tasks = pebbles_core::task::pump();
        pebbles_render::stats::reset_frame();

        // GC a dead overlay first (see render()); make_current so it hits THIS window's.
        w.ui.make_current();
        pebbles_widgets::overlay::gc_dead();
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
        let corrective = w.ui.paint(&mut self.text, &mut self.scene);
        if let Some(caret) = pebbles_render::text_edit::take_ime_caret_update() {
            place_ime(&w.window, caret, scale);
        }
        self.frame.reset();
        self.frame.append(&self.scene, Some(Affine::scale(scale)));
        self.text.finish_frame();

        let key = w.window.id();
        let Some(outcome) = submit_frame(
            &mut self.sink,
            key,
            &w.window,
            &mut w.pacer,
            &mut w.frame_id,
            &mut self.frame,
            w.background,
        ) else {
            return;
        };
        if frame_stats_enabled()
            && let Some(line) = self.render_state().and_then(|s| stats_line(s, key))
        {
            eprintln!("[pebbles gpu] {line}");
        }
        note_first_present(
            &mut self.first_present_logged,
            self.started_at,
            outcome == FrameOutcome::Presented,
        );
        if outcome == FrameOutcome::Presented {
            // 08 §6.7: the first present after a reset confirms it.
            self.gpu_reset_unconfirmed = false;
            self.gpu_reset_failures = 0;
        } else {
            // An occluded acquire marks the window hidden until a visibility event (P12).
            follow_up(outcome, &mut self.sink, &mut self.gpu_reset_requested, &w.window, &mut w.occluded);
            return;
        }

        if pending_tasks || corrective || pebbles_core::animation::active() {
            w.window.request_redraw();
        }
    }
}

/// The occluded gate (08 §5.4): render only visible windows.
fn should_render(occluded: bool) -> bool {
    !occluded
}

/// Build `window`'s packet from the painted `frame` and hand it to the sink (08 §17.3 steps 1–2
/// and 5). The scene comes back for reuse. `None`: no sink yet, or the window is at its
/// in-flight cap (the redraw is remembered).
fn submit_frame(
    sink: &mut Option<FrameSink>,
    key: frame::WindowKey,
    window: &Window,
    pacer: &mut Pacer,
    frame_id: &mut u64,
    frame: &mut Scene,
    base_color: Color,
) -> Option<FrameOutcome> {
    let sink = sink.as_mut()?;
    if !pacer.begin(false) {
        return None;
    }
    *frame_id += 1;
    let size = window.inner_size();
    let packet = FramePacket {
        frame_id: *frame_id,
        scene: std::mem::replace(frame, Scene::new()),
        size: (size.width, size.height),
        scale_factor: window.scale_factor(),
        base_color,
        sync: None,
    };
    // 08 §17.1 T1: the last winit call before the hand-over.
    window.pre_present_notify();
    let (outcome, scene) = sink.submit(key, packet);
    *frame = scene;
    if pacer.done(outcome) {
        window.request_redraw();
    }
    Some(outcome)
}

/// The `[pebbles gpu]` stats line of the renderer that drew window `key`.
fn stats_line(state: &RenderState, key: frame::WindowKey) -> Option<String> {
    let renderer = state.renderers.get(state.device_of(key))?.as_ref()?;
    gpu::frame_stats_line(renderer)
}

/// The runner's follow-up for a frame that was not presented (08 §5.1, §5.6, §17.3 step 5):
/// every window ends presented, with a redraw scheduled, or occluded.
fn follow_up(
    outcome: FrameOutcome,
    sink: &mut Option<FrameSink>,
    reset_requested: &mut bool,
    window: &Arc<Window>,
    occluded: &mut bool,
) {
    match outcome {
        // `Pacer::done` already re-requested a skipped frame (§5.6).
        FrameOutcome::Presented | FrameOutcome::Skipped => {}
        FrameOutcome::Occluded => *occluded = true,
        FrameOutcome::SurfaceLost => {
            #[cfg(not(target_family = "wasm"))]
            {
                use pebbles_foundation::log::{Cat, Level, logf};
                let phys = window.inner_size();
                let state = sink.as_mut().and_then(FrameSink::state_mut);
                let recreated = state.map(|state| {
                    let surface = pollster::block_on(state.ctx.create_surface(
                        window.clone(),
                        phys.width.max(1),
                        phys.height.max(1),
                    ));
                    surface.map(|surface| state.adopt(window.id(), surface))
                });
                match recreated {
                    Some(Ok(())) => {
                        logf(
                            Level::Warn,
                            Cat::Gpu,
                            format_args!("surface of {:?} lost — recreated", window.id()),
                        );
                    }
                    // Recreation failed: rebuild the whole stack (08 §5.1 `Lost` row).
                    _ => *reset_requested = true,
                }
            }
            // wasm recreates asynchronously through the GPU recovery path (C20).
            #[cfg(target_family = "wasm")]
            {
                let _ = sink;
                *reset_requested = true;
            }
            window.request_redraw();
        }
        FrameOutcome::DeviceLost => {
            *reset_requested = true;
            window.request_redraw();
        }
    }
}

/// The wait before the next GPU reset after `failures` consecutive failed resets (08 §6.7):
/// 1 s, 2 s, 4 s, 8 s, then 10 s forever.
pub(super) fn reset_backoff(failures: u32) -> Duration {
    Duration::from_secs(1u64.checked_shl(failures).unwrap_or(u64::MAX).clamp(1, 10))
}

#[cfg(test)]
mod tests {
    use super::{reset_backoff, should_render};

    /// 08 §5.7: an occluded window renders nothing (and so requests no frames).
    #[test]
    fn occluded_skips_frame() {
        assert!(!should_render(true));
        assert!(should_render(false));
    }

    #[test]
    fn reset_backoff_sequence() {
        let secs = |k| reset_backoff(k).as_secs();
        assert_eq!([secs(0), secs(1), secs(2), secs(3), secs(4), secs(9)], [1, 2, 4, 8, 10, 10]);
        assert_eq!(secs(64), 10, "no shift overflow");
        assert_eq!(secs(u32::MAX), 10);
    }
}
