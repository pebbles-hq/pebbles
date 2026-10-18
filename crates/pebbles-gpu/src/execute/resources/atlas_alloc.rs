//! The coverage atlas, CPU side (10 §5.3–§5.6, §5.10; M01) — no wgpu types. Glyph and Tier B path
//! masks share the mask pages and one LRU; color glyphs live in the color pages. Allocation runs
//! the exact §5.5 ladder: try → evict stale → grow side → add layer → evict unpinned → overflow.

use std::collections::HashMap;

use guillotiere::{AllocId, AtlasAllocator as Guillotine, size2};

use crate::stats::FrameStats;

/// Padding around every entry, in atlas px (10 §5.5).
pub(crate) const GLYPH_GUTTER_PX: u32 = 1;
/// Initial page sides (10 §5.4).
pub(crate) const GLYPH_MASK_INITIAL_SIDE: u32 = 1024;
pub(crate) const GLYPH_COLOR_INITIAL_SIDE: u32 = 512;
/// Ladder step 2: entries unused for this many frames are evicted before the atlas grows.
pub(crate) const GLYPH_LADDER_STALE_FRAMES: u64 = 120;
/// §5.10 periodic sweep.
pub(crate) const ATLAS_SWEEP_EVERY_FRAMES: u64 = 256;
pub(crate) const ATLAS_SWEEP_STALE_FRAMES: u64 = 3600;

/// Glyph atlas key (DR-07). Exact, collision-free fields only — no pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GlyphKey {
    /// `peniko::Blob::id()` of the font bytes (process-unique, never reused).
    pub font_blob: u64,
    /// Face index within a collection (`FontData::index`).
    pub font_index: u32,
    /// swash glyph id; display-list ids above `u16::MAX` never get a key.
    pub glyph_id: u16,
    /// Raster size in 1/64 px.
    pub size_q: u32,
    /// Quarter-pixel bin of the pen x, 0..=3.
    pub subpx_x: u8,
    /// Vertical hinting (02 §7.8); false until R27.
    pub hint: bool,
    /// 0 when the run has no normalized coords, else a 64-bit hash of them.
    pub coords_hash: u64,
}

/// Tier B path-mask key (02 R15). Exact, collision-free fields only — no pointers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MaskKey {
    /// Hash of the `Geometry` (every coordinate as `to_bits`).
    pub geometry_hash: u64,
    /// The device transform's 2×2 part × 1024, rounded.
    pub linear_q: [i32; 4],
    /// Fractional device translation in 1/4096 px (x, y), 0..=4095 each.
    pub subpx: (u16, u16),
    /// 0 = non-zero, 1 = even-odd.
    pub rule: u8,
    /// Hash of width / joins / caps / dashes; 0 for fills.
    pub stroke_hash: u64,
}

/// A coverage-atlas key (10 §5.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CoverageKey {
    Glyph(GlyphKey),
    Path(MaskKey),
}

impl From<GlyphKey> for CoverageKey {
    fn from(k: GlyphKey) -> Self {
        Self::Glyph(k)
    }
}

impl From<MaskKey> for CoverageKey {
    fn from(k: MaskKey) -> Self {
        Self::Path(k)
    }
}

/// What an entry holds, for per-kind byte budgets (10 §5, §8.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MaskKind {
    Glyph,
    Path,
}

impl CoverageKey {
    pub(crate) fn kind(&self) -> MaskKind {
        match self {
            Self::Glyph(_) => MaskKind::Glyph,
            Self::Path(_) => MaskKind::Path,
        }
    }
}

/// Which pages an entry lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AtlasKind {
    /// `R8Unorm` coverage.
    Mask,
    /// `Rgba8Unorm`, premultiplied.
    Color,
}

/// Where a resident entry's texels are, and how to place them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AtlasSlot {
    pub kind: AtlasKind,
    pub layer: u16,
    /// Top-left of the texels (inside the gutter), atlas px.
    pub x: u16,
    pub y: u16,
    /// Bitmap size in px.
    pub w: u16,
    pub h: u16,
    /// Placement bearing.
    pub left: i16,
    pub top: i16,
}

struct Entry {
    /// `None` = empty glyph (e.g. a space): a cached miss.
    slot: Option<(AtlasSlot, AllocId)>,
    last_used: u64,
    /// Padded area in px (0 when empty).
    area: u64,
}

/// No room after every ladder step (§5.5 step 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Overflow;

/// The pages of one kind (10 §5.4).
pub(crate) struct AtlasPages {
    pub kind: AtlasKind,
    /// Current side, px (a power of two).
    pub side: u32,
    max_side: u32,
    /// Active layers, `len() ≥ 1`.
    layers: Vec<Guillotine>,
    max_layers: u32,
    /// `max(2, layers.len())`: the GPU texture's layer count (W1; never a multiple of 6, W2).
    pub physical_layers: u32,
    /// Σ padded area of resident slots.
    pub occupied_px: u64,
    /// +1 whenever `side` or `physical_layers` changes.
    pub generation: u64,
}

fn size(v: u32) -> i32 {
    i32::try_from(v).unwrap_or(i32::MAX)
}

impl AtlasPages {
    /// `max_layers` is clamped to 2..=5 (W1, W2).
    pub(crate) fn new(kind: AtlasKind, initial_side: u32, max_side: u32, max_layers: u32) -> Self {
        let side = initial_side.min(max_side).max(1);
        Self {
            kind,
            side,
            max_side: max_side.max(side),
            layers: vec![Guillotine::new(size2(size(side), size(side)))],
            max_layers: max_layers.clamp(2, 5),
            physical_layers: 2,
            occupied_px: 0,
            generation: 0,
        }
    }

    fn try_alloc(&mut self, pw: u32, ph: u32) -> Option<(usize, guillotiere::Allocation)> {
        let sz = size2(size(pw), size(ph));
        self.layers.iter_mut().enumerate().find_map(|(i, l)| l.allocate(sz).map(|a| (i, a)))
    }

    /// Ladder step 3.
    fn grow_side(&mut self) -> bool {
        if self.side >= self.max_side {
            return false;
        }
        self.side *= 2;
        for l in &mut self.layers {
            l.grow(size2(size(self.side), size(self.side)));
        }
        self.generation += 1;
        true
    }

    /// Ladder step 4.
    fn add_layer(&mut self) -> bool {
        if u32::try_from(self.layers.len()).unwrap_or(u32::MAX) >= self.max_layers {
            return false;
        }
        self.layers.push(Guillotine::new(size2(size(self.side), size(self.side))));
        let p = u32::try_from(self.layers.len()).unwrap_or(u32::MAX).max(2);
        if p != self.physical_layers {
            self.physical_layers = p;
            self.generation += 1;
        }
        true
    }

    /// Occupancy in percent of the active area (§9.1).
    pub(crate) fn occupancy_pct(&self) -> u8 {
        let total = u64::from(self.side).pow(2) * self.layers.len() as u64;
        u8::try_from(self.occupied_px * 100 / total.max(1)).unwrap_or(100)
    }

    /// Active layer count.
    #[cfg(test)]
    pub(crate) fn active_layers(&self) -> usize {
        self.layers.len()
    }
}

/// The coverage atlas: one entry map over the mask and color pages (10 §5.3).
pub(crate) struct CoverageAtlas {
    entries: HashMap<CoverageKey, Entry>,
    pub mask: AtlasPages,
    pub color: AtlasPages,
    frame: u64,
    /// Bytes held by path masks; never above `path_budget`.
    path_bytes: u64,
    path_budget: u64,
    /// Smallest padded area that overflowed this frame, per atlas kind (mask, color). After step 5
    /// nothing of that kind can be freed until the frame ends, so any request at least that large
    /// overflows too: fail fast instead of re-running the ladder.
    overflowed: [Option<u64>; 2],
}

impl CoverageAtlas {
    pub(crate) fn new(max_side: u32, mask_layers: u32, color_layers: u32, path_budget: u64) -> Self {
        Self {
            entries: HashMap::new(),
            mask: AtlasPages::new(AtlasKind::Mask, GLYPH_MASK_INITIAL_SIDE, max_side, mask_layers),
            color: AtlasPages::new(AtlasKind::Color, GLYPH_COLOR_INITIAL_SIDE, max_side, color_layers),
            frame: 0,
            path_bytes: 0,
            path_budget,
            overflowed: [None; 2],
        }
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
        self.overflowed = [None; 2];
    }

    /// §5.10: every `ATLAS_SWEEP_EVERY_FRAMES`, drop entries (resident and empty) unused for
    /// `ATLAS_SWEEP_STALE_FRAMES`.
    pub(crate) fn end_frame(&mut self, stats: &mut FrameStats) {
        if self.frame.is_multiple_of(ATLAS_SWEEP_EVERY_FRAMES) {
            let frame = self.frame;
            self.evict(|_, e| e.last_used + ATLAS_SWEEP_STALE_FRAMES < frame, stats);
        }
    }

    /// Look up `key`: `Some(None)` is a cached empty entry. Pins the entry to this frame.
    pub(crate) fn get(&mut self, key: impl Into<CoverageKey>) -> Option<Option<AtlasSlot>> {
        let e = self.entries.get_mut(&key.into())?;
        e.last_used = self.frame;
        Some(e.slot.map(|s| s.0))
    }

    /// Record an empty entry (zero-size bitmap) so it is never rasterized again.
    pub(crate) fn insert_empty(&mut self, key: impl Into<CoverageKey>) {
        self.entries.insert(key.into(), Entry { slot: None, last_used: self.frame, area: 0 });
    }

    fn pages(&mut self, kind: AtlasKind) -> &mut AtlasPages {
        match kind {
            AtlasKind::Mask => &mut self.mask,
            AtlasKind::Color => &mut self.color,
        }
    }

    /// Remove every resident entry matching `pred` (deallocating in the same step: guillotiere
    /// panics on a double deallocate). Returns how many were removed.
    fn evict(&mut self, pred: impl Fn(&CoverageKey, &Entry) -> bool, stats: &mut FrameStats) -> usize {
        let victims: Vec<CoverageKey> =
            self.entries.iter().filter(|(k, e)| pred(k, e)).map(|(k, _)| *k).collect();
        let mut n = 0;
        for key in victims {
            let Some(e) = self.entries.remove(&key) else { continue };
            if let Some((slot, id)) = e.slot {
                let pages = self.pages(slot.kind);
                if let Some(l) = pages.layers.get_mut(usize::from(slot.layer)) {
                    l.deallocate(id);
                }
                pages.occupied_px = pages.occupied_px.saturating_sub(e.area);
                if key.kind() == MaskKind::Path {
                    self.path_bytes = self.path_bytes.saturating_sub(e.area);
                }
                stats.glyph_evictions += 1;
                n += 1;
            }
        }
        n
    }

    /// Allocate a `w`×`h` entry with a 1-px gutter in the `atlas` pages, running the §5.5
    /// ladder. A path mask over the path budget first evicts unused path masks (never glyphs).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn insert(
        &mut self,
        key: impl Into<CoverageKey>,
        atlas: AtlasKind,
        w: u32,
        h: u32,
        left: i32,
        top: i32,
        stats: &mut FrameStats,
    ) -> Result<AtlasSlot, Overflow> {
        let key = key.into();
        let (pw, ph) = (w + 2 * GLYPH_GUTTER_PX, h + 2 * GLYPH_GUTTER_PX);
        let area = u64::from(pw) * u64::from(ph);
        let frame = self.frame;
        if key.kind() == MaskKind::Path && self.path_bytes + area > self.path_budget {
            self.evict(|k, e| k.kind() == MaskKind::Path && e.last_used < frame, stats);
            if self.path_bytes + area > self.path_budget {
                return Err(Overflow);
            }
        }
        let kind_index = usize::from(atlas == AtlasKind::Color);
        if self.overflowed[kind_index].is_some_and(|a| area >= a) {
            stats.glyph_overflow += 1;
            return Err(Overflow);
        }
        // Entries of this atlas kind with a slot (§5.5 steps 2 and 5).
        let of_kind = move |e: &Entry| e.slot.is_some_and(|(s, _)| s.kind == atlas);
        let mut step = 2;
        let (layer, alloc) = loop {
            // TRY (step 1, and after every step that made room).
            if let Some(found) = self.pages(atlas).try_alloc(pw, ph) {
                break found;
            }
            match step {
                2 => {
                    self.evict(|_, e| of_kind(e) && e.last_used + GLYPH_LADDER_STALE_FRAMES < frame, stats);
                    step = 3;
                }
                3 => {
                    if self.pages(atlas).grow_side() {
                        stats.atlas_grows += 1;
                    } else {
                        step = 4;
                    }
                }
                4 => {
                    if self.pages(atlas).add_layer() {
                        stats.atlas_grows += 1;
                    } else {
                        step = 5;
                    }
                }
                5 => {
                    self.evict(|_, e| of_kind(e) && e.last_used < frame, stats);
                    step = 6;
                }
                _ => {
                    stats.glyph_overflow += 1;
                    let seen = self.overflowed[kind_index].get_or_insert(area);
                    *seen = (*seen).min(area);
                    return Err(Overflow);
                }
            }
        };
        let r = alloc.rectangle;
        let to16 = |v: i32| u16::try_from(v).unwrap_or(u16::MAX);
        let gutter = to16(size(GLYPH_GUTTER_PX));
        let slot = AtlasSlot {
            kind: atlas,
            layer: u16::try_from(layer).map_err(|_| Overflow)?,
            x: to16(r.min.x) + gutter,
            y: to16(r.min.y) + gutter,
            w: u16::try_from(w).map_err(|_| Overflow)?,
            h: u16::try_from(h).map_err(|_| Overflow)?,
            left: i16::try_from(left).unwrap_or(0),
            top: i16::try_from(top).unwrap_or(0),
        };
        self.pages(atlas).occupied_px += area;
        if key.kind() == MaskKind::Path {
            self.path_bytes += area;
        }
        self.entries.insert(key, Entry { slot: Some((slot, alloc.id)), last_used: frame, area });
        Ok(slot)
    }

    /// Resident + empty entries.
    #[cfg(any(test, debug_assertions))]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Bytes held by path masks.
    #[cfg(test)]
    pub(crate) fn path_bytes(&self) -> u64 {
        self.path_bytes
    }
}

#[cfg(test)]
#[path = "atlas_alloc_tests.rs"]
mod tests;
