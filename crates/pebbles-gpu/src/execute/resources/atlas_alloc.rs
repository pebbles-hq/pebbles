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
}

/// No room even after evicting every glyph not used this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AtlasFull;

/// The allocator for one atlas page (one kind).
pub(crate) struct AtlasAllocator {
    kind: AtlasKind,
    side: u32,
    alloc: Guillotine,
    entries: HashMap<GlyphKey, Entry>,
    frame: u64,
}

impl AtlasAllocator {
    pub(crate) fn new(kind: AtlasKind, side: u32) -> Self {
        let s = i32::try_from(side).unwrap_or(i32::MAX);
        Self { kind, side, alloc: Guillotine::new(size2(s, s)), entries: HashMap::new(), frame: 0 }
    }

    /// Start a new frame: entries used before it become evictable.
    pub(crate) fn begin_frame(&mut self) {
        self.frame += 1;
    }

    /// Look up `key`. `Some(None)` is a cached empty glyph. Stamps the entry as used this frame.
    pub(crate) fn get(&mut self, key: &GlyphKey) -> Option<Option<AtlasSlot>> {
        let e = self.entries.get_mut(key)?;
        e.last_used = self.frame;
        Some(e.slot.map(|s| s.0))
    }

    /// Record an empty glyph (zero-size bitmap) so it is never rasterized again.
    pub(crate) fn insert_empty(&mut self, key: GlyphKey) {
        self.entries.insert(key, Entry { slot: None, last_used: self.frame });
    }

    /// Allocate a `w`×`h` glyph with a 1-px gutter. On failure, evict entries not used this
    /// frame in LRU order and retry once.
    pub(crate) fn insert(
        &mut self,
        key: GlyphKey,
        w: u32,
        h: u32,
        left: i32,
        top: i32,
    ) -> Result<AtlasSlot, AtlasFull> {
        let pw = w + 2 * GLYPH_GUTTER_PX;
        let ph = h + 2 * GLYPH_GUTTER_PX;
        if pw > self.side || ph > self.side {
            return Err(AtlasFull);
        }
        let size =
            size2(i32::try_from(pw).map_err(|_| AtlasFull)?, i32::try_from(ph).map_err(|_| AtlasFull)?);
        let allocation = match self.alloc.allocate(size) {
            Some(a) => a,
            None => {
                self.evict_unused();
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
        self.entries.insert(key, Entry { slot: Some((slot, allocation.id)), last_used: self.frame });
        Ok(slot)
    }

    /// Deallocate every entry not used this frame, oldest first.
    fn evict_unused(&mut self) {
        let frame = self.frame;
        let mut stale: Vec<(u64, GlyphKey)> = self
            .entries
            .iter()
            .filter(|(_, e)| e.last_used < frame)
            .map(|(k, e)| (e.last_used, *k))
            .collect();
        stale.sort_unstable_by_key(|s| s.0);
        for (_, key) in stale {
            if let Some(Entry { slot: Some((_, id)), .. }) = self.entries.remove(&key) {
                self.alloc.deallocate(id);
            }
        }
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
        assert_eq!(a.get(&key(1)), Some(Some(slot)));
        assert_eq!(a.get(&key(2)), None);
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn evicts_lru_when_full_then_fits() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 32);
        a.insert(key(1), 30, 30, 0, 0).unwrap();
        a.begin_frame();
        let slot = a.insert(key(2), 30, 30, 0, 0).expect("evicts the unused glyph");
        assert_eq!((slot.x, slot.y), (1, 1));
        assert_eq!(a.get(&key(1)), None);
    }

    #[test]
    fn entries_used_this_frame_are_never_evicted() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 32);
        a.insert(key(1), 30, 30, 0, 0).unwrap();
        a.begin_frame();
        assert!(a.get(&key(1)).is_some());
        assert_eq!(a.insert(key(2), 30, 30, 0, 0), Err(AtlasFull));
        assert!(a.get(&key(1)).is_some());
    }

    #[test]
    fn empty_glyph_has_no_slot() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 32);
        a.insert_empty(key(3));
        assert_eq!(a.get(&key(3)), Some(None));
    }

    #[test]
    fn full_atlas_reports_atlas_full() {
        let mut a = AtlasAllocator::new(AtlasKind::Mask, 16);
        assert_eq!(a.insert(key(1), 20, 4, 0, 0), Err(AtlasFull));
    }
}
