//! Per-target textures (10 §4): the offscreen layer pool of ONE render target, owned by its
//! `RenderSurface` / offscreen renderer — never by the per-device `Renderer` — so windows of
//! different sizes never thrash and a closed window frees its targets.

/// Target-sized textures of one render target. The main pass is single-sampled (R17): the
/// target owns only its pool of offscreen Tier C / layer targets (09 §3.8).
#[derive(Default)]
pub struct FrameTargets {
    pub(crate) pool: crate::execute::resources::layer_pool::LayerPool,
    /// 09 §4.4: the root renders here (then is blitted) when a backdrop composite must copy
    /// from it; dropped after `TARGET_IDLE_FRAMES` unused frames.
    root_color: Option<RootColor>,
    /// M30 (DR-25): damage tracking for this target — `Some` when enabled. The root then always
    /// renders into `root_color`, which persists between frames, and partial frames redraw only
    /// the damage (into `scratch`, copied into `root_color`).
    damage: Option<crate::prepare::damage::DamageTracker>,
    /// Bumped whenever `root_color` is (re)created: its content is undefined (a full redraw).
    root_generation: u64,
    /// M30: the cleared texture a partial frame renders its damage into.
    scratch: Option<RootColor>,
    /// Depth textures for depth-requesting custom renderers (R62), created on first use: one
    /// per attachment key (the root, or a layer target hosting a custom op).
    depth: Vec<DepthTarget>,
    /// Minted by the Renderer on first render (its `TargetBook`, 10 §4.4 E4).
    id: Option<u64>,
    /// The book's retire list: dropping these targets hands `id` back so the book forgets them at
    /// once (an idle app renders no frames, so the stale-frame sweep alone would never run).
    retire: Option<Retired>,
}

/// Ids of dropped `FrameTargets`, drained by their `TargetBook`.
type Retired = std::sync::Arc<std::sync::Mutex<Vec<u64>>>;

impl Drop for FrameTargets {
    fn drop(&mut self) {
        if let (Some(id), Some(retire)) = (self.id, &self.retire)
            && let Ok(mut ids) = retire.lock()
        {
            ids.push(id);
        }
    }
}

/// What a target-sized attachment must match (10 §4.1): its size, color format and sample
/// count — always from the target texture, never from the window (DR-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TargetKey {
    pub width: u32,
    pub height: u32,
    pub format: wgpu::TextureFormat,
    pub sample_count: u32,
}

/// Bytes per sample used for budgeting (exact for the formats we create, 10 §4.1).
pub(crate) fn bytes_per_sample(format: wgpu::TextureFormat) -> u64 {
    match format {
        wgpu::TextureFormat::Depth32Float => 4,
        other => u64::from(other.block_copy_size(None).unwrap_or(4)),
    }
}

/// Frames a target may go unrendered before the Renderer's book forgets it (10 §4.4 E4).
pub(crate) const TARGET_STALE_FRAMES: u64 = 300;

/// Per-Renderer accounting of the targets it renders (10 §4.1, M13). It owns no textures: the
/// textures are freed when their `FrameTargets` drops.
#[derive(Debug)]
pub(crate) struct TargetBook {
    /// FrameTargets id → (bytes, last frame rendered).
    entries: std::collections::HashMap<u64, (u64, u64)>,
    /// Ids handed back by dropped `FrameTargets`, not yet removed from `entries`.
    retired: Retired,
    next_id: u64,
    total_bytes: u64,
    budget_bytes: u64,
    warned: bool,
}

impl TargetBook {
    pub(crate) fn new(budget_bytes: u64) -> Self {
        Self {
            entries: Default::default(),
            retired: Retired::default(),
            next_id: 0,
            total_bytes: 0,
            budget_bytes,
            warned: false,
        }
    }

    /// E4/E5: record `targets` as rendered on `frame`; true when the book is over budget (logged
    /// once; creation is never refused).
    pub(crate) fn record(&mut self, targets: &mut FrameTargets, frame: u64) -> bool {
        self.reap();
        if targets.id.is_none() {
            targets.retire = Some(self.retired.clone());
        }
        let id = *targets.id.get_or_insert_with(|| {
            self.next_id += 1;
            self.next_id
        });
        let bytes = targets.frame_target_bytes();
        let old = self.entries.insert(id, (bytes, frame)).map_or(0, |(b, _)| b);
        self.total_bytes = self.total_bytes - old + bytes;
        let over = self.total_bytes > self.budget_bytes;
        if over && !std::mem::replace(&mut self.warned, true) {
            pebbles_foundation::log::logf(
                pebbles_foundation::log::Level::Warn,
                pebbles_foundation::log::Cat::Gpu,
                format_args!(
                    "frame targets hold {} B, over the {} B budget (10 §4.4)",
                    self.total_bytes, self.budget_bytes
                ),
            );
        }
        over
    }

    /// E4: forget targets not rendered for `TARGET_STALE_FRAMES` frames.
    pub(crate) fn end_frame(&mut self, frame: u64) {
        self.reap();
        let mut freed = 0;
        self.entries.retain(|_, (bytes, last)| {
            let keep = frame.saturating_sub(*last) < TARGET_STALE_FRAMES;
            if !keep {
                freed += *bytes;
            }
            keep
        });
        self.total_bytes -= freed;
    }

    /// Forget the targets dropped since the last call.
    fn reap(&mut self) {
        let ids = self.retired.lock().map(|mut ids| std::mem::take(&mut *ids)).unwrap_or_default();
        for id in ids {
            if let Some((bytes, _)) = self.entries.remove(&id) {
                self.total_bytes -= bytes;
            }
        }
    }

    /// Booked targets still alive (census): dropped ones count as gone before the next reap.
    #[cfg(debug_assertions)]
    pub(crate) fn len(&self) -> usize {
        let pending = self
            .retired
            .lock()
            .map_or(0, |ids| ids.iter().filter(|id| self.entries.contains_key(id)).count());
        self.entries.len() - pending
    }

    /// Booked bytes of live targets (census, 10 §9.3).
    pub(crate) fn total_bytes(&self) -> u64 {
        let pending: u64 = self
            .retired
            .lock()
            .map_or(0, |ids| ids.iter().filter_map(|id| self.entries.get(id)).map(|(b, _)| b).sum());
        self.total_bytes - pending
    }
}

struct DepthTarget {
    key: TargetKey,
    format: wgpu::TextureFormat,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    /// Frames since last use (dropped after `TARGET_IDLE_FRAMES`).
    idle: u32,
}

/// Frames a target-sized intermediate may sit unused before it is dropped (09 §4.4).
pub(crate) const TARGET_IDLE_FRAMES: u32 = 600;

pub(crate) struct RootColor {
    pub texture: wgpu::Texture,
    pub view: wgpu::TextureView,
    idle: u32,
    /// The blit's bind group over `view`, built once (a per-frame one would break B5).
    pub blit_group: Option<wgpu::BindGroup>,
}

/// `PEBBLES_GPU_DAMAGE=0` turns damage tracking off everywhere (M30), read once.
fn damage_allowed() -> bool {
    static ALLOWED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ALLOWED.get_or_init(|| std::env::var("PEBBLES_GPU_DAMAGE").map_or(true, |v| v.trim() != "0"))
}

/// A single-sample color target the root can render into, copy from and blit from.
fn color_target(
    device: &wgpu::Device,
    size: (u32, u32),
    format: wgpu::TextureFormat,
    label: &'static str,
    stats: &mut crate::stats::FrameStats,
) -> Option<RootColor> {
    let d = wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d { width: size.0, height: size.1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    // 11 §2.6: on a second OOM the frame renders without the intermediate.
    let Some(texture) = crate::execute::alloc::create_with_retry(device, || device.create_texture(&d), || {})
    else {
        stats.record(crate::stats::Unsupported::GpuOom);
        return None;
    };
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    stats.textures_created += 1;
    stats.texture_views_created += 1;
    Some(RootColor { texture, view, idle: 0, blit_group: None })
}

impl std::fmt::Debug for FrameTargets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameTargets").field("pool_bytes", &self.pool.bytes()).finish_non_exhaustive()
    }
}

impl FrameTargets {
    /// Empty targets; textures are created on first use.
    pub fn new() -> Self {
        Self::default()
    }

    /// Census fields owned by one render target (M05).
    #[cfg(debug_assertions)]
    #[cfg_attr(target_family = "wasm", allow(dead_code))] // the offscreen renderer is native-only
    pub(crate) fn census_into(&self, c: &mut crate::census::GpuCensus) {
        // The `frame_target_*` fields come from the Renderer's book (M13).
        c.layer_pool_textures += self.pool.len();
        c.layer_pool_bytes += self.pool.bytes();
    }

    /// Pooled offscreen layer / Tier C targets currently held (census, 09 §3.8).
    pub fn layer_pool_targets(&self) -> usize {
        self.pool.len()
    }

    /// Bytes of GPU textures this target owns beyond its own color texture (census, 10 §9).
    pub fn frame_target_bytes(&self) -> u64 {
        self.pool.bytes() + self.root_color_bytes() + self.depth_bytes()
    }

    /// Depth textures held for custom renderers (census, R62).
    pub fn depth_targets(&self) -> usize {
        self.depth.len()
    }

    fn depth_bytes(&self) -> u64 {
        self.depth
            .iter()
            .map(|d| {
                let t = &d.texture;
                u64::from(t.width())
                    * u64::from(t.height())
                    * bytes_per_sample(d.format)
                    * u64::from(t.sample_count())
            })
            .sum()
    }

    /// Create the depth texture of `key` unless it exists (R62); marks it used this frame.
    pub(crate) fn ensure_depth(
        &mut self,
        device: &wgpu::Device,
        key: TargetKey,
        format: wgpu::TextureFormat,
        stats: &mut crate::stats::FrameStats,
    ) {
        if let Some(d) = self.depth.iter_mut().find(|d| d.key == key && d.format == format) {
            d.idle = 0;
            return;
        }
        let d = wgpu::TextureDescriptor {
            label: Some("pebbles.gpu.target.depth"),
            size: wgpu::Extent3d { width: key.width, height: key.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: key.sample_count,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        };
        // 11 §2.6: unused depth targets are the eviction; a second OOM leaves the custom pass
        // without depth (`CustomRender::depth` is `None`).
        let depth = &mut self.depth;
        let Some(texture) = crate::execute::alloc::create_with_retry(
            device,
            || device.create_texture(&d),
            || depth.retain(|t| t.idle == 0),
        ) else {
            stats.record(crate::stats::Unsupported::GpuOom);
            return;
        };
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        stats.textures_created += 1;
        stats.texture_views_created += 1;
        self.depth.push(DepthTarget { key, format, texture, view, idle: 0 });
    }

    pub(crate) fn depth_view(
        &self,
        key: TargetKey,
        format: wgpu::TextureFormat,
    ) -> Option<&wgpu::TextureView> {
        self.depth.iter().find(|d| d.key == key && d.format == format).map(|d| &d.view)
    }

    fn root_color_bytes(&self) -> u64 {
        [&self.root_color, &self.scratch]
            .into_iter()
            .flatten()
            .map(|r| {
                let t = &r.texture;
                u64::from(t.width())
                    * u64::from(t.height())
                    * u64::from(t.format().block_copy_size(None).unwrap_or(4))
            })
            .sum()
    }

    /// The root intermediate of `size` / `format` (created or recreated on demand).
    pub(crate) fn root_color(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        stats: &mut crate::stats::FrameStats,
    ) -> Option<&mut RootColor> {
        let fresh = self
            .root_color
            .as_ref()
            .is_some_and(|r| (r.texture.width(), r.texture.height()) == size && r.texture.format() == format);
        if !fresh {
            self.root_color = None;
            self.root_color = color_target(device, size, format, "pebbles.gpu.root_color", stats);
            self.root_generation += 1;
        }
        let r = self.root_color.as_mut()?;
        r.idle = 0;
        Some(r)
    }

    /// M30: the scratch target of a partial frame, `size` / `format` like the root.
    pub(crate) fn damage_scratch(
        &mut self,
        device: &wgpu::Device,
        size: (u32, u32),
        format: wgpu::TextureFormat,
        stats: &mut crate::stats::FrameStats,
    ) -> Option<&RootColor> {
        let fresh = self
            .scratch
            .as_ref()
            .is_some_and(|r| (r.texture.width(), r.texture.height()) == size && r.texture.format() == format);
        if !fresh {
            self.scratch = None;
            self.scratch = color_target(device, size, format, "pebbles.gpu.damage_scratch", stats);
        }
        let r = self.scratch.as_mut()?;
        r.idle = 0;
        Some(r)
    }

    /// Turn on damage tracking for this target (M30, DR-25): window surfaces redraw only what
    /// changed. `PEBBLES_GPU_DAMAGE=0` keeps it off.
    #[must_use]
    pub fn with_damage_tracking(mut self) -> Self {
        if damage_allowed() {
            self.damage = Some(crate::prepare::damage::DamageTracker::default());
        }
        self
    }

    /// Whether this target tracks damage.
    pub(crate) fn tracks_damage(&self) -> bool {
        self.damage.is_some()
    }

    /// The generation of the persistent root (bumped on every (re)creation).
    pub(crate) fn root_generation(&self) -> u64 {
        self.root_generation
    }

    /// M30: this frame's damage against the previous frame of this target.
    pub(crate) fn damage_frame(
        &mut self,
        scene: &pebbles_render::paint::Scene,
        key: crate::prepare::damage::FrameKey,
    ) -> crate::prepare::damage::Damage {
        match &mut self.damage {
            Some(t) => t.frame(scene, key),
            None => crate::prepare::damage::Damage::Full,
        }
    }

    /// The blit's bind group over the root intermediate, built once per root texture.
    pub(crate) fn root_blit_group(
        &mut self,
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        stats: &mut crate::stats::FrameStats,
    ) -> Option<wgpu::BindGroup> {
        let r = self.root_color.as_mut()?;
        let group = r.blit_group.get_or_insert_with(|| {
            stats.bind_groups_created += 1;
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("pebbles.gpu.bind.blit"),
                layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&r.view),
                }],
            })
        });
        Some(group.clone())
    }

    /// Forget the previous frame (the persistent root may be stale): the next frame is full.
    pub(crate) fn reset_damage(&mut self) {
        if let Some(t) = &mut self.damage {
            t.reset();
        }
    }

    /// End of frame: age the root intermediate (09 §4.4).
    pub(crate) fn end_frame_root(&mut self, used: bool) {
        for d in &mut self.depth {
            d.idle += 1;
        }
        self.depth.retain(|d| d.idle < TARGET_IDLE_FRAMES);
        if let Some(r) = &mut self.root_color
            && !used
        {
            r.idle += 1;
            if r.idle >= TARGET_IDLE_FRAMES {
                self.root_color = None;
            }
        }
        if let Some(r) = &mut self.scratch {
            r.idle += 1;
            if r.idle >= TARGET_IDLE_FRAMES {
                self.scratch = None;
            }
        }
    }
}

/// Where one frame renders: a view, its format, and the target's own textures.
pub struct RenderTarget<'a> {
    pub(crate) view: &'a wgpu::TextureView,
    format: wgpu::TextureFormat,
    size: (u32, u32),
    pub(crate) targets: &'a mut FrameTargets,
    /// GL sRGB-only surface: shaders decode sRGB before the hardware re-encodes (02 §3.8, R13).
    pub(crate) decode_srgb_output: bool,
}

impl<'a> RenderTarget<'a> {
    /// `format` is the VIEW format (it may differ from the texture format on the sRGB-view path).
    /// The size is read from `view.texture()` (DR-12: size from the target, never from a window).
    pub fn new(
        view: &'a wgpu::TextureView,
        format: wgpu::TextureFormat,
        targets: &'a mut FrameTargets,
    ) -> Self {
        let texture = view.texture();
        Self { view, format, size: (texture.width(), texture.height()), targets, decode_srgb_output: false }
    }

    /// Mark the target as a GL sRGB-only surface (02 §3.8; from
    /// [`RenderSurface::decode_srgb_output`](crate::RenderSurface::decode_srgb_output)). Never set
    /// it for offscreen Unorm targets.
    #[must_use]
    pub fn with_decode_srgb_output(mut self, on: bool) -> Self {
        self.decode_srgb_output = on;
        self
    }

    /// Size in device pixels, from the target texture.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// The view format the renderer draws in.
    pub fn format(&self) -> wgpu::TextureFormat {
        self.format
    }
}

/// Per-frame parameters.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct FrameParams {
    /// Clear color, sRGB-encoded (premultiplied by `prepare::color::clear_color`).
    pub base_color: peniko::Color,
    /// Applied to the whole scene (DPI scale in the end state; identity in the transition).
    pub root_transform: kurbo::Affine,
    /// The target's device-pixel ratio (02 §7.8 hinting policy; R27). Default 1.0.
    pub scale_factor: f64,
    /// Force glyph hinting on/off for experiments (render-check `--glyph-hint`); `None` = the
    /// 02 §7.8 policy.
    pub glyph_hint: Option<bool>,
}

impl FrameParams {
    /// Clear to `base_color`, identity root transform.
    pub fn new(base_color: peniko::Color) -> Self {
        Self { base_color, root_transform: kurbo::Affine::IDENTITY, scale_factor: 1.0, glyph_hint: None }
    }

    /// Replace the root transform.
    #[must_use]
    pub fn with_root_transform(mut self, root_transform: kurbo::Affine) -> Self {
        self.root_transform = root_transform;
        self
    }

    /// Set the device-pixel ratio (R27).
    #[must_use]
    pub fn with_scale_factor(mut self, scale_factor: f64) -> Self {
        self.scale_factor = scale_factor;
        self
    }

    /// Force glyph hinting on or off; `None` restores the policy (R27 experiments).
    #[must_use]
    pub fn with_glyph_hint(mut self, hint: Option<bool>) -> Self {
        self.glyph_hint = hint;
        self
    }
}

/// 4 when the color format supports 4× + resolve and the depth format (if the target
/// has one, R62) supports 4×; otherwise 1. Never 8 (DR-06; 02 §5.4).
pub(crate) fn negotiate_sample_count(
    color: wgpu::TextureFormatFeatureFlags,
    depth: Option<wgpu::TextureFormatFeatureFlags>,
) -> u32 {
    let color_ok = color.sample_count_supported(4)
        && color.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE);
    let depth_ok = depth.is_none_or(|f| f.sample_count_supported(4));
    if color_ok && depth_ok { 4 } else { 1 }
}

/// The negotiated count for `color` (+ optional `depth`) on `adapter` (02 §5.4).
pub(crate) fn sample_count_for(
    adapter: &wgpu::Adapter,
    color: wgpu::TextureFormat,
    depth: Option<wgpu::TextureFormat>,
) -> u32 {
    negotiate_sample_count(
        adapter.get_texture_format_features(color).flags,
        depth.map(|f| adapter.get_texture_format_features(f).flags),
    )
}

/// Per-device cache of [`sample_count_for`] decisions keyed by `(color, depth)` (10 §4.3), so
/// Tier C runs, GPU clip masks (`R8Unorm`, L09) and depth targets (R62) share one cache.
#[derive(Debug, Default)]
pub(crate) struct MsaaCache {
    decided: std::collections::HashMap<(wgpu::TextureFormat, Option<wgpu::TextureFormat>), u32>,
}

impl MsaaCache {
    /// The sample count for `(color, depth)`, logged on the first decision per pair.
    pub(crate) fn get(
        &mut self,
        adapter: &wgpu::Adapter,
        color: wgpu::TextureFormat,
        depth: Option<wgpu::TextureFormat>,
    ) -> u32 {
        *self.decided.entry((color, depth)).or_insert_with(|| {
            let n = sample_count_for(adapter, color, depth);
            log_decision(adapter, color, depth, n);
            n
        })
    }
}

fn log_decision(
    adapter: &wgpu::Adapter,
    color: wgpu::TextureFormat,
    depth: Option<wgpu::TextureFormat>,
    n: u32,
) {
    use pebbles_foundation::log::{Cat, Level, logf};
    if n == 1 {
        logf(
            Level::Warn,
            Cat::Gpu,
            format_args!("MSAA unavailable for {color:?}; rendering without multisampling"),
        );
        return;
    }
    let flags = adapter.get_texture_format_features(color).flags;
    let a = flags.sample_count_supported(4)
        && flags.contains(wgpu::TextureFormatFeatureFlags::MULTISAMPLE_RESOLVE);
    let b = depth.is_none_or(|d| adapter.get_texture_format_features(d).flags.sample_count_supported(4));
    logf(
        Level::Info,
        Cat::Gpu,
        format_args!("MSAA: {color:?} → {n}× (color 4×+resolve: {a}, depth 4×: {b})"),
    );
}

#[cfg(test)]
mod tests {
    use super::negotiate_sample_count;
    use wgpu::TextureFormatFeatureFlags as F;

    #[test]
    fn x4_with_resolve_and_depth() {
        assert_eq!(
            negotiate_sample_count(F::MULTISAMPLE_X4 | F::MULTISAMPLE_RESOLVE, Some(F::MULTISAMPLE_X4)),
            4
        );
    }

    #[test]
    fn x4_without_resolve() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X4, None), 1);
    }

    #[test]
    fn depth_without_x4() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X4 | F::MULTISAMPLE_RESOLVE, Some(F::empty())), 1);
    }

    #[test]
    fn no_depth() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X4 | F::MULTISAMPLE_RESOLVE, None), 4);
    }

    #[test]
    fn only_x8() {
        assert_eq!(negotiate_sample_count(F::MULTISAMPLE_X8 | F::MULTISAMPLE_RESOLVE, None), 1);
    }

    /// S16: a closed window's targets leave the book when they drop, not 300 frames later.
    #[test]
    #[cfg(debug_assertions)]
    fn dropped_targets_leave_the_book_at_once() {
        let mut book = super::TargetBook::new(u64::MAX);
        let mut main = super::FrameTargets::new();
        let mut windows: Vec<_> = (0..10).map(|_| super::FrameTargets::new()).collect();
        book.record(&mut main, 1);
        for w in &mut windows {
            book.record(w, 1);
        }
        assert_eq!(book.len(), 11);
        drop(windows);
        assert_eq!(book.len(), 1, "census counts only live targets, before any reap");
        book.end_frame(2);
        assert_eq!((book.len(), book.entries.len()), (1, 1));
        book.record(&mut main, 3);
        assert_eq!(book.len(), 1);
    }
}
