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
    /// Built after user input reached the window: the render thread draws it before frames
    /// that only advance an animation (08 §17.3 step 4), so typing into one window is not
    /// queued behind other windows' animation frames.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub input: bool,
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

    /// Whether a packet waits unconsumed.
    pub(crate) fn is_full(&self) -> bool {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).is_some()
    }

    /// The waiting packet's place in a render-thread batch (08 §17.3 step 4): 0 when it carries
    /// a live-resize `sync`, 1 when it was built after user input, 2 otherwise (and when empty).
    pub(crate) fn rank(&self) -> u8 {
        match self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
            Some(p) if p.sync.is_some() => 0,
            Some(p) if p.input => 1,
            _ => 2,
        }
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
    /// `pre_present` runs right before an inline present (winit's `pre_present_notify`); the
    /// render thread never calls it (T1).
    pub(crate) fn submit(&mut self, key: WindowKey, packet: FramePacket, pre_present: &dyn Fn()) -> Handover {
        match self {
            Self::Inline(state) => {
                let outcome = render_packet_ref(state, key, &packet, pre_present);
                Handover::Rendered(outcome, packet.scene)
            }
            #[cfg(not(target_family = "wasm"))]
            Self::Threaded(thread) => Handover::Queued { replaced: thread.submit(key, packet).is_some() },
        }
    }

    /// Whether `key`'s previous packet still waits unconsumed in the render thread's mailbox
    /// (08 §17.3 step 1; inline frames never wait).
    pub(crate) fn queued(&self, key: WindowKey) -> bool {
        match self {
            Self::Inline(_) => {
                let _ = key;
                false
            }
            #[cfg(not(target_family = "wasm"))]
            Self::Threaded(thread) => thread.queued(key),
        }
    }

    /// `PEBBLES_SYNTH_GPU_LOSS` (08 §6.9): destroy the real devices, wherever they live.
    pub(crate) fn inject_synthetic_loss(&self) {
        match self {
            Self::Inline(state) => state.ctx.inject_synthetic_loss(),
            #[cfg(not(target_family = "wasm"))]
            Self::Threaded(thread) => thread.simulate_loss(),
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
    render_packet_ref(state, key, &packet, &|| {})
}

/// [`render_packet`] by reference, so the inline sink can hand the scene back for reuse.
fn render_packet_ref(
    state: &mut RenderState,
    key: WindowKey,
    packet: &FramePacket,
    pre_present: &dyn Fn(),
) -> FrameOutcome {
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
        pre_present,
        |line| {
            logf(Level::Debug, Cat::Gpu, format_args!("{line}"));
        },
    )
}

/// How long the UI thread waits for a live-resize frame to be presented (08 §17.4).
pub(crate) const RESIZE_SYNC_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(100);

/// A window's live-resize state (08 §17.4).
#[derive(Debug, Default)]
pub(crate) struct ResizeSync {
    /// Set by `Resized` / `ScaleFactorChanged`; the next frame waits for its present.
    pub pending: bool,
    /// The timeout was logged for this window (once per window).
    pub logged: bool,
}

/// Per-window in-flight pacing (08 §17.3 steps 1 and 5).
#[derive(Debug, Default)]
pub(crate) struct Pacer {
    in_flight: u32,
    redraw_pending: bool,
    /// A frame of this window was presented (a live-resize wait needs something on screen).
    presented: bool,
    /// User input reached the window since its last packet (`FramePacket::input`).
    input: bool,
}

impl Pacer {
    /// Whether to build a packet now (the UI frame already ran). At the cap the redraw is
    /// remembered instead; a live-resize frame (P26) bypasses the cap.
    pub(crate) fn begin(&mut self, resize_sync: bool) -> bool {
        if self.defer(resize_sync, false) {
            return false;
        }
        self.in_flight += 1;
        true
    }

    /// Whether this `RedrawRequested` must wait; the redraw is remembered and the next
    /// completion requests it. Checked BEFORE the UI frame runs (08 §17.3 step 1), so a
    /// deferred redraw costs nothing. It waits at the cap, and also while the window's previous
    /// packet still sits unconsumed in the mailbox (`queued`): another one would only replace
    /// it unseen (latest-wins), so the UI would build frames as fast as it can while the render
    /// thread is busy with other windows. A live-resize frame never waits.
    pub(crate) fn defer(&mut self, resize_sync: bool, queued: bool) -> bool {
        let defer = !resize_sync && (self.in_flight >= MAX_IN_FLIGHT || queued);
        self.redraw_pending |= defer;
        defer
    }

    /// A packet completed (`FrameDone` or the inline return). Returns whether to request a
    /// redraw: a skipped frame is always retried (§5.6), and a deferred redraw runs now (also
    /// after a lost surface or device, so a redraw deferred behind those frames is not lost).
    /// An occluded window keeps its deferred redraw for the visibility event.
    pub(crate) fn done(&mut self, outcome: FrameOutcome) -> bool {
        self.in_flight = self.in_flight.saturating_sub(1);
        match outcome {
            FrameOutcome::Skipped => {
                self.redraw_pending = false;
                true
            }
            FrameOutcome::Presented => {
                self.presented = true;
                std::mem::take(&mut self.redraw_pending)
            }
            FrameOutcome::SurfaceLost | FrameOutcome::DeviceLost => std::mem::take(&mut self.redraw_pending),
            FrameOutcome::Occluded => false,
        }
    }

    /// A queued packet was displaced in the mailbox before rendering (08 §17.3 step 3).
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub(crate) fn replaced(&mut self) {
        self.in_flight = self.in_flight.saturating_sub(1);
    }

    /// Whether a frame of this window has been presented. Before that a live-resize wait
    /// (08 §17.4) has nothing on screen to keep in step, and the first frame (renderer and
    /// pipeline creation) may legitimately take longer than `RESIZE_SYNC_TIMEOUT`.
    pub(crate) fn has_presented(&self) -> bool {
        self.presented
    }

    /// User input reached this window: its next packet is input-driven.
    pub(crate) fn note_input(&mut self) {
        self.input = true;
    }

    /// Whether the packet being built is input-driven (clears the mark).
    pub(crate) fn take_input(&mut self) -> bool {
        std::mem::take(&mut self.input)
    }
}

/// How long an `Occluded` acquire that the window system never confirmed waits before one
/// probe frame (08 §5.4, §5.6).
pub(crate) const OCCLUDED_PROBE: std::time::Duration = std::time::Duration::from_millis(500);

/// A window's occluded state (08 §5.4) in both frame modes.
///
/// Two sources set it: the window system (`WindowEvent::Occluded(true)`, cleared by its
/// visibility events) and an `Occluded` acquire. On the render thread the acquire result
/// arrives late (`FrameDone`): it must not override a visibility event that came after its
/// packet was built, or the window stays blank with nothing left to wake it. And an acquire
/// can report `Occluded` for a window the window system calls visible (macOS: a
/// `CAMetalLayer` asked for a drawable off the main thread before the window's first
/// display), after which no visibility event ever comes — so that state is probed again after
/// [`OCCLUDED_PROBE`] instead of waiting forever (one acquire per probe, never a spin).
#[derive(Debug, Default)]
pub(crate) struct Occlusion {
    occluded: bool,
    /// The window system reported the window hidden: only its visibility events clear it.
    by_system: bool,
    /// The last frame id issued before the latest visibility event: an `Occluded` outcome of
    /// a packet up to this id is stale.
    shown_after: u64,
    probe_at: Option<web_time::Instant>,
}

impl Occlusion {
    pub(crate) fn is_occluded(&self) -> bool {
        self.occluded
    }

    /// `WindowEvent::Occluded(occluded)`; `last_frame_id` is the window's latest packet id.
    /// Returns whether to request a redraw.
    pub(crate) fn system(&mut self, occluded: bool, last_frame_id: u64) -> bool {
        if occluded {
            self.occluded = true;
            self.by_system = true;
            self.probe_at = None;
            false
        } else {
            self.shown(last_frame_id);
            true
        }
    }

    /// A visibility event (`Occluded(false)`, `Focused(true)`, a non-zero `Resized`,
    /// `ScaleFactorChanged`): visible now, whatever older packets report.
    pub(crate) fn shown(&mut self, last_frame_id: u64) {
        self.occluded = false;
        self.by_system = false;
        self.probe_at = None;
        self.shown_after = self.shown_after.max(last_frame_id);
    }

    /// Packet `frame_id` reported an `Occluded` acquire at `now`. Returns whether it applied
    /// (`false`: the packet predates a visibility event; the caller requests a redraw).
    pub(crate) fn acquire_occluded(&mut self, frame_id: u64, now: web_time::Instant) -> bool {
        if frame_id <= self.shown_after {
            return false;
        }
        self.occluded = true;
        if !self.by_system {
            self.probe_at = Some(now + OCCLUDED_PROBE);
        }
        true
    }

    /// When a probe frame is due (an unconfirmed acquire occlusion), for the loop's wake-up.
    pub(crate) fn probe_at(&self) -> Option<web_time::Instant> {
        self.probe_at.filter(|_| self.occluded && !self.by_system)
    }

    /// At `now`, whether the probe is due; if so the window counts as visible again for one
    /// frame (the caller requests the redraw).
    pub(crate) fn probe_due(&mut self, now: web_time::Instant) -> bool {
        if self.probe_at().is_some_and(|t| t <= now) {
            self.occluded = false;
            self.probe_at = None;
            return true;
        }
        false
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
            input: false,
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

    /// The previous packet still waits in the mailbox: the next redraw builds nothing (it would
    /// only replace that packet unseen) and runs when the queued one completes.
    #[test]
    fn queued_packet_defers_redraw() {
        let mut pacer = Pacer::default();
        assert!(pacer.begin(false));
        assert!(pacer.defer(false, true), "one packet queued: no second UI frame");
        assert!(!pacer.defer(true, true), "a live-resize frame still goes");
        assert!(pacer.done(FrameOutcome::Presented), "the completion requests the deferred redraw");
        assert!(!pacer.defer(false, false));
    }

    /// A redraw deferred behind frames that come back lost is not dropped (the window would
    /// freeze until the next input).
    #[test]
    fn deferred_redraw_survives_loss() {
        for lost in [FrameOutcome::DeviceLost, FrameOutcome::SurfaceLost] {
            let mut pacer = Pacer::default();
            assert!(pacer.begin(false) && pacer.begin(false));
            assert!(pacer.defer(false, false));
            assert!(pacer.done(lost), "{lost:?}");
            assert!(!pacer.done(lost), "{lost:?}: exactly one redraw");
        }
    }

    #[test]
    fn has_presented_after_first_present() {
        let mut pacer = Pacer::default();
        assert!(pacer.begin(false));
        pacer.done(FrameOutcome::Skipped);
        assert!(!pacer.has_presented());
        assert!(pacer.begin(false));
        pacer.done(FrameOutcome::Presented);
        assert!(pacer.has_presented());
    }

    /// A late `FrameDone(Occluded)` for a packet built before a visibility event is stale.
    #[test]
    fn late_occluded_does_not_override_visibility() {
        let now = web_time::Instant::now();
        let mut o = Occlusion::default();
        assert!(o.system(false, 3), "Occluded(false) after packet 3 was built");
        assert!(!o.acquire_occluded(3, now), "packet 3's Occluded is stale");
        assert!(!o.is_occluded());
        assert!(o.acquire_occluded(4, now), "a packet built after it counts");
        assert!(o.is_occluded());
        o.shown(4);
        assert!(!o.is_occluded());
    }

    /// An `Occluded` acquire the window system never confirmed is probed again, once per
    /// `OCCLUDED_PROBE`; a window-system occlusion waits for its visibility event.
    #[test]
    fn unconfirmed_occlusion_is_probed() {
        let t0 = web_time::Instant::now();
        let mut o = Occlusion::default();
        assert!(o.acquire_occluded(1, t0));
        assert_eq!(o.probe_at(), Some(t0 + OCCLUDED_PROBE));
        assert!(!o.probe_due(t0), "no spin: nothing before the probe time");
        assert!(o.probe_due(t0 + OCCLUDED_PROBE));
        assert!(!o.is_occluded(), "one probe frame may run");
        assert!(!o.probe_due(t0 + OCCLUDED_PROBE * 2), "one probe per occluded acquire");

        let mut o = Occlusion::default();
        assert!(!o.system(true, 0));
        assert!(o.acquire_occluded(1, t0));
        assert_eq!(o.probe_at(), None, "the window system said hidden: wait for its event");
        assert!(!o.probe_due(t0 + OCCLUDED_PROBE * 10));
        assert!(o.system(false, 1));
        assert!(!o.is_occluded());
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
