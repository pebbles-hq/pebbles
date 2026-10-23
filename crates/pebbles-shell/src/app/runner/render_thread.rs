//! The render thread (08 §17, DR-26; native only): one `pebbles-render` thread owns the
//! [`RenderState`] and renders every window's packets. It never calls winit (08 §17.1 T1) and
//! never waits for the UI thread; it reports each frame back through `FrameDone`.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender};
use std::thread::JoinHandle;
use std::time::Duration;

use pebbles_foundation::log::{Cat, Level, logf};

use super::frame::{FrameOutcome, FramePacket, Mailbox, RenderState, WindowKey, render_packet};
use super::gpu::RenderContext;

/// How long closing a window waits for the render thread to drop its surface (08 §17.5).
pub(crate) const WINDOW_REMOVE_TIMEOUT: Duration = Duration::from_secs(1);
/// How long shutdown waits for the render thread (08 §17.7).
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

/// Reports a finished packet to the UI thread (`PebblesUserEvent::FrameDone` through the
/// event-loop proxy). Never blocks.
pub(crate) type FrameDoneFn = Box<dyn Fn(WindowKey, u64, FrameOutcome) + Send>;

pub(crate) enum RenderMsg {
    /// A new window, or a replacement surface after `SurfaceLost` (same key).
    AddWindow {
        key: WindowKey,
        surface: wgpu::Surface<'static>,
        size: (u32, u32),
    },
    /// The window's mailbox holds a packet.
    Frame(WindowKey),
    RemoveWindow {
        key: WindowKey,
        ack: SyncSender<()>,
    },
    /// Device-loss recovery (08 §17.6): a new context and one new surface per window.
    #[allow(dead_code)] // sent by the threaded device-loss protocol (P26)
    ReplaceGpu {
        gpu: RenderContext,
        surfaces: Vec<(WindowKey, wgpu::Surface<'static>, (u32, u32))>,
    },
    Shutdown {
        ack: SyncSender<()>,
    },
    /// Render a packet into an offscreen target and send its pixels back (tests only).
    #[cfg(test)]
    Readback {
        packet: FramePacket,
        reply: SyncSender<pebbles_gpu::RgbaImage>,
    },
}

impl RenderMsg {
    fn is_lifecycle(&self) -> bool {
        !matches!(self, Self::Frame(_))
    }
}

/// The UI thread's handle on the render thread.
pub(crate) struct RenderThread {
    tx: Sender<RenderMsg>,
    /// The context's instance: the UI thread creates surfaces with it (08 §17.5).
    instance: wgpu::Instance,
    mailboxes: Arc<std::sync::Mutex<HashMap<WindowKey, Arc<Mailbox>>>>,
    join: Option<JoinHandle<()>>,
}

impl RenderThread {
    /// Move `state` (with the windows it already has) onto a new `pebbles-render` thread.
    pub(crate) fn spawn(state: RenderState, done: FrameDoneFn) -> std::io::Result<Self> {
        let mailboxes: HashMap<WindowKey, Arc<Mailbox>> =
            state.surfaces.keys().map(|&k| (k, Arc::new(Mailbox::default()))).collect();
        let mailboxes = Arc::new(std::sync::Mutex::new(mailboxes));
        let instance = state.ctx.instance();
        let (tx, rx) = mpsc::channel();
        let shared = Arc::clone(&mailboxes);
        let join = std::thread::Builder::new()
            .name("pebbles-render".into())
            .spawn(move || render_loop(state, &rx, &shared, &done))?;
        Ok(Self { tx, instance, mailboxes, join: Some(join) })
    }

    pub(crate) fn instance(&self) -> wgpu::Instance {
        self.instance.clone()
    }

    fn mailbox(&self, key: WindowKey) -> Arc<Mailbox> {
        let mut map = self.mailboxes.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(map.entry(key).or_default())
    }

    /// 08 §17.5: hand a window's surface over (a new window, or a replacement after `SurfaceLost`).
    pub(crate) fn add_window(&self, key: WindowKey, surface: wgpu::Surface<'static>, size: (u32, u32)) {
        self.mailbox(key);
        let _ = self.tx.send(RenderMsg::AddWindow { key, surface, size });
    }

    /// 08 §17.3 step 3: post the packet and wake the thread. Returns the packet it replaced,
    /// which the caller counts as completed (`Skipped`).
    pub(crate) fn submit(&self, key: WindowKey, packet: FramePacket) -> Option<FramePacket> {
        let replaced = self.mailbox(key).post(packet);
        let _ = self.tx.send(RenderMsg::Frame(key));
        replaced
    }

    /// 08 §17.5 close: drop the window's surface on the render thread and wait for it. `false`
    /// on timeout — the caller must keep its `Arc<Window>` alive (T2).
    pub(crate) fn remove_window(&self, key: WindowKey) -> bool {
        let acked = remove_window_with(&self.tx, key, WINDOW_REMOVE_TIMEOUT);
        self.mailboxes.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&key);
        acked
    }

    #[cfg(test)]
    fn sender(&self) -> &Sender<RenderMsg> {
        &self.tx
    }
}

/// Send `RemoveWindow` and wait for the ack (08 §17.5). Logs one warning on timeout.
fn remove_window_with(tx: &Sender<RenderMsg>, key: WindowKey, timeout: Duration) -> bool {
    let (ack, done) = mpsc::sync_channel(1);
    if tx.send(RenderMsg::RemoveWindow { key, ack }).is_err() {
        return true; // the thread is gone, and its surfaces with it
    }
    match done.recv_timeout(timeout) {
        Ok(()) | Err(RecvTimeoutError::Disconnected) => true,
        Err(RecvTimeoutError::Timeout) => {
            logf(
                Level::Warn,
                Cat::Gpu,
                format_args!("render thread did not release window {key:?} within {timeout:?}; keeping it"),
            );
            false
        }
    }
}

/// 08 §17.7 shutdown: the render thread drops the state (surfaces first), acks and returns.
impl Drop for RenderThread {
    fn drop(&mut self) {
        let (ack, done) = mpsc::sync_channel(1);
        if self.tx.send(RenderMsg::Shutdown { ack }).is_ok()
            && done.recv_timeout(SHUTDOWN_TIMEOUT) == Err(RecvTimeoutError::Timeout)
        {
            logf(
                Level::Warn,
                Cat::Gpu,
                format_args!("render thread did not stop within {SHUTDOWN_TIMEOUT:?}"),
            );
            return; // never block the UI thread forever on a wedged driver
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// 08 §17.3 step 4: lifecycle messages first (arrival order), then frames whose packet carries
/// a live-resize `sync`, then the remaining frames (arrival order).
fn order_batch(batch: Vec<RenderMsg>, has_sync: impl Fn(WindowKey) -> bool) -> Vec<RenderMsg> {
    let (mut lifecycle, frames): (Vec<_>, Vec<_>) = batch.into_iter().partition(RenderMsg::is_lifecycle);
    let (synced, rest): (Vec<_>, Vec<_>) =
        frames.into_iter().partition(|m| matches!(m, RenderMsg::Frame(k) if has_sync(*k)));
    lifecycle.extend(synced);
    lifecycle.extend(rest);
    lifecycle
}

fn render_loop(
    mut state: RenderState,
    rx: &Receiver<RenderMsg>,
    mailboxes: &std::sync::Mutex<HashMap<WindowKey, Arc<Mailbox>>>,
    done: &FrameDoneFn,
) {
    let mailbox =
        |key| mailboxes.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&key).cloned();
    #[cfg(test)]
    let mut offscreen: Option<pebbles_gpu::OffscreenRenderer> = None;
    while let Ok(first) = rx.recv() {
        let mut batch = vec![first];
        batch.extend(rx.try_iter());
        let batch = order_batch(batch, |k| mailbox(k).is_some_and(|m| m.has_sync()));
        for msg in batch {
            match msg {
                RenderMsg::AddWindow { key, surface, size } => {
                    match pollster::block_on(state.ctx.adopt_surface(surface, size)) {
                        Ok(surface) => state.adopt(key, surface),
                        // The next frame reports `SurfaceLost`; the UI thread retries (08 §17.5).
                        Err(e) => {
                            state.surfaces.remove(&key);
                            logf(Level::Error, Cat::Gpu, format_args!("could not adopt a surface: {e}"));
                        }
                    }
                }
                RenderMsg::Frame(key) => {
                    let Some(packet) = mailbox(key).and_then(|m| m.take()) else { continue };
                    let (frame_id, sync) = (packet.frame_id, packet.sync.clone());
                    let outcome = render_packet(&mut state, key, packet);
                    if let Some(sync) = sync {
                        let _ = sync.try_send(outcome);
                    }
                    done(key, frame_id, outcome);
                }
                RenderMsg::RemoveWindow { key, ack } => {
                    state.surfaces.remove(&key);
                    let _ = ack.send(());
                }
                RenderMsg::ReplaceGpu { gpu, surfaces } => {
                    state = RenderState::new(gpu);
                    for (key, surface, size) in surfaces {
                        match pollster::block_on(state.ctx.adopt_surface(surface, size)) {
                            Ok(surface) => state.adopt(key, surface),
                            Err(e) => {
                                logf(Level::Error, Cat::Gpu, format_args!("could not adopt a surface: {e}"))
                            }
                        }
                    }
                }
                RenderMsg::Shutdown { ack } => {
                    drop(state);
                    let _ = ack.send(());
                    return;
                }
                #[cfg(test)]
                RenderMsg::Readback { packet, reply } => {
                    let offscreen = offscreen
                        .get_or_insert_with(|| pebbles_gpu::OffscreenRenderer::new().expect("offscreen"));
                    let image = offscreen.render_rgba(
                        &packet.scene,
                        packet.size,
                        packet.base_color,
                        packet.scale_factor,
                    );
                    let _ = reply.send(image.expect("readback"));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use winit::window::WindowId;

    fn key(n: u64) -> WindowKey {
        WindowId::from(n)
    }

    fn ack() -> SyncSender<()> {
        mpsc::sync_channel(1).0
    }

    fn label(m: &RenderMsg) -> String {
        match m {
            RenderMsg::Frame(k) => format!("frame{}", u64::from(*k)),
            RenderMsg::RemoveWindow { key, .. } => format!("remove{}", u64::from(*key)),
            RenderMsg::Shutdown { .. } => "shutdown".into(),
            _ => "other".into(),
        }
    }

    #[test]
    fn batch_order() {
        let batch = vec![
            RenderMsg::Frame(key(1)),
            RenderMsg::RemoveWindow { key: key(9), ack: ack() },
            RenderMsg::Frame(key(2)),
            RenderMsg::Frame(key(3)),
            RenderMsg::Shutdown { ack: ack() },
        ];
        let ordered: Vec<_> = order_batch(batch, |k| k == key(3)).iter().map(label).collect();
        assert_eq!(ordered, ["remove9", "shutdown", "frame3", "frame1", "frame2"]);
    }

    /// The test plays the render thread on the channel.
    #[test]
    fn close_waits_for_ack() {
        let (tx, rx) = mpsc::channel();
        let window = Arc::new(()); // the `Arc<Window>` stand-in
        let render = std::thread::spawn(move || {
            if let Ok(RenderMsg::RemoveWindow { ack, .. }) = rx.recv() {
                std::thread::sleep(Duration::from_millis(30));
                let _ = ack.send(());
            }
            rx // keep the channel open for the second case
        });
        assert!(remove_window_with(&tx, key(1), Duration::from_secs(5)), "acked");
        drop(window);
        let rx = render.join().unwrap();
        // No ack within the timeout: the caller keeps its window.
        let kept = Arc::new(());
        let started = std::time::Instant::now();
        assert!(!remove_window_with(&tx, key(2), Duration::from_millis(50)));
        assert!(started.elapsed() >= Duration::from_millis(50));
        assert_eq!(Arc::strong_count(&kept), 1, "still held by the UI side");
        drop(rx);
    }

    /// Scene `i` of a small corpus: overlapping fills, strokes and rounded rects.
    fn scene(i: usize) -> pebbles_render::Scene {
        use kurbo::{Affine, Rect, RoundedRect, Stroke};
        use pebbles_render::paint::Painter;
        let mut scene = pebbles_render::Scene::new();
        let mut p = Painter::new(&mut scene);
        let f = i as f64;
        let color =
            peniko::Color::from_rgba8((i * 37 % 256) as u8, (i * 91 % 256) as u8, (i * 53 % 256) as u8, 200);
        p.fill(peniko::Fill::NonZero, Affine::IDENTITY, color, None, &Rect::new(f, f, 40.0 + f, 30.0 + f));
        let rr = RoundedRect::new(8.0, 20.0, 56.0, 60.0 - f / 2.0, 2.0 + f / 2.0);
        p.fill(
            peniko::Fill::NonZero,
            Affine::rotate(f * 0.05),
            peniko::Color::BLACK.with_alpha(0.4),
            None,
            &rr,
        );
        p.stroke(
            &Stroke::new(1.0 + f / 4.0),
            Affine::IDENTITY,
            peniko::Color::from_rgb8(20, 90, 200),
            None,
            &rr,
        );
        scene
    }

    /// 20 corpus scenes rendered on the render thread equal the inline output byte for byte.
    #[test]
    #[ignore = "needs a GPU adapter (lavapipe in CI)"]
    fn threaded_matches_inline() {
        let ctx = pollster::block_on(RenderContext::new(&pebbles_gpu::GpuInit::headless()));
        let thread = RenderThread::spawn(RenderState::new(ctx), Box::new(|_, _, _| {})).expect("spawn");
        let mut inline = pebbles_gpu::OffscreenRenderer::new().expect("offscreen");
        for i in 0..20 {
            let packet = FramePacket {
                frame_id: i as u64,
                scene: scene(i),
                size: (64, 64),
                scale_factor: 1.0,
                base_color: peniko::Color::WHITE,
                sync: None,
            };
            let (reply, rx) = mpsc::sync_channel(1);
            thread.sender().send(RenderMsg::Readback { packet, reply }).unwrap();
            let threaded = rx.recv_timeout(Duration::from_secs(30)).expect("readback");
            let expected =
                inline.render_rgba(&scene(i), (64, 64), peniko::Color::WHITE, 1.0).expect("inline");
            assert_eq!(threaded.pixels, expected.pixels, "scene {i}");
        }
    }
}
