//! Glyph atlas allocation — pure CPU, no wgpu types (10 §5). C09 lands one page per kind;
//! growth and extra array layers are M01.

use std::collections::HashMap;

use guillotiere::{AllocId, AtlasAllocator as Guillotine, size2};

/// Padding around every glyph, in atlas px (10 §5.5 `GLYPH_GUTTER_PX`).
pub(crate) const GLYPH_GUTTER_PX: u32 = 1;

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
    /// 0 when the run has no normalized coords, else a 64-bit hash of them (collisions negligible).
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

/// A coverage-atlas key (10 §5: glyph and path masks share pages and the LRU).
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

/// What an atlas entry holds, for per-kind byte budgets (10 §5, M28).
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

/// Which atlas page a glyph lives in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AtlasKind {
    /// `R8Unorm` coverage.
    Mask,
    /// `Rgba8Unorm`, premultiplied.
    Color,
}

/// Where a resident glyph's texels are, and how to place them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AtlasSlot {
    pub kind: AtlasKind,
    pub layer: u16,
    /// Top-left of the glyph texels (inside the gutter), atlas px.
    pub x: u16,
    pub y: u16,
    /// Glyph bitmap size in px.
    pub w: u16,
    pub h: u16,
    /// swash placement bearing.
    pub left: i16,
    pub top: i16,
}

struct Entry {
    /// `None` = empty glyph (e.g. a space): a cached miss.
    slot: Option<(AtlasSlot, AllocId)>,
    last_used: u64,
    /// Padded texel bytes this entry occupies (0 when empty).
    bytes: u64,
}

/// No room even after evicting every glyph not used this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AtlasFull;

/// The allocator for one atlas page (one kind).
pub(crate) struct AtlasAllocator {
    kind: AtlasKind,
    side: u32,
    alloc: Guillotine,
    entries: HashMap<CoverageKey, Entry>,
    frame: u64,
    /// Bytes held by path masks; never above `path_budget`.
    path_bytes: u64,
    path_budget: u64,
}

impl AtlasAllocator {
    pub(crate) fn new(kind: AtlasKind, side: u32) -> Self {
        let s = i32::try_from(side).unwrap_or(i32::MAX);
        Self {
            kind,
            side,
            alloc: Guillotine::new(size2(s, s)),
            entries: HashMap::new(),
            frame: 0,
            path_bytes: 0,
            path_budget: 8 << 20, // replaced by GpuBudgets::path_mask_bytes
        }
    }

    /// Start a new frame: entries used before it become evictable.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Look up `key`. `Some(None)` is a cached empty glyph. Stamps the entry as used this frame.
    pub(crate) fn get(&mut self, key: impl Into<CoverageKey>) -> Option<Option<AtlasSlot>> {
        let e = self.entries.get_mut(&key.into())?;
        e.last_used = self.frame;
        Some(e.slot.map(|s| s.0))
    }

    /// Record an empty glyph (zero-size bitmap) so it is never rasterized again.
    pub(crate) fn insert_empty(&mut self, key: impl Into<CoverageKey>) {
        self.entries.insert(key.into(), Entry { slot: None, last_used: self.frame, bytes: 0 });
    }

    /// Allocate a `w`×`h` mask with a 1-px gutter. On failure, evict entries not used this
    /// frame in LRU order and retry once. A path mask that would exceed the path budget first
    /// evicts unused path masks (never glyphs); still over budget → `AtlasFull`.
    pub(crate) fn insert(
        &mut self,
        key: impl Into<CoverageKey>,
        w: u32,
        h: u32,
        left: i32,
        top: i32,
    ) -> Result<AtlasSlot, AtlasFull> {
        let key = key.into();
        let pw = w + 2 * GLYPH_GUTTER_PX;
        let ph = h + 2 * GLYPH_GUTTER_PX;
        if pw > self.side || ph > self.side {
            return Err(AtlasFull);
        }
        let bytes = u64::from(pw) * u64::from(ph);
        if key.kind() == MaskKind::Path && self.path_bytes + bytes > self.path_budget {
            self.evict_unused(Some(MaskKind::Path));
            if self.path_bytes + bytes > self.path_budget {
                return Err(AtlasFull);
            }
        }
        let size =
            size2(i32::try_from(pw).map_err(|_| AtlasFull)?, i32::try_from(ph).map_err(|_| AtlasFull)?);
        let allocation = match self.alloc.allocate(size) {
            Some(a) => a,
            None => {
                self.evict_unused(None);
                self.alloc.allocate(size).ok_or(AtlasFull)?
            }
        };
        let r = allocation.rectangle;
        let to16 = |v: i32| u16::try_from(v).unwrap_or(u16::MAX);
        let gutter = to16(i32::try_from(GLYPH_GUTTER_PX).unwrap_or(1));
        let slot = AtlasSlot {
            kind: self.kind,
            layer: 0,
            x: to16(r.min.x) + gutter,
            y: to16(r.min.y) + gutter,
            w: u16::try_from(w).map_err(|_| AtlasFull)?,
            h: u16::try_from(h).map_err(|_| AtlasFull)?,
            left: i16::try_from(left).unwrap_or(0),
            top: i16::try_from(top).unwrap_or(0),
        };
        self.entries.insert(key, Entry { slot: Some((slot, allocation.id)), last_used: self.frame, bytes });
        if key.kind() == MaskKind::Path {
            self.path_bytes += bytes;
        }
        Ok(slot)
    }

    /// Deallocate every entry (of `only` kind, if given) not used this frame, oldest first.
    fn evict_unused(&mut self, only: Option<MaskKind>) {
        let frame = self.frame;
        let mut stale: Vec<(u64, CoverageKey)> = self
            .entries
            .iter()
            .filter(|(k, e)| e.last_used < frame && only.is_none_or(|kind| k.kind() == kind))
            .map(|(k, e)| (e.last_used, *k))
            .collect();
        stale.sort_unstable_by_key(|s| s.0);
        for (_, key) in stale {
            if let Some(Entry { slot: Some((_, id)), bytes, .. }) = self.entries.remove(&key) {
                self.alloc.deallocate(id);
                if key.kind() == MaskKind::Path {
                    self.path_bytes -= bytes;
                }
            }
        }
    }

    /// Bytes held by path masks.
    #[cfg(test)]
    pub(crate) fn path_bytes(&self) -> u64 {
        self.path_bytes
    }

    /// Set the path-mask budget (`GpuBudgets::path_mask_bytes`).
    pub(crate) fn set_path_budget(&mut self, bytes: u64) {
        self.path_budget = bytes;
    }

    /// Resident entries (including cached empty glyphs).
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(glyph_id: u16) -> GlyphKey {
        GlyphKey {
            font_blob: 1,
            font_index: 0,
            glyph_id,
            size_q: 16 * 64,
            subpx_x: 0,
            hint: false,
            coords_hash: 0,
        }
    }

    #[test]
    fn same_key_hits_without_reinsert() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 64);
        let slot = a.insert(key(1), 10, 12, 1, 11).unwrap();
        assert_eq!(a.get(key(1)), Some(Some(slot)));
        assert_eq!(a.get(key(2)), None);
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn evicts_lru_when_full_then_fits() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 32);
        a.insert(key(1), 30, 30, 0, 0).unwrap();
        a.begin_frame();
        let slot = a.insert(key(2), 30, 30, 0, 0).expect("evicts the unused glyph");
        assert_eq!((slot.x, slot.y), (1, 1));
        assert_eq!(a.get(key(1)), None);
    }

    #[test]
    fn entries_used_this_frame_are_never_evicted() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 32);
        a.insert(key(1), 30, 30, 0, 0).unwrap();
        a.begin_frame();
        assert!(a.get(key(1)).is_some());
        assert_eq!(a.insert(key(2), 30, 30, 0, 0), Err(AtlasFull));
        assert!(a.get(key(1)).is_some());
    }

    #[test]
    fn empty_glyph_has_no_slot() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 32);
        a.insert_empty(key(3));
        assert_eq!(a.get(key(3)), Some(None));
    }

    fn path(n: u64) -> MaskKey {
        MaskKey { geometry_hash: n, linear_q: [1024, 0, 0, 1024], subpx: (0, 0), rule: 0, stroke_hash: 0 }
    }

    #[test]
    fn path_masks_share_the_page_and_count_bytes() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 64);
        let g = a.insert(key(1), 8, 8, 0, 0).unwrap();
        let p = a.insert(path(1), 8, 8, 0, 0).unwrap();
        assert_ne!((g.x, g.y), (p.x, p.y));
        assert_eq!(a.path_bytes(), 100);
        assert_eq!(a.get(path(1)), Some(Some(p)));
        assert_eq!(a.get(path(2)), None);
    }

    #[test]
    fn path_budget_evicts_unused_paths_never_glyphs() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 64);
        a.set_path_budget(150);
        a.insert(key(1), 8, 8, 0, 0).unwrap();
        a.insert(path(1), 8, 8, 0, 0).unwrap();
        a.begin_frame();
        a.insert(path(2), 8, 8, 0, 0).expect("evicts the unused path mask");
        assert_eq!(a.get(path(1)), None);
        assert!(a.get(key(1)).is_some(), "glyph kept");
        assert_eq!(a.path_bytes(), 100);
        // Over budget within one frame: the used path cannot be evicted.
        assert_eq!(a.insert(path(3), 8, 8, 0, 0), Err(AtlasFull));
    }

    #[test]
    fn full_atlas_reports_atlas_full() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 16);
        assert_eq!(a.insert(key(1), 20, 4, 0, 0), Err(AtlasFull));
    }
}
