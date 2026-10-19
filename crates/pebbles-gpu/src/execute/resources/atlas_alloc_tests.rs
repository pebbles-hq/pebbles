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
        colr_fg: None,
    }
}

fn sized(glyph_id: u16, size_q: u32) -> GlyphKey {
    GlyphKey { size_q, ..key(glyph_id) }
}

fn path(n: u64) -> MaskKey {
    MaskKey {
        geometry_hash: n,
        linear_q: [1024, 0, 0, 1024],
        subpx: (0, 0),
        rule: 0,
        stroke_hash: 0,
        blur_q: 0,
        blur_style: 0,
    }
}

fn atlas(max_side: u32, layers: u32) -> CoverageAtlas {
    CoverageAtlas::new(max_side, layers, layers, 8 << 20)
}

fn st() -> FrameStats {
    FrameStats::default()
}

#[test]
fn same_key_same_slot() {
    let mut a = atlas(2048, 4);
    let slot = a.insert(key(1), AtlasKind::Mask, 10, 12, 1, 11, &mut st()).unwrap();
    assert_eq!(a.get(key(1)), Some(Some(slot)));
    assert_eq!(a.get(key(2)), None);
    a.begin_frame();
    assert_eq!(a.get(key(1)), Some(Some(slot)), "the slot keeps its position across frames");
}

#[test]
fn ladder_evicts_stale_before_growing() {
    // A 1024 page filled by 4 glyphs of 510² (512² padded); the next one is placed by evicting a
    // stale entry (step 2), not by growing (step 3).
    let mut a = atlas(2048, 4);
    let mut s = st();
    for i in 0..4 {
        a.insert(key(i), AtlasKind::Mask, 510, 510, 0, 0, &mut s).unwrap();
    }
    for _ in 0..=GLYPH_LADDER_STALE_FRAMES {
        a.begin_frame();
    }
    a.insert(key(9), AtlasKind::Mask, 510, 510, 0, 0, &mut s).unwrap();
    assert_eq!((a.mask.side, s.atlas_grows), (1024, 0));
    assert!(s.glyph_evictions >= 1);
}

#[test]
fn ladder_grows_side_then_layers() {
    let mut a = atlas(2048, 3);
    let mut s = st();
    // Every glyph is used this frame (pinned): no eviction can help, so the ladder grows.
    for i in 0..16 {
        a.insert(key(i), AtlasKind::Mask, 510, 510, 0, 0, &mut s).unwrap();
    }
    assert_eq!(a.mask.side, 2048, "step 3 first");
    assert_eq!(a.mask.active_layers(), 1);
    a.insert(key(100), AtlasKind::Mask, 510, 510, 0, 0, &mut s).unwrap();
    assert_eq!(a.mask.active_layers(), 2, "then step 4");
    assert!(s.atlas_grows >= 2);
    assert!(a.mask.generation >= 1);
}

#[test]
fn pinned_entries_never_evicted() {
    let mut a = atlas(1024, 2);
    let mut s = st();
    for i in 0..8 {
        a.insert(key(i), AtlasKind::Mask, 510, 510, 0, 0, &mut s).unwrap();
    }
    assert!(a.insert(key(50), AtlasKind::Mask, 510, 510, 0, 0, &mut s).is_err());
    for i in 0..8 {
        assert!(a.get(key(i)).is_some(), "entry {i} pinned this frame");
    }
}

#[test]
fn overflow_after_all_steps() {
    let mut a = atlas(512, 2);
    let mut s = st();
    // A bitmap larger than the max page side goes through every step and overflows.
    assert_eq!(a.insert(key(1), AtlasKind::Mask, 600, 20, 0, 0, &mut s), Err(Overflow));
    assert_eq!(s.glyph_overflow, 1);
    assert_eq!(a.mask.side, 512);
}

#[test]
fn layer_counts_are_webgl_safe() {
    for max_layers in 0..=8u32 {
        let mut a = atlas(1024, max_layers);
        let mut s = st();
        let mut i = 0u16;
        // Grow through every layer, checking the physical count after each allocation.
        while a.insert(key(i), AtlasKind::Mask, 1000, 1000, 0, 0, &mut s).is_ok() {
            let p = a.mask.physical_layers;
            assert!(p >= 2 && !p.is_multiple_of(6) && p <= 5, "max_layers {max_layers}: physical {p}");
            i += 1;
        }
        assert!(a.color.physical_layers >= 2);
    }
}

#[test]
fn churn_2000_glyphs_never_overflows() {
    let mut a = atlas(2048, 4);
    let mut s = st();
    // 2000 distinct 20×20 glyphs; each frame uses a sliding window of 200, every glyph id
    // recurring with a 60-frame period.
    for frame in 0..300u32 {
        a.begin_frame();
        for j in 0..200u32 {
            let id = ((frame * 33 + j) % 2000) as u16;
            if a.get(key(id)).is_none() {
                a.insert(key(id), AtlasKind::Mask, 20, 20, 0, 0, &mut s).expect("never overflows");
            }
        }
        a.end_frame(&mut s);
    }
    assert_eq!(s.glyph_overflow, 0);
}

#[test]
fn sweep_drops_empty_entries() {
    let mut a = atlas(1024, 2);
    let mut s = st();
    a.insert_empty(sized(1, 640));
    a.insert(key(2), AtlasKind::Mask, 10, 10, 0, 0, &mut s).unwrap();
    for _ in 0..(ATLAS_SWEEP_STALE_FRAMES + 2 * ATLAS_SWEEP_EVERY_FRAMES) {
        a.begin_frame();
        a.end_frame(&mut s);
    }
    assert_eq!(a.len(), 0, "empty and resident stale entries are swept");
}

#[test]
fn path_masks_share_the_page_and_count_bytes() {
    let mut a = atlas(2048, 4);
    let mut s = st();
    let g = a.insert(key(1), AtlasKind::Mask, 8, 8, 0, 0, &mut s).unwrap();
    let p = a.insert(path(1), AtlasKind::Mask, 8, 8, 0, 0, &mut s).unwrap();
    assert_ne!((g.x, g.y), (p.x, p.y));
    assert_eq!(a.path_bytes(), 100);
}

#[test]
fn path_budget_evicts_unused_paths_never_glyphs() {
    let mut a = CoverageAtlas::new(2048, 4, 2, 150);
    let mut s = st();
    a.insert(key(1), AtlasKind::Mask, 8, 8, 0, 0, &mut s).unwrap();
    a.insert(path(1), AtlasKind::Mask, 8, 8, 0, 0, &mut s).unwrap();
    a.begin_frame();
    assert!(a.get(key(1)).is_some());
    a.insert(path(2), AtlasKind::Mask, 8, 8, 0, 0, &mut s).expect("evicts the unused path mask");
    assert_eq!(a.get(path(1)), None);
    assert!(a.get(key(1)).is_some(), "glyph kept");
    assert_eq!(a.insert(path(3), AtlasKind::Mask, 8, 8, 0, 0, &mut s), Err(Overflow));
}

/// S09: an atlas capped after a growth OOM keeps the other kind and never grows again.
#[test]
fn cap_kind_drops_only_that_kind_and_freezes_size() {
    let mut stats = FrameStats::default();
    let mut a = CoverageAtlas::new(2048, 4, 4, u64::MAX);
    a.begin_frame();
    let g = key;
    a.insert(g(1), AtlasKind::Mask, 8, 8, 0, 0, &mut stats).expect("mask");
    a.insert(g(2), AtlasKind::Color, 8, 8, 0, 0, &mut stats).expect("color");
    let side = a.mask.side;
    a.cap_kind(AtlasKind::Mask, side, 2);
    assert!(a.get(g(1)).is_none(), "mask entries dropped");
    assert!(a.get(g(2)).is_some(), "color entries kept");
    // Fill past the capped side: it overflows instead of growing.
    let mut n = 0;
    while a
        .insert(g(100 + u16::try_from(n).unwrap_or(0)), AtlasKind::Mask, 200, 200, 0, 0, &mut stats)
        .is_ok()
    {
        n += 1;
        assert!(n < 10_000);
    }
    assert_eq!(a.mask.side, side, "never grows past the texture");
}
