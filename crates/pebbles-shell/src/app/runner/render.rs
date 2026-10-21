//! The frame pipeline: reconcile → layout → bounds publish → paint → GPU present,
//! for the main window ([`Runner::render`]) and secondary windows
//! ([`Runner::render_window`]). E2 frame stats live here too.

#[allow(clippy::wildcard_imports)]
use super::*;

use super::gpu_log;
use crate::gpu::{PresentOutcome, present_frame};

use kurbo::Stroke;
use peniko::Brush;

/// E2: whether `PEBBLES_FRAME_STATS=1` (or `true`) is set — checked once. Gates the
/// opt-in per-frame timing print in `render()`.
fn frame_stats_enabled() -> bool {
    use std::sync::OnceLock;
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var("PEBBLES_FRAME_STATS").is_ok_and(|v| v == "1" || v == "true"))
}

/// X.6 — synthetic device-loss injection: `PEBBLES_SYNTH_GPU_LOSS=N` poisons the
/// GPU error counter every N frames, driving the SAME full-stack recovery a real
/// lost device triggers (fresh instance/adapter/device, every surface and
/// renderer rebuilt). Soak tool only — unset in normal runs.
fn synth_gpu_loss_every() -> u64 {
    use std::sync::OnceLock;
    static EVERY: OnceLock<u64> = OnceLock::new();
    *EVERY.get_or_init(|| {
        std::env::var("PEBBLES_SYNTH_GPU_LOSS").ok().and_then(|v| v.parse().ok()).unwrap_or(0)
    })
}

impl Runner {
    /// Rebuild the WHOLE GPU stack — instance, adapter, device, every surface, every renderer —
    /// when a device is lost or poisoned (08 §6.5). A renderer-level rebuild is not enough: a
    /// lost device hands out invalid resources forever. UI state survives (no remount).
    /// Retries at most once per second. Returns whether a reset ran (the caller re-renders).
    pub(super) fn recover_gpu_if_poisoned(&mut self) -> bool {
        use pebbles_foundation::log::Level;
        // (web) An async recovery is already in flight.
        #[cfg(target_family = "wasm")]
        if self.gpu_pending {
            return false;
        }
        // 1. Poisoned?
        let poisoned = self.gpu_reset_requested || self.context.as_ref().is_some_and(|c| c.needs_reset());
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
        self.gpu_retry_at = Some(started + Duration::from_secs(1));
        // 3. One line per lost device.
        let g = self.gpu_generation;
        for lost in self.context.as_ref().map(|c| c.lost_devices()).unwrap_or_default() {
            gpu_log(Level::Error, &format!("GPU device lost (generation={g}, {lost})"));
        }
        // 4. Drop every renderer.
        self.renderers = Vec::new();
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
        self.context = Some(pollster::block_on(new_context(init)));
        let mut failure: Option<String> = None;
        if let (Some(active), Some(ctx)) = (self.active.as_mut(), self.context.as_mut()) {
            let phys = active.window.inner_size();
            match pollster::block_on(ctx.create_surface(
                active.window.clone(),
                phys.width.max(1),
                phys.height.max(1),
            )) {
                Ok(surface) => active.surface = surface,
                Err(e) => failure = Some(crate::gpu::error_text(&e)),
            }
        }
        for w in self.windows.values_mut() {
            let Some(ctx) = self.context.as_mut() else { break };
            let phys = w.window.inner_size();
            match pollster::block_on(ctx.create_surface(
                w.window.clone(),
                phys.width.max(1),
                phys.height.max(1),
            )) {
                Ok(surface) => w.surface = surface,
                Err(e) => failure = failure.or(Some(crate::gpu::error_text(&e))),
            }
        }
        if let Some(ctx) = self.context.as_ref() {
            self.renderers.resize_with(ctx.device_count(), || None);
        }
        if let Some(error) = failure {
            self.gpu_reset_failures += 1;
            self.gpu_reset_requested = true;
            let n = self.gpu_reset_failures;
            gpu_log(Level::Error, &format!("GPU stack rebuild failed (attempt {n}, retry in 1 s): {error}"));
        } else {
            self.gpu_reset_failures = 0;
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
    fn start_web_recovery(&mut self, init: HostInit, started: Instant) {
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
        self.context = None;
        self.recovery_started = Some(started);
        let phys = window.inner_size();
        let (w, h) = (phys.width.max(1), phys.height.max(1));
        wasm_bindgen_futures::spawn_local(async move {
            let mut context = new_context(&init).await;
            let event = match context.create_surface(window.clone(), w, h).await {
                Ok(surface) => PebblesUserEvent::SurfaceReady { context, surface, window },
                Err(e) => PebblesUserEvent::GpuRecoveryFailed { window, error: crate::gpu::error_text(&e) },
            };
            let _ = proxy.send_event(event);
        });
    }

    pub(super) fn render(&mut self) {
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
            if let Some(ctx) = self.context.as_ref() {
                ctx.inject_synthetic_loss();
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
        // Web: the true canvas size only arrives AFTER creation (winit reports 0×0
        // right after `create_window`), and the `Resized` carrying it fires during
        // the async GPU-init gap while `self.active` is still `None`, so that event
        // is dropped. Self-heal — make the surface match the window before drawing,
        // so we never render into a stale 1×1 target (which shows as a grey box).
        // Only on size change, to avoid the known wasm dpr resize feedback loop.
        #[cfg(target_family = "wasm")]
        if active.surface.size() != (phys.width, phys.height) {
            self.context.as_mut().unwrap().resize_surface(&mut active.surface, phys.width, phys.height);
        }
        let logical = Size::new(phys.width as f64 / scale, phys.height as f64 / scale);
        pebbles_widgets::overlay::set_window_size(logical.width, logical.height);
        let t = Instant::now();
        self.ui.layout(&mut self.text, logical);
        let layout = t.elapsed();

        // NaN tripwire (dev mode): a non-finite size/offset becomes a NaN path
        // coordinate that corrupts vello's GPU glyph atlas and panics its CPU
        // renderer. Throttled to every 8th frame — the full-tree scan is dev-only
        // diagnostics, not something every frame should pay for.
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

        // 4. Render + present. The host's `present_frame` acquires the swapchain image,
        // renders straight into it and presents. NOTHING here may panic — on failure the
        // host logs and skips THIS frame; a poisoned device reaches the reset via `needs_reset()`.
        let surface = &mut active.surface;
        let idx = surface.device_index();
        let renderer = match self.renderers[idx].as_mut() {
            Some(r) => r,
            None => self.renderers[idx]
                .insert(self.context.as_ref().unwrap().new_renderer(idx).expect("renderer")),
        };
        let outcome = present_frame(
            self.context.as_ref().unwrap(),
            renderer,
            surface,
            &self.frame,
            self.background,
            &active.window,
        );
        if stats && let Some(line) = crate::gpu::frame_stats_line(renderer) {
            eprintln!("[pebbles gpu] {line}");
        }
        note_first_present(
            &mut self.first_present_logged,
            self.started_at,
            outcome == PresentOutcome::Presented,
        );
        if outcome != PresentOutcome::Presented {
            follow_up(
                outcome,
                &mut self.context,
                &mut self.gpu_reset_requested,
                &active.window,
                &mut active.surface,
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
        // NOTE: `w` is detached from `self.windows` during dispatch, so a reset
        // here cannot refresh its surface in the map — recreate it directly (native only:
        // the web rebuild is async, 08 §6.6).
        #[cfg(not(target_family = "wasm"))]
        if self.recover_gpu_if_poisoned() {
            let phys = w.window.inner_size();
            if let Ok(surface) = pollster::block_on(self.context.as_mut().unwrap().create_surface(
                w.window.clone(),
                phys.width.max(1),
                phys.height.max(1),
            )) {
                self.renderers.resize_with(self.context.as_ref().unwrap().device_count(), || None);
                let idx = surface.device_index();
                if self.renderers[idx].is_none() {
                    self.renderers[idx] = self.context.as_ref().unwrap().new_renderer(idx);
                }
                w.surface = surface;
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
        self.frame.reset();
        self.frame.append(&self.scene, Some(Affine::scale(scale)));
        self.text.finish_frame();

        let surface = &mut w.surface;
        let idx = surface.device_index();
        // Same no-panic policy as render(): recreate a missing renderer, skip the
        // frame (and schedule the GPU reset) on a render failure.
        let renderer = match self.renderers[idx].as_mut() {
            Some(r) => r,
            None => self.renderers[idx]
                .insert(self.context.as_ref().unwrap().new_renderer(idx).expect("renderer")),
        };
        let outcome = present_frame(
            self.context.as_ref().unwrap(),
            renderer,
            surface,
            &self.frame,
            w.background,
            &w.window,
        );
        if frame_stats_enabled()
            && let Some(line) = crate::gpu::frame_stats_line(renderer)
        {
            eprintln!("[pebbles gpu] {line}");
        }
        note_first_present(
            &mut self.first_present_logged,
            self.started_at,
            outcome == PresentOutcome::Presented,
        );
        if outcome != PresentOutcome::Presented {
            // Secondary windows track occlusion from P12; an occluded acquire just skips here.
            let mut occluded = false;
            follow_up(
                outcome,
                &mut self.context,
                &mut self.gpu_reset_requested,
                &w.window,
                &mut w.surface,
                &mut occluded,
            );
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

/// The runner's follow-up for a frame that was not presented (08 §5.1, §5.6): every window ends
/// presented, with a redraw scheduled, or occluded.
fn follow_up(
    outcome: PresentOutcome,
    context: &mut Option<RenderContext>,
    reset_requested: &mut bool,
    window: &Arc<Window>,
    surface: &mut RenderSurface<'static>,
    occluded: &mut bool,
) {
    match outcome {
        PresentOutcome::Presented | PresentOutcome::Skipped | PresentOutcome::RedrawRequested => {}
        PresentOutcome::Occluded => *occluded = true,
        PresentOutcome::RecreateSurface => {
            let id = surface.device_index();
            #[cfg(not(target_family = "wasm"))]
            {
                use pebbles_foundation::log::{Cat, Level, logf};
                let phys = window.inner_size();
                let recreated = context.as_mut().map(|ctx| {
                    pollster::block_on(ctx.create_surface(
                        window.clone(),
                        phys.width.max(1),
                        phys.height.max(1),
                    ))
                });
                match recreated {
                    Some(Ok(new)) => {
                        *surface = new;
                        logf(Level::Warn, Cat::Gpu, format_args!("surface #{id} lost — recreated"));
                    }
                    // Recreation failed: rebuild the whole stack (08 §5.1 `Lost` row).
                    _ => *reset_requested = true,
                }
            }
            // wasm recreates asynchronously through the GPU recovery path (C20).
            #[cfg(target_family = "wasm")]
            {
                let _ = (context, id);
                *reset_requested = true;
            }
            window.request_redraw();
        }
        PresentOutcome::ResetGpu => {
            *reset_requested = true;
            window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_render;

    /// 08 §5.7: an occluded window renders nothing (and so requests no frames).
    #[test]
    fn occluded_skips_frame() {
        assert!(!should_render(true));
        assert!(should_render(false));
    }
}
