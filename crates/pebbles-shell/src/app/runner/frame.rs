//! Frame packets (08 §17, DR-26): the UI thread paints one [`FramePacket`] per window and hands
//! it to a [`FrameSink`]; [`render_packet`] — the ONE render path, with no winit call inside
//! (08 §17.1 T1) — acquires, renders and presents it against the window's surface in the
//! [`RenderState`].

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::mpsc::SyncSender;

use peniko::Color;

use super::gpu::{RenderContext, RenderSurface, Renderer, Scene};

/// A window's key in the render state: winit's id (Copy + Hash + Send).
pub(crate) type WindowKey = winit::window::WindowId;

/// Frames a window may have handed over and not yet seen completed (08 §17.3): one rendering,
/// one queued. Inline frames complete inside the hand-over.
pub(crate) const MAX_IN_FLIGHT: u32 = 2;

/// One window's frame, built on the UI thread.
pub(crate) struct FramePacket {
    /// Per window, +1 per packet.
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // read by the render thread's `FrameDone`
    pub frame_id: u64,
    pub scene: Scene,
    /// `window.inner_size()` in physical px at paint time.
    pub size: (u32, u32),
    /// `window.scale_factor()` → `FrameParams::scale_factor` (R27).
    pub scale_factor: f64,
    pub base_color: Color,
    /// Live-resize sync (08 §17.4, P26).
    #[allow(dead_code)] // set by the threaded live-resize path (P26)
    pub sync: Option<SyncSender<FrameOutcome>>,
}

/// What happened to a packet (mapped from the 08 §5 acquire outcomes).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameOutcome {
    Presented,
    /// Not presented: minimized / 0×0 (§4.4), `Timeout` or `Outdated` after the §5 retry, or
    /// replaced in the mailbox.
    Skipped,
    /// The surface is occluded (§5.4): stop requesting frames for this window until it is
    /// visible again.
    Occluded,
    /// The surface was dropped (§5.1 `Lost`): the UI thread creates a new one (§17.5).
    SurfaceLost,
    /// The device is lost (§6): rebuild the GPU stack.
    DeviceLost,
}

/// Latest-wins slot: `post` replaces an unconsumed packet and returns it (it counts as `Skipped`).
#[derive(Default)]
#[cfg_attr(target_family = "wasm", allow(dead_code))] // the render thread's per-window slot
pub(crate) struct Mailbox(Mutex<Option<FramePacket>>);

#[cfg_attr(target_family = "wasm", allow(dead_code))] // the render thread's per-window slot
impl Mailbox {
    pub(crate) fn post(&self, packet: FramePacket) -> Option<FramePacket> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).replace(packet)
    }

    pub(crate) fn take(&self) -> Option<FramePacket> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take()
    }

    /// Whether the waiting packet carries a live-resize `sync` (08 §17.3 step 4 ordering).
    pub(crate) fn has_sync(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .is_some_and(|p| p.sync.is_some())
    }
}

/// Everything the GPU side owns (06 §5.1): the context, one lazily built renderer per device,
/// and every window's surface — the main window and every secondary window in one map.
pub(crate) struct RenderState {
    pub ctx: RenderContext,
    pub renderers: Vec<Option<Renderer>>,
    pub surfaces: HashMap<WindowKey, RenderSurface<'static>>,
}

// 08 §17.1 T4: the render thread takes the whole state (P25).
#[cfg(not(target_family = "wasm"))]
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<RenderState>();
};

impl RenderState {
    pub(crate) fn new(ctx: RenderContext) -> Self {
        Self { ctx, renderers: Vec::new(), surfaces: HashMap::new() }
    }

    /// Add a window's surface, or replace it after `SurfaceLost` (same key, 08 §17.5).
    pub(crate) fn adopt(&mut self, key: WindowKey, surface: RenderSurface<'static>) {
        self.renderers.resize_with(self.ctx.device_count().max(self.renderers.len()), || None);
        self.surfaces.insert(key, surface);
    }

    /// The renderer for device `idx`, built on first use (renderers rebuild lazily, 08 §6.4).
    pub(crate) fn renderer(&mut self, idx: usize) -> Option<&mut Renderer> {
        if self.renderers.len() <= idx {
            self.renderers.resize_with(idx + 1, || None);
        }
        if self.renderers[idx].is_none() {
            self.renderers[idx] = self.ctx.new_renderer(idx);
        }
        self.renderers[idx].as_mut()
    }

    /// The device index of a window's surface (0 when it has none).
    pub(crate) fn device_of(&self, key: WindowKey) -> usize {
        self.surfaces.get(&key).map_or(0, RenderSurface::device_index)
    }
}

/// Where the UI thread sends frames.
pub(crate) enum FrameSink {
    Inline(RenderState),
    /// Native only (08 §17.8).
    #[cfg(not(target_family = "wasm"))]
    Threaded(super::render_thread::RenderThread),
}

/// What [`FrameSink::submit`] did with a packet.
pub(crate) enum Handover {
    /// Rendered inline; the scene comes back for reuse.
    Rendered(FrameOutcome, Scene),
    /// Queued on the render thread; `FrameDone` reports it. `replaced` is true when it displaced
    /// an unconsumed packet (that one counts as completed, `Skipped`).
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    Queued { replaced: bool },
}

impl FrameSink {
    /// The render state when this thread owns it (inline mode).
    pub(crate) fn state(&self) -> Option<&RenderState> {
        match self {
            Self::Inline(state) => Some(state),
            #[cfg(not(target_family = "wasm"))]
            Self::Threaded(_) => None,
        }
    }

    pub(crate) fn state_mut(&mut self) -> Option<&mut RenderState> {
        match self {
            Self::Inline(state) => Some(state),
            #[cfg(not(target_family = "wasm"))]
            Self::Threaded(_) => None,
        }
    }

    /// Hand a packet over. Inline: rendered now, and the scene comes back for reuse with the
    /// outcome. (The threaded sink of P25 answers through `FrameDone` instead.)
    pub(crate) fn submit(&mut self, key: WindowKey, packet: FramePacket) -> Handover {
        match self {
            Self::Inline(state) => {
                let outcome = render_packet_ref(state, key, &packet);
                Handover::Rendered(outcome, packet.scene)
            }
            #[cfg(not(target_family = "wasm"))]
            Self::Threaded(thread) => Handover::Queued { replaced: thread.submit(key, packet).is_some() },
        }
    }

    /// The instance the UI thread creates surfaces with (08 §17.5).
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn instance(&self) -> wgpu::Instance {
        match self {
            Self::Inline(state) => state.ctx.instance(),
            Self::Threaded(thread) => thread.instance(),
        }
    }

    /// 08 §17.5: adopt a surface the UI thread created (a new window, or a replacement after
    /// `SurfaceLost`). Inline adopts now; threaded hands it to the render thread.
    #[cfg(not(target_family = "wasm"))]
    pub(crate) fn add_window(
        &mut self,
        key: WindowKey,
        surface: wgpu::Surface<'static>,
        size: (u32, u32),
    ) -> Result<(), String> {
        match self {
            Self::Inline(state) => {
                let surface = pollster::block_on(state.ctx.adopt_surface(surface, size))
                    .map_err(|e| super::gpu::error_text(&e))?;
                state.adopt(key, surface);
                Ok(())
            }
            Self::Threaded(thread) => {
                thread.add_window(key, surface, size);
                Ok(())
            }
        }
    }

    /// 08 §17.5 close: drop the window's surface before its window. `false` when the render
    /// thread did not acknowledge in time — keep the window alive (T2).
    pub(crate) fn remove_window(&mut self, key: WindowKey) -> bool {
        match self {
            Self::Inline(state) => {
                state.surfaces.remove(&key);
                true
            }
            #[cfg(not(target_family = "wasm"))]
            Self::Threaded(thread) => thread.remove_window(key),
        }
    }
}

/// The ONE function that renders a packet, in both modes. No winit inside (08 §17.1 T1).
#[cfg_attr(target_family = "wasm", allow(dead_code))] // inline mode reuses the scene instead
pub(crate) fn render_packet(state: &mut RenderState, key: WindowKey, packet: FramePacket) -> FrameOutcome {
    render_packet_ref(state, key, &packet)
}

/// [`render_packet`] by reference, so the inline sink can hand the scene back for reuse.
fn render_packet_ref(state: &mut RenderState, key: WindowKey, packet: &FramePacket) -> FrameOutcome {
    use pebbles_foundation::log::{Cat, Level, logf};
    // 08 §5.3 rule 3: never acquire on a dead device.
    if state.ctx.needs_reset() {
        return FrameOutcome::DeviceLost;
    }
    // (a) A window without a surface (its adoption failed) asks the UI thread for a new one
    // rather than spinning on skipped frames; minimized / 0×0 (§4.4 `Hidden`) presents nothing.
    let Some(idx) = state.surfaces.get(&key).map(RenderSurface::device_index) else {
        return FrameOutcome::SurfaceLost;
    };
    if packet.size.0 == 0 || packet.size.1 == 0 {
        return FrameOutcome::Skipped;
    }
    let RenderState { ctx, renderers, surfaces } = state;
    if renderers.len() <= idx {
        renderers.resize_with(idx + 1, || None);
    }
    if renderers[idx].is_none() {
        renderers[idx] = ctx.new_renderer(idx);
    }
    let (Some(renderer), Some(surface)) = (renderers[idx].as_mut(), surfaces.get_mut(&key)) else {
        return FrameOutcome::DeviceLost;
    };
    // (b)–(e) Configure when needed, acquire, render, present.
    ctx.present(
        renderer,
        surface,
        &packet.scene,
        packet.base_color,
        packet.size,
        packet.scale_factor,
        |line| {
            logf(Level::Debug, Cat::Gpu, format_args!("{line}"));
        },
    )
}

/// Per-window in-flight pacing (08 §17.3 steps 1 and 5).
#[derive(Debug, Default)]
pub(crate) struct Pacer {
    in_flight: u32,
    redraw_pending: bool,
}

impl Pacer {
    /// `RedrawRequested`: whether to build a packet now. At the cap the redraw is remembered
    /// instead; a live-resize frame (P26) bypasses the cap.
    pub(crate) fn begin(&mut self, resize_sync: bool) -> bool {
        if self.in_flight >= MAX_IN_FLIGHT && !resize_sync {
            self.redraw_pending = true;
            return false;
        }
        self.in_flight += 1;
        true
    }

    /// A packet completed (`FrameDone` or the inline return). Returns whether to request a
    /// redraw: a skipped frame is always retried (§5.6), and a deferred redraw runs now.
    pub(crate) fn done(&mut self, outcome: FrameOutcome) -> bool {
        self.in_flight = self.in_flight.saturating_sub(1);
        match outcome {
            FrameOutcome::Skipped => {
                self.redraw_pending = false;
                true
            }
            FrameOutcome::Presented => std::mem::take(&mut self.redraw_pending),
            FrameOutcome::Occluded | FrameOutcome::SurfaceLost | FrameOutcome::DeviceLost => false,
        }
    }
    /// A queued packet was displaced in the mailbox before rendering (08 §17.3 step 3).
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub(crate) fn replaced(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn packet(frame_id: u64) -> FramePacket {
        FramePacket {
            frame_id,
            scene: Scene::new(),
            size: (8, 8),
            scale_factor: 1.0,
            base_color: Color::WHITE,
            sync: None,
        }
    }

    #[test]
    fn mailbox_latest_wins() {
        let mailbox = Mailbox::default();
        assert!(mailbox.post(packet(1)).is_none());
        assert_eq!(mailbox.post(packet(2)).map(|p| p.frame_id), Some(1));
        assert_eq!(mailbox.take().map(|p| p.frame_id), Some(2));
        assert!(mailbox.take().is_none());
    }

    /// A fake threaded sink: packets stay in flight until the test completes them.
    #[test]
    fn in_flight_cap_defers_redraw() {
        let mut pacer = Pacer::default();
        let mut built = 0;
        for _ in 0..3 {
            if pacer.begin(false) {
                built += 1;
            }
        }
        assert_eq!(built, MAX_IN_FLIGHT, "the third RedrawRequested builds no packet");
        let redraws = [pacer.done(FrameOutcome::Presented), pacer.done(FrameOutcome::Presented)];
        assert_eq!(redraws, [true, false], "the next FrameDone requests exactly one redraw");
    }

    #[test]
    fn resize_sync_bypasses_cap() {
        let mut pacer = Pacer::default();
        assert!(pacer.begin(false) && pacer.begin(false));
        assert!(pacer.begin(true), "a resize packet is still built at the cap");
    }

    #[test]
    fn send_bounds() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<Scene>();
        #[cfg(not(target_family = "wasm"))]
        {
            fn send<T: Send>() {}
            send::<RenderState>();
            send::<FramePacket>();
        }
    }
}
