//! The frame pipeline: reconcile → layout → bounds publish → paint → GPU present,
//! for the main window ([`Runner::render`]) and secondary windows
//! ([`Runner::render_window`]). E2 frame stats live here too.

#[allow(clippy::wildcard_imports)]
use super::*;

use vello::kurbo::Stroke;
use vello::peniko::Brush;

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
    /// Reconcile any dirty subtrees, lay out to the window, paint, and present.
    /// If uncaptured GPU errors landed since the last check, rebuild the WHOLE
    /// GPU stack — instance, adapter, device, every surface, every renderer.
    /// A renderer-level rebuild is not enough: a lost/errored DEVICE hands out
    /// invalid resources forever (observed as `Buffer 'vello.scene' is invalid`
    /// straight after a fresh renderer). Throttled to ~1/second so a
    /// persistently broken driver logs steadily instead of thrashing. Returns
    /// whether a reset ran (the caller re-renders immediately after).
    pub(super) fn recover_gpu_if_poisoned(&mut self) -> bool {
        let errs = GPU_ERRORS.load(std::sync::atomic::Ordering::Relaxed);
        if errs == self.gpu_errors_seen {
            return false;
        }
        if self.last_gpu_reset.is_some_and(|t| t.elapsed() < Duration::from_secs(1)) {
            return false; // let the pending reset settle before judging it
        }
        self.gpu_errors_seen = errs;
        self.last_gpu_reset = Some(Instant::now());
        pebbles_core::log::warn(
            pebbles_core::log::Cat::Gpu,
            format!("resetting GPU stack (device + surfaces + renderers) after {errs} errors"),
        );
        eprintln!("pebbles: resetting the GPU stack (device + surfaces + renderers)…");

        // A fresh instance/adapter/device pool; the old one may be lost.
        self.context = Some(RenderContext::new());
        self.renderers = Vec::new();

        // Recreate the main window's surface + renderer on the new device.
        if let Some(active) = self.active.as_mut() {
            let phys = active.window.inner_size();
            match pollster::block_on(self.context.as_mut().unwrap().create_surface(
                active.window.clone(),
                phys.width.max(1),
                phys.height.max(1),
                wgpu::PresentMode::AutoVsync,
            )) {
                Ok(surface) => {
                    self.renderers.resize_with(self.context.as_ref().unwrap().devices.len(), || None);
                    let dh = &self.context.as_ref().unwrap().devices[surface.dev_id];
                    install_error_handler(&dh.device);
                    self.renderers[surface.dev_id] = Some(new_renderer(&dh.device, &dh.queue));
                    active.surface = surface;
                }
                Err(e) => eprintln!("pebbles: GPU reset could not recreate the main surface: {e}"),
            }
        }
        // And every secondary window's surface.
        for w in self.windows.values_mut() {
            let phys = w.window.inner_size();
            match pollster::block_on(self.context.as_mut().unwrap().create_surface(
                w.window.clone(),
                phys.width.max(1),
                phys.height.max(1),
                wgpu::PresentMode::AutoVsync,
            )) {
                Ok(surface) => {
                    self.renderers.resize_with(self.context.as_ref().unwrap().devices.len(), || None);
                    let dh = &self.context.as_ref().unwrap().devices[surface.dev_id];
                    install_error_handler(&dh.device);
                    if self.renderers[surface.dev_id].is_none() {
                        self.renderers[surface.dev_id] = Some(new_renderer(&dh.device, &dh.queue));
                    }
                    w.surface = surface;
                }
                Err(e) => eprintln!("pebbles: GPU reset could not recreate a window surface: {e}"),
            }
        }
        true
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
            GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        if active.surface.config.width != phys.width || active.surface.config.height != phys.height {
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
            self.scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, &Brush::Solid(accent), None, &r);
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

        // 4. Render to the offscreen target and blit to the surface.
        // NOTHING in this GPU section may panic: a desktop app must survive any
        // driver hiccup. On failure: log, bump GPU_ERRORS (the recovery reset
        // picks it up next frame), and skip THIS frame.
        let surface = &active.surface;
        let device_handle = &self.context.as_ref().unwrap().devices[surface.dev_id];
        let renderer = match self.renderers[surface.dev_id].as_mut() {
            Some(r) => r,
            None => self.renderers[surface.dev_id]
                .insert(new_renderer(&device_handle.device, &device_handle.queue)),
        };

        if let Err(e) = renderer.render_to_texture(
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
        ) {
            eprintln!("pebbles: vello render failed (skipping frame, scheduling GPU reset): {e}");
            GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            active.window.request_redraw();
            return;
        }
        // DRIVER WORKAROUND — wait the vello compute pass out before touching the
        // swapchain. On some Linux/Vulkan drivers (seen on RADV/Wayland), letting
        // the blit/present chain queue up while the vello submission is still in
        // flight races in the driver and surfaces as spurious, timing-dependent
        // validation errors ("Texture/Buffer … is invalid") that poison the
        // device. A desktop UI is nowhere near GPU-bound, so the sync costs
        // nothing perceptible; correctness beats pipelining here.
        //
        // BUT bounded: a `Wait` with no timeout blocks THIS (main) thread forever
        // if the submission never completes — a lost device would freeze the app
        // to a black window instead of triggering recovery. Poll returns Timeout,
        // we log it, and fall through; the error handler / reset path takes over.
        // Non-blocking: process any completed GPU work without WAITING. A `Wait`
        // here froze markdown-heavy scenes on Intel (the submission never
        // signalled in time and every frame timed out → black screen). Present
        // immediately; the swapchain + AutoVsync already pace us.
        let _ = device_handle.device.poll(wgpu::PollType::Poll);

        let surface_texture = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            // Timeout / occluded / outdated / lost — skip this frame and try again.
            _ => return,
        };
        let mut encoder = device_handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.blit") });
        let target_view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        surface.blitter.copy(&device_handle.device, &mut encoder, &surface.target_view, &target_view);
        device_handle.queue.submit([encoder.finish()]);
        active.window.pre_present_notify();
        surface_texture.present();

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
        // here cannot refresh its surface in the map — recreate it directly.
        if self.recover_gpu_if_poisoned() {
            let phys = w.window.inner_size();
            if let Ok(surface) = pollster::block_on(self.context.as_mut().unwrap().create_surface(
                w.window.clone(),
                phys.width.max(1),
                phys.height.max(1),
                wgpu::PresentMode::AutoVsync,
            )) {
                self.renderers.resize_with(self.context.as_ref().unwrap().devices.len(), || None);
                let dh = &self.context.as_ref().unwrap().devices[surface.dev_id];
                if self.renderers[surface.dev_id].is_none() {
                    self.renderers[surface.dev_id] = Some(new_renderer(&dh.device, &dh.queue));
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

        let surface = &w.surface;
        let device_handle = &self.context.as_ref().unwrap().devices[surface.dev_id];
        // Same no-panic policy as render(): recreate a missing renderer, skip the
        // frame (and schedule the GPU reset) on a render failure.
        let renderer = match self.renderers[surface.dev_id].as_mut() {
            Some(r) => r,
            None => self.renderers[surface.dev_id]
                .insert(new_renderer(&device_handle.device, &device_handle.queue)),
        };
        if let Err(e) = renderer.render_to_texture(
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
        ) {
            eprintln!("pebbles: vello render failed (skipping frame, scheduling GPU reset): {e}");
            GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            w.window.request_redraw();
            return;
        }
        // Non-blocking (see render()): never Wait on the render submission.
        let _ = device_handle.device.poll(wgpu::PollType::Poll);
        let surface_texture = match surface.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            _ => return,
        };
        let mut encoder = device_handle
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("pebbles.window.blit") });
        let target_view = surface_texture.texture.create_view(&wgpu::TextureViewDescriptor::default());
        surface.blitter.copy(&device_handle.device, &mut encoder, &surface.target_view, &target_view);
        device_handle.queue.submit([encoder.finish()]);
        w.window.pre_present_notify();
        surface_texture.present();

        if pending_tasks || corrective || pebbles_core::animation::active() {
            w.window.request_redraw();
        }
    }
}
