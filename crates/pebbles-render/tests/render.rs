//! Unit tests for the pure layout logic — no GPU, no window.
//!
//! An integration test (it drives the crate through its public API), kept out of
//! `src/` so the library source is implementation only.

use pebbles_foundation::{
    Alignment, Axis, CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize, Size, TextBaseline,
    VerticalDirection,
};

use pebbles_render::constraints::BoxConstraints;
use pebbles_render::objects::{
    ParagraphStyle, RenderColoredBox, RenderConstrainedBox, RenderFlex, RenderPadding, RenderParagraph,
};
use pebbles_render::text::TextEnv;
use pebbles_render::tree::RenderTree;

fn tight(w: f64, h: f64) -> BoxConstraints {
    BoxConstraints::tight(Size::new(w, h))
}

#[test]
fn constraints_constrain_and_deflate() {
    let c = BoxConstraints { min_width: 0.0, max_width: 100.0, min_height: 0.0, max_height: 50.0 };
    assert_eq!(c.constrain(Size::new(200.0, 10.0)), Size::new(100.0, 10.0));

    let d = c.deflate(EdgeInsets::all(10.0));
    assert_eq!(d.max_width, 80.0);
    assert_eq!(d.max_height, 30.0);
}

#[test]
fn tight_constraints_are_tight() {
    assert!(tight(10.0, 20.0).is_tight());
    assert!(!BoxConstraints::UNBOUNDED.is_tight());
}

#[test]
fn alignment_inscribes_child() {
    let parent = Size::new(100.0, 100.0);
    let child = Size::new(20.0, 20.0);
    assert_eq!(Alignment::TOP_LEFT.inscribe(child, parent).to_point().x, 0.0);
    assert_eq!(Alignment::CENTER.inscribe(child, parent).to_point().x, 40.0);
    assert_eq!(Alignment::BOTTOM_RIGHT.inscribe(child, parent).to_point().y, 80.0);
}

/// Bundled families register into every `TextEnv`, are listed first in
/// discovery, and actually change shaping: the same string must measure
/// differently in Inter vs JetBrains Mono.
#[test]
fn builtin_fonts_register_list_and_shape() {
    let mut text = TextEnv::new();
    let fams = pebbles_render::fonts::available_families();
    assert_eq!(
        &fams[..pebbles_render::fonts::BUILTIN_FAMILIES.len()],
        pebbles_render::fonts::BUILTIN_FAMILIES
    );
    assert!(pebbles_render::fonts::has_family("inter")); // case-insensitive
    assert!(pebbles_render::fonts::is_builtin("SPACE GROTESK"));

    let mut tree = RenderTree::new();
    let sans = tree.insert(Box::new(RenderParagraph::new(
        "Baguette 0123456789",
        ParagraphStyle { font_family: Some("Inter".into()), ..ParagraphStyle::default() },
    )));
    let mono = tree.insert(Box::new(RenderParagraph::new(
        "Baguette 0123456789",
        ParagraphStyle { font_family: Some("JetBrains Mono".into()), ..ParagraphStyle::default() },
    )));
    tree.root = Some(sans);
    tree.layout(&mut text, BoxConstraints::UNBOUNDED);
    let sans_w = tree.size_of(sans).width;
    tree.root = Some(mono);
    tree.layout(&mut text, BoxConstraints::UNBOUNDED);
    let mono_w = tree.size_of(mono).width;
    assert_ne!(sans_w, mono_w, "family selection must change shaping");

    // Registering a bundled face again through the public API works.
    let n = text.register_font(pebbles_render::fonts::builtin_fonts()[0].1.to_vec());
    assert!(n > 0);
}

/// A childless colored box fills the space it is given.
#[test]
fn colored_box_fills_when_childless() {
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let root = tree.insert(Box::new(RenderColoredBox::new(pebbles_foundation::palette::RED)));
    tree.root = Some(root);
    tree.layout(&mut text, tight(120.0, 80.0));
    assert_eq!(tree.size_of(root), Size::new(120.0, 80.0));
}

/// Padding grows its child by the inset amounts and positions it inset.
#[test]
fn padding_grows_and_offsets_child() {
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let pad = tree.insert(Box::new(RenderPadding::new(EdgeInsets::all(10.0))));
    let child =
        tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(30.0, 30.0)))));
    tree.insert_child(pad, child, 0);
    tree.root = Some(pad);
    tree.layout(&mut text, BoxConstraints::UNBOUNDED);
    assert_eq!(tree.size_of(pad), Size::new(50.0, 50.0));
    assert_eq!(tree.offset_of(child).to_point().x, 10.0);
    assert_eq!(tree.offset_of(child).to_point().y, 10.0);
}

/// Every bundled Tabler `Path` primitive — both outline and filled — must parse
/// under kurbo's SVG parser, guarding the whole generated set (~5100 outline + ~1050
/// filled) against a path command the renderer can't handle.
#[test]
fn all_tabler_paths_parse() {
    use pebbles_render::BezPath;
    use pebbles_render::objects::{IconPrim, tabler};

    let mut checked = 0usize;
    let both = tabler::ALL.iter().chain(tabler::filled::ALL.iter());
    for (name, data) in both {
        for prim in data.prims {
            if let IconPrim::Path(d) = prim {
                assert!(BezPath::from_svg(d).is_ok(), "tabler `{name}` has an unparsable path: {d}");
                checked += 1;
            }
        }
    }
    assert!(checked > 5000, "expected the full Tabler set, only saw {checked} paths");
}

/// A row with one fixed child and one flex child splits the remaining main-axis
/// space to the flex child.
#[test]
fn flex_distributes_remaining_space() {
    use pebbles_foundation::FlexFit;
    use pebbles_render::objects::FlexParentData;

    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let flex = tree.insert(Box::new(RenderFlex::new(
        Axis::Horizontal,
        MainAxisAlignment::Start,
        CrossAxisAlignment::Start,
        MainAxisSize::Max,
        0.0,
        VerticalDirection::Down,
        TextBaseline::Alphabetic,
    )));
    // Fixed 40px-wide child.
    let fixed =
        tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    // Flexible child (fills the rest).
    let flexible = tree.insert(Box::new(RenderColoredBox::new(pebbles_foundation::palette::BLUE)));
    tree.insert_child(flex, fixed, 0);
    tree.insert_child(flex, flexible, 1);
    tree.set_parent_data(flexible, Box::new(FlexParentData { flex: 1, fit: FlexFit::Tight }));
    tree.root = Some(flex);

    tree.layout(&mut text, tight(200.0, 50.0));
    assert_eq!(tree.size_of(fixed).width, 40.0);
    // 200 total - 40 fixed = 160 for the single flex child.
    assert_eq!(tree.size_of(flexible).width, 160.0);
    // Positioned after the fixed child.
    assert_eq!(tree.offset_of(flexible).to_point().x, 40.0);
}

/// `spacing` (Flutter's `Flex.spacing`) reserves a fixed gap between children:
/// it grows the shrink-wrapped main size and offsets each subsequent child.
#[test]
fn flex_spacing_reserves_and_positions() {
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let flex = tree.insert(Box::new(RenderFlex::new(
        Axis::Horizontal,
        MainAxisAlignment::Start,
        CrossAxisAlignment::Start,
        MainAxisSize::Min,
        10.0,
        VerticalDirection::Down,
        TextBaseline::Alphabetic,
    )));
    let a = tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    let b = tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    tree.insert_child(flex, a, 0);
    tree.insert_child(flex, b, 1);
    tree.root = Some(flex);

    tree.layout(&mut text, BoxConstraints::UNBOUNDED);
    // Shrink-wrapped width = 40 + 40 + one 10px gap = 90.
    assert_eq!(tree.size_of(flex).width, 90.0);
    // The second child sits after the first plus the gap.
    assert_eq!(tree.offset_of(b).to_point().x, 50.0);
}

/// D2: under a right-to-left ambient direction, a Row lays its children out in reverse
/// — the first child ends up on the RIGHT. (Each `#[test]` runs on its own thread, so
/// the thread-local direction is isolated; reset anyway.)
#[test]
fn row_reverses_child_order_under_rtl() {
    use pebbles_foundation::TextDirection;

    pebbles_render::set_text_direction(TextDirection::Rtl);

    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let flex = tree.insert(Box::new(RenderFlex::new(
        Axis::Horizontal,
        MainAxisAlignment::Start,
        CrossAxisAlignment::Start,
        MainAxisSize::Min,
        0.0,
        VerticalDirection::Down,
        TextBaseline::Alphabetic,
    )));
    let a = tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    let b = tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    tree.insert_child(flex, a, 0);
    tree.insert_child(flex, b, 1);
    tree.root = Some(flex);

    tree.layout(&mut text, BoxConstraints::UNBOUNDED);
    // Reversed: child `a` (index 0) is placed second → to the right of `b`.
    assert_eq!(tree.offset_of(b).to_point().x, 0.0, "index-1 child leads on the left under RTL");
    assert_eq!(tree.offset_of(a).to_point().x, 40.0, "index-0 child is on the right under RTL");

    pebbles_render::set_text_direction(TextDirection::Ltr);
}

/// F2: the inspector's hit chain runs root → deepest, tagging each node's name + size.
#[test]
fn inspect_returns_the_hit_chain_deepest_last() {
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let flex = tree.insert(Box::new(RenderFlex::new(
        Axis::Horizontal,
        MainAxisAlignment::Start,
        CrossAxisAlignment::Start,
        MainAxisSize::Min,
        0.0,
        VerticalDirection::Down,
        TextBaseline::Alphabetic,
    )));
    let child =
        tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    tree.insert_child(flex, child, 0);
    tree.root = Some(flex);
    tree.layout(&mut text, BoxConstraints::UNBOUNDED);

    let chain = pebbles_render::inspect::inspect_at(&tree, pebbles_foundation::Offset::new(20.0, 10.0));
    assert!(!chain.is_empty(), "the point hits something");
    assert_eq!(chain.first().unwrap().name, "RenderFlex", "root is the flex");
    let deepest = chain.last().unwrap();
    assert_eq!(deepest.bounds.width(), 40.0, "deepest node is the 40×20 child");
    assert_eq!(deepest.bounds.height(), 20.0);
}

/// E3: a paragraph shapes only when its text / style change and lays out only when its
/// width changes too; an identical relayout reuses both. A new wrap width relayouts the
/// cached shape without reshaping (03 §3.8).
#[test]
fn paragraph_reshapes_only_when_inputs_change() {
    use pebbles_render::objects::{layout_count, reset_layout_count, reset_shape_count, shape_count};

    reset_shape_count();
    reset_layout_count();
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let p = tree.insert(Box::new(RenderParagraph::new("Hello world", ParagraphStyle::default())));
    tree.root = Some(p);

    tree.layout(&mut text, tight(200.0, 100.0));
    assert_eq!((shape_count(), layout_count()), (1, 1), "first layout shapes and lays out once");

    // Identical constraints → no reshape, no relayout.
    tree.layout(&mut text, tight(200.0, 100.0));
    assert_eq!((shape_count(), layout_count()), (1, 1), "an identical relayout reuses both");

    // A narrower wrap width → a new layout of the SAME shape.
    tree.layout(&mut text, tight(120.0, 100.0));
    assert_eq!((shape_count(), layout_count()), (1, 2), "a wrap-width change relayouts, never reshapes");

    // …and that new width is itself cached.
    tree.layout(&mut text, tight(120.0, 100.0));
    assert_eq!((shape_count(), layout_count()), (1, 2), "the new width is cached too");
}

// ---------------------------------------------------------------------------
// Viewport culling (P0): nothing offscreen is encoded into the scene.
// ---------------------------------------------------------------------------

#[test]
fn scroll_viewport_culls_offscreen_subtrees() {
    use pebbles_render::objects::RenderScroll;
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let scroll = tree.insert(Box::new(RenderScroll::new(Axis::Vertical)));
    let col = tree.insert(Box::new(RenderFlex::new(
        Axis::Vertical,
        MainAxisAlignment::Start,
        CrossAxisAlignment::Stretch,
        MainAxisSize::Min,
        0.0,
        VerticalDirection::Down,
        TextBaseline::Alphabetic,
    )));
    tree.insert_child(scroll, col, 0);
    for i in 0..100 {
        let p = tree.insert(Box::new(RenderParagraph::new(format!("row {i}"), ParagraphStyle::default())));
        tree.insert_child(col, p, i);
    }
    tree.root = Some(scroll);
    tree.layout(&mut text, tight(300.0, 200.0));

    pebbles_render::stats::reset_frame();
    let mut scene = pebbles_render::Scene::new();
    tree.paint(&mut text, &mut scene);
    let painted = pebbles_render::stats::painted_nodes();
    let culled = pebbles_render::stats::culled_nodes();
    assert!(painted < 30, "only ~a viewport of rows encodes (painted {painted})");
    assert!(culled > 60, "the rest culls (culled {culled})");

    // Scroll to the middle: rows on BOTH sides cull now, roughly the same window.
    {
        let s = tree.object_mut(scroll).downcast_mut::<RenderScroll>().unwrap();
        s.offset = 600.0;
        s.target = 600.0;
    }
    tree.mark_needs_layout(scroll);
    tree.layout(&mut text, tight(300.0, 200.0));
    pebbles_render::stats::reset_frame();
    let mut scene = pebbles_render::Scene::new();
    tree.paint(&mut text, &mut scene);
    let painted = pebbles_render::stats::painted_nodes();
    assert!(painted < 30, "mid-scroll window stays bounded (painted {painted})");
}

#[test]
fn shadow_bleeding_into_view_survives_culling() {
    use pebbles_foundation::{Color, Offset};
    use pebbles_render::decoration::{BoxDecoration, BoxShadow};
    use pebbles_render::objects::RenderDecoratedBox;

    // The window is 300×200. A shadowed card sits fully BELOW it (y 220..260),
    // wrapped in a plain tight box whose own rect is offscreen — its blur reaches
    // y≈190, so the subtree paint rect must keep it painted. A second card at
    // y=560 is far below and must cull (wrapper and all).
    let shadow = || {
        BoxDecoration::new().color(Color::from_rgba8(10, 10, 10, 255)).shadow(BoxShadow::new(
            Color::from_rgba8(0, 0, 0, 128),
            Offset::new(0.0, 0.0),
            15.0,
            0.0,
        ))
    };
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let col = tree.insert(Box::new(RenderFlex::new(
        Axis::Vertical,
        MainAxisAlignment::Start,
        CrossAxisAlignment::Stretch,
        MainAxisSize::Min,
        0.0,
        VerticalDirection::Down,
        TextBaseline::Alphabetic,
    )));
    let push = |tree: &mut RenderTree, node: pebbles_render::RenderId, idx: usize| {
        tree.insert_child(col, node, idx);
    };
    let spacer1 = tree.insert(Box::new(RenderConstrainedBox::new(tight(300.0, 220.0))));
    push(&mut tree, spacer1, 0);
    let wrap1 = tree.insert(Box::new(RenderConstrainedBox::new(tight(300.0, 40.0))));
    let card1 = tree.insert(Box::new(RenderDecoratedBox::new(shadow())));
    tree.insert_child(wrap1, card1, 0);
    push(&mut tree, wrap1, 1);
    let spacer2 = tree.insert(Box::new(RenderConstrainedBox::new(tight(300.0, 300.0))));
    push(&mut tree, spacer2, 2);
    let wrap2 = tree.insert(Box::new(RenderConstrainedBox::new(tight(300.0, 40.0))));
    let card2 = tree.insert(Box::new(RenderDecoratedBox::new(shadow())));
    tree.insert_child(wrap2, card2, 0);
    push(&mut tree, wrap2, 3);

    tree.root = Some(col);
    tree.layout(&mut text, tight(300.0, 200.0));
    pebbles_render::stats::reset_frame();
    let mut scene = pebbles_render::Scene::new();
    tree.paint(&mut text, &mut scene);
    // Painted: col + spacer1 + wrap1 + card1 (the shadow reaches into view).
    // Culled: spacer2 + wrap2 (card2 never visited — its parent culled).
    assert_eq!(pebbles_render::stats::painted_nodes(), 4, "the offscreen shadowed card still paints");
    assert_eq!(pebbles_render::stats::culled_nodes(), 2, "the far-away card culls at its wrapper");
}

#[test]
fn scrolling_repositions_without_relayout() {
    use pebbles_render::objects::RenderScroll;
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let scroll = tree.insert(Box::new(RenderScroll::new(Axis::Vertical)));
    let col = tree.insert(Box::new(RenderFlex::new(
        Axis::Vertical,
        MainAxisAlignment::Start,
        CrossAxisAlignment::Stretch,
        MainAxisSize::Min,
        0.0,
        VerticalDirection::Down,
        TextBaseline::Alphabetic,
    )));
    tree.insert_child(scroll, col, 0);
    for i in 0..100 {
        let p = tree.insert(Box::new(RenderParagraph::new(format!("row {i}"), ParagraphStyle::default())));
        tree.insert_child(col, p, i);
    }
    tree.root = Some(scroll);
    tree.layout(&mut text, tight(300.0, 200.0));

    // A scroll tick: mutate the offset and re-position the clipped child the way
    // dispatch does — WITHOUT marking layout.
    {
        let s = tree.object_mut(scroll).downcast_mut::<RenderScroll>().unwrap();
        s.offset = 600.0;
        s.target = 600.0;
    }
    tree.set_scrolled_child_offset(scroll, pebbles_foundation::Offset::new(0.0, -600.0));

    // The next frame's layout is a no-op: nothing is dirty, constraints unchanged.
    pebbles_render::stats::reset_frame();
    tree.layout(&mut text, tight(300.0, 200.0));
    assert_eq!(pebbles_render::stats::layout_calls(), 0, "a scroll frame runs zero layout");

    // Paint sees the moved window: early rows cull, a mid-document band paints.
    let mut scene = pebbles_render::Scene::new();
    tree.paint(&mut text, &mut scene);
    let painted = pebbles_render::stats::painted_nodes();
    let culled = pebbles_render::stats::culled_nodes();
    assert!(painted < 30, "mid-scroll paint window stays bounded (painted {painted})");
    assert!(culled > 0, "rows on both sides culled ({culled})");
}

// ---------------------------------------------------------------------------
// P5: the editor field shapes through the window cache and paints windowed.
// ---------------------------------------------------------------------------

#[test]
fn text_field_caret_blink_reuses_the_shaped_layout() {
    use pebbles_render::objects::{RenderTextField, TextFieldStyle};
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let mut field = RenderTextField::new("hello editor\nsecond line\nthird line", TextFieldStyle::default());
    field.multiline = true;
    field.field_id = 4242;
    field.focused = true;
    let id = tree.insert(Box::new(field));
    tree.root = Some(id);
    tree.layout(&mut text, tight(300.0, 200.0));
    let first = pebbles_render::text_edit::get_lines(4242).expect("published line table");
    assert_eq!(first.line_count(), 3);
    assert_eq!(text.shape_cache_len(), 3, "one shaped layout per line");

    // A caret blink: mutate display-only state, relayout — the SAME table must
    // come back (Rc identity), with zero new cache entries and zero shaping.
    tree.object_mut(id).downcast_mut::<RenderTextField>().unwrap().caret_visible = false;
    tree.mark_needs_layout(id);
    tree.layout(&mut text, tight(300.0, 200.0));
    let second = pebbles_render::text_edit::get_lines(4242).expect("published line table");
    assert!(std::rc::Rc::ptr_eq(&first, &second), "a blink must not rebuild the table");
    assert_eq!(text.shape_cache_len(), 3);

    // A real edit (append a line) shapes exactly ONE new line: the untouched
    // lines are cache hits — the keystroke cost is O(changed lines), never O(doc).
    {
        let f = tree.object_mut(id).downcast_mut::<RenderTextField>().unwrap();
        f.text.push_str("\nfourth line");
    }
    tree.mark_needs_layout(id);
    tree.layout(&mut text, tight(300.0, 200.0));
    let third = pebbles_render::text_edit::get_lines(4242).expect("published line table");
    assert!(!std::rc::Rc::ptr_eq(&second, &third), "an edit rebuilds the table");
    assert_eq!(third.line_count(), 4);
    assert_eq!(text.shape_cache_len(), 4, "exactly one NEW line shaped");
    pebbles_render::text_edit::clear(4242);
}

#[test]
fn line_table_motion_moves_the_caret_across_lines() {
    use pebbles_render::objects::{RenderTextField, TextFieldStyle};
    use pebbles_render::text_edit as edit;
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let src = "alpha beta\n\ngamma delta words\nlast";
    let mut field = RenderTextField::new(src, TextFieldStyle::default());
    field.multiline = true;
    field.field_id = 4244;
    let id = tree.insert(Box::new(field));
    tree.root = Some(id);
    tree.layout(&mut text, tight(400.0, 300.0));
    let table = edit::get_lines(4244).expect("table");
    assert_eq!(table.line_count(), 4, "empty line is a real line");

    // Line 0 = "alpha beta" (0..10), line 1 = "" (11..11),
    // line 2 = "gamma delta words" (12..29), line 3 = "last" (30..34).
    // Right at a line end crosses onto the next line's start (over the newline).
    let (_, f) = edit::right(4244, 10, 10, false).expect("right");
    assert_eq!(f, 11, "line end -> next (empty) line start");
    let (_, f) = edit::right(4244, 11, 11, false).expect("right");
    assert_eq!(f, 12, "empty line -> next line start");
    // Left at a line start crosses to the previous line's end.
    let (_, f) = edit::left(4244, 12, 12, false).expect("left");
    assert_eq!(f, 11, "line start -> previous (empty) line end");
    // Plain left/right inside a line moves one grapheme.
    let (_, f) = edit::right(4244, 0, 0, false).expect("right");
    assert_eq!(f, 1);
    let (_, f) = edit::left(4244, 5, 5, false).expect("left");
    assert_eq!(f, 4);
    // Home/End resolve within the line.
    let (_, f) = edit::line_start(4244, 17, 17, false).expect("home");
    assert_eq!(f, 12);
    let (_, f) = edit::line_end(4244, 17, 17, false).expect("end");
    assert_eq!(f, 29);
    // Down from line 0 lands on a real offset inside line 1 (the empty line -> its start).
    let (_, f) = edit::line_down(4244, 3, 3, false).expect("down");
    assert_eq!(f, 11, "down into the empty line sits at its start");
    // Down again into "gamma delta words", up returns to the empty line.
    let (_, f2) = edit::line_down(4244, f, f, false).expect("down");
    assert!((12..=29).contains(&f2), "down lands inside line 2 ({f2})");
    let (_, f3) = edit::line_up(4244, f2, f2, false).expect("up");
    assert_eq!(f3, 11);
    // Word motion crosses the boundary at a line start.
    let (_, f) = edit::word_left(4244, 12, 12, false).expect("word left");
    assert_eq!(f, 11, "word-left at a line start hops to the previous line end");
    // Hit-testing: a point on line 3's band maps into line 3's byte range.
    let y3 = {
        let r = table.caret_rect(30, 1.0);
        (r.y0 + r.y1) / 2.0
    };
    let b = edit::hit(4244, 2.0, y3).expect("hit");
    assert!((30..=34).contains(&b), "hit lands in the last line ({b})");
    // Selection extend keeps the anchor.
    let (a, f) = edit::extend_to(4244, 0, 0, 2.0, y3).expect("extend");
    assert_eq!(a, 0);
    assert!(f >= 30);
    pebbles_render::text_edit::clear(4244);
}

#[test]
fn text_field_paint_is_windowed_like_paragraphs() {
    use pebbles_render::objects::{RenderScroll, RenderTextField, TextFieldStyle};
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let scroll = tree.insert(Box::new(RenderScroll::new(Axis::Vertical)));
    let mut source = String::new();
    for i in 0..3000 {
        source.push_str(&format!("line {i} with several words here\n"));
    }
    let mut field = RenderTextField::new(source, TextFieldStyle::default());
    field.multiline = true;
    field.field_id = 4243;
    let f = tree.insert(Box::new(field));
    tree.insert_child(scroll, f, 0);
    tree.root = Some(scroll);
    tree.layout(&mut text, tight(400.0, 240.0));

    // P5.2: the COLD build is lazy — a 3000-line document materializes only the
    // caret window at layout (plus the visible window at paint below), never
    // every line. Pre-P5.2 this was 3000 shapes on mount.
    let cold = text.shape_cache_len();
    assert!(cold < 200, "a cold 3000-line mount shapes O(window), not O(document) ({cold})");

    pebbles_render::stats::reset_frame();
    let mut scene = pebbles_render::Scene::new();
    tree.paint(&mut text, &mut scene);
    let runs = pebbles_render::stats::glyph_runs();
    assert!(runs < 200, "a 3000-line field encodes only the window ({runs} runs)");

    // The keystroke contract at scale: with the caret already ON the target line
    // (its window materialized last frame, as in real typing), editing that line
    // shapes exactly one new layout — every other line is carried or a cache hit.
    let at = {
        let f = tree.object_mut(f).downcast_mut::<RenderTextField>().unwrap();
        let at = f.text.find("line 1500").expect("target line");
        f.anchor = at;
        f.focus = at;
        at
    };
    tree.mark_needs_layout(f);
    tree.layout(&mut text, tight(400.0, 240.0)); // caret window materializes here
    let before = text.shape_cache_len();
    {
        let f = tree.object_mut(f).downcast_mut::<RenderTextField>().unwrap();
        f.text.insert(at, 'x');
        f.anchor = at + 1;
        f.focus = at + 1;
    }
    tree.mark_needs_layout(f);
    let t0 = std::time::Instant::now();
    tree.layout(&mut text, tight(400.0, 240.0));
    let took = t0.elapsed();
    eprintln!("[perf field] keystroke relayout on 3000 lines: {took:?}");
    assert_eq!(text.shape_cache_len(), before + 1, "a keystroke shapes exactly ONE line of 3000");
    pebbles_render::text_edit::clear(4243);
}

// ---------------------------------------------------------------------------
// P5.2 — lazy cold build: estimate-then-measure + motion fallbacks
// ---------------------------------------------------------------------------

/// Wrapped lines drift from their estimates; the paint pass measures them on
/// first visibility, reflows the table, and requests a corrective relayout —
/// which must SETTLE (no request once the visible window is measured). The
/// wrapped region sits far below the caret window, so PAINT (not layout) does
/// the measuring — the scroll offset puts it in view.
#[test]
fn text_field_lazy_estimates_settle_via_corrective_relayout() {
    use pebbles_render::objects::{RenderScroll, RenderTextField, TextFieldStyle};
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let scroll = tree.insert(Box::new(RenderScroll::new(Axis::Vertical)));
    // Short lines except a band of heavy wrappers well beyond the caret window.
    let mut source = String::new();
    for i in 0..1000 {
        if (300..320).contains(&i) {
            source.push_str(&format!("long line {i} "));
            for _ in 0..30 {
                source.push_str("wrapping words keep coming ");
            }
        } else {
            source.push_str(&format!("short {i}"));
        }
        source.push('\n');
    }
    let mut field = RenderTextField::new(source, TextFieldStyle::default());
    field.multiline = true;
    field.field_id = 4245;
    let f = tree.insert(Box::new(field));
    tree.insert_child(scroll, f, 0);
    tree.root = Some(scroll);

    tree.layout(&mut text, tight(300.0, 400.0));
    let style = TextFieldStyle::default();
    let line_px = f64::from(style.font_size) * f64::from(style.line_height);
    {
        let table = pebbles_render::text_edit::get_lines(4245).expect("published");
        assert!((table.line_height(300) - line_px * 8.0).abs() > 0.0, "sanity: slot exists");
        assert!(!table.line_is_materialized(300), "the wrapped band starts unmaterialized");
        // Scroll the band into view (estimated position is close enough) — the
        // live wheel path: mutate the object's offset AND re-position the child
        // (scroll-is-paint moves the node offset without a layout pass).
        let y300 = table.line_top(300);
        let sc = tree.object_mut(scroll).downcast_mut::<RenderScroll>().unwrap();
        sc.offset = y300;
        sc.target = y300;
        tree.set_scrolled_child_offset(scroll, pebbles_foundation::Offset::new(0.0, -y300));
    }
    tree.mark_needs_paint(scroll);

    let mut scene = pebbles_render::Scene::new();
    let mut passes = 0;
    loop {
        let pending = tree.paint(&mut text, &mut scene);
        if pending.is_empty() {
            break;
        }
        for id in pending {
            tree.mark_needs_layout(id);
        }
        tree.layout(&mut text, tight(300.0, 400.0));
        passes += 1;
        assert!(passes < 6, "estimate-then-measure settles within a few passes");
    }
    eprintln!("[perf field] corrective passes to settle: {passes}");
    assert!(passes >= 1, "scrolling into a wrapped band actually triggered the corrective pass");
    let table = pebbles_render::text_edit::get_lines(4245).expect("published");
    assert!(table.line_is_materialized(300), "the visible band materialized");
    assert!(
        table.line_height(300) > 2.0 * line_px,
        "a wrapped line measured taller than its one-line estimate ({} vs {line_px})",
        table.line_height(300),
    );
    assert!(table.materialized_count() < 400, "the tail stays estimates");
    pebbles_render::text_edit::clear(4245);
}

/// Motion on a line that has never materialized (far outside the window +
/// caret window) must stay char-boundary–safe on multibyte text — approximate
/// is fine, panicking or splitting a char is not.
#[test]
fn text_field_lazy_motion_fallbacks_are_char_safe() {
    use pebbles_render::objects::{RenderScroll, RenderTextField, TextFieldStyle};
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let scroll = tree.insert(Box::new(RenderScroll::new(Axis::Vertical)));
    let mut source = String::new();
    for i in 0..600 {
        if i == 400 {
            source.push_str("héllo wörld ε—дом");
        } else {
            source.push_str(&format!("line {i}"));
        }
        source.push('\n');
    }
    let full = source.clone();
    let mut field = RenderTextField::new(source, TextFieldStyle::default());
    field.multiline = true;
    field.field_id = 4246;
    let f = tree.insert(Box::new(field));
    tree.insert_child(scroll, f, 0);
    tree.root = Some(scroll);
    tree.layout(&mut text, tight(400.0, 240.0));

    let table = pebbles_render::text_edit::get_lines(4246).expect("published");
    // Line 400 is far outside the caret window (caret at 0) and never painted.
    let start: usize = full.split('\n').take(400).map(|l| l.len() + 1).sum();
    let hline = "héllo wörld ε—дом";
    assert_eq!(&full[start..start + hline.len()], hline, "target line located");
    assert!(table.materialized_count() < 100, "tail never materialized");

    // One step right from the line start crosses the 2-byte 'h'? No — 'h' is
    // ASCII; step from 'h' onto 'é' (2 bytes) and back, plus word/line motion.
    let (_, r1) = pebbles_render::text_edit::right(4246, start, start, false).expect("right");
    assert!(full.is_char_boundary(r1) && r1 > start, "right lands on a boundary");
    let (_, r2) = pebbles_render::text_edit::right(4246, r1, r1, false).expect("right over é");
    assert!(full.is_char_boundary(r2) && r2 > r1, "é stepped whole");
    let (_, l1) = pebbles_render::text_edit::left(4246, r2, r2, false).expect("left");
    assert_eq!(l1, r1, "left retraces the same boundary");
    let (_, e) = pebbles_render::text_edit::line_end(4246, start, start, false).expect("end");
    assert_eq!(e, start + hline.len(), "End = source line end without shaping");
    let (_, s) = pebbles_render::text_edit::line_start(4246, e, e, false).expect("start");
    assert_eq!(s, start, "Home = source line start without shaping");
    let (_, w) = pebbles_render::text_edit::word_right(4246, start, start, false).expect("word");
    assert!(full.is_char_boundary(w) && w > start, "word motion boundary-safe");
    let (_, up) = pebbles_render::text_edit::line_up(4246, start + 1, start + 1, false).expect("up");
    assert!(full.is_char_boundary(up) && up < start, "vertical hop lands above, on a boundary");
    pebbles_render::text_edit::clear(4246);
}

/// Flutter `Matrix4.identity()..setEntry(3, 2, 0.002)..rotateY(0.6)` (column-major storage).
fn perspective_matrix4() -> [f64; 16] {
    let (s, c) = 0.6f64.sin_cos();
    let p = [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.002, 0.0, 0.0, 0.0, 1.0];
    let r = [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0];
    let mut out = [0.0; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = (0..4).map(|k| p[k * 4 + row] * r[col * 4 + k]).sum();
        }
    }
    out
}

/// L66: a perspective `Matrix4` hit-tests through its inverse: a point on the visible
/// (projected) card hits the child; a point inside the untransformed box but outside the
/// trapezoid misses it.
#[test]
fn projective_transform_hit_tests_through_inverse() {
    use pebbles_render::RenderTransform;
    use pebbles_render::paint::Projective;
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let t = tree.insert(Box::new(RenderTransform::matrix4(perspective_matrix4(), Alignment::CENTER)));
    let child = tree.insert(Box::new(RenderColoredBox::new(pebbles_foundation::palette::RED)));
    tree.insert_child(t, child, 0);
    tree.root = Some(t);
    tree.layout(&mut text, tight(200.0, 100.0));
    assert!(tree.hit_test(pebbles_foundation::Offset::new(100.0, 50.0)).contains(&child), "the center hits");
    // The same homography the tree uses: around the center.
    let h = Projective::from_affine(kurbo::Affine::translate((-100.0, -50.0)))
        .then(&Projective::from_matrix4(perspective_matrix4()))
        .then(&Projective::from_affine(kurbo::Affine::translate((100.0, 50.0))));
    let inv = h.inverse().unwrap();
    let outside = (0..20)
        .flat_map(|i| [(f64::from(i) * 10.0 + 1.0, 1.0), (f64::from(i) * 10.0 + 1.0, 99.0)])
        .find(|&(x, y)| {
            inv.map_point(kurbo::Point::new(x, y))
                .is_none_or(|q| !(0.0..200.0).contains(&q.x) || !(0.0..100.0).contains(&q.y))
        })
        .expect("a box point outside the trapezoid");
    let hits = tree.hit_test(pebbles_foundation::Offset::new(outside.0, outside.1));
    assert!(!hits.contains(&child), "{outside:?} is outside the projected card");
}

/// L66: an affine `Matrix4` keeps the affine transform path (no projection).
#[test]
fn affine_matrix4_uses_affine_path() {
    use pebbles_render::RenderTransform;
    use pebbles_render::object::RenderObject;
    let mut m = [0.0; 16];
    m[0] = 2.0;
    m[5] = 2.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m[12] = 5.0;
    let t = RenderTransform::matrix4(m, Alignment::TOP_LEFT);
    assert!(t.projective.is_none());
    assert_eq!(t.transform(Size::new(10.0, 10.0)), Some(kurbo::Affine::new([2.0, 0.0, 0.0, 2.0, 5.0, 0.0])));
    assert!(t.projection(Size::new(10.0, 10.0)).is_none());
}

/// Counts its `layout` calls and lays out its children with the incoming constraints.
struct LayoutProbe(std::rc::Rc<std::cell::Cell<u32>>);

impl pebbles_render::RenderObject for LayoutProbe {
    fn layout(&mut self, cx: &mut pebbles_render::LayoutCx<'_>, constraints: BoxConstraints) -> Size {
        self.0.set(self.0.get() + 1);
        for child in cx.children() {
            cx.layout_child(child, constraints);
        }
        constraints.constrain(Size::new(10.0, 10.0))
    }

    fn paint(&self, _cx: &mut pebbles_render::PaintCx<'_>, _offset: pebbles_foundation::Offset) {}
}

/// A font-set change re-lays out every node: `mark_all_needs_layout` dirties the whole tree,
/// not just one ancestor path.
#[test]
fn mark_all_needs_layout_dirties_every_node() {
    let runs = std::rc::Rc::new(std::cell::Cell::new(0));
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let root = tree.insert(Box::new(LayoutProbe(runs.clone())));
    for i in 0..3 {
        let child = tree.insert(Box::new(LayoutProbe(runs.clone())));
        tree.insert_child(root, child, i);
    }
    tree.root = Some(root);
    tree.layout(&mut text, tight(100.0, 100.0));
    assert_eq!(runs.get(), 4);
    tree.layout(&mut text, tight(100.0, 100.0));
    let clean = runs.get();
    tree.mark_all_needs_layout();
    tree.layout(&mut text, tight(100.0, 100.0));
    assert_eq!(runs.get() - clean, 4, "every node re-runs layout");
}

/// The public `text_edit` motions, hit and caret queries run on a cosmic-text `TextLayout`
/// published with `store_text` (03b E11), with unchanged signatures.
#[test]
fn text_edit_routes_text_layout_entries() {
    use pebbles_render::text_edit;
    let mut env = TextEnv::new();
    let style = ParagraphStyle {
        font_family: Some("JetBrains Mono".into()),
        font_size: 20.0,
        ..ParagraphStyle::default()
    };
    let layout = pebbles_render::text::layout_for_test(&mut env, "hello world\nab", &style, &[], None);
    let id = 77_001;
    text_edit::store(id, layout);
    assert_eq!(text_edit::right(id, 0, 0, false), Some((1, 1)));
    assert_eq!(text_edit::left(id, 2, 7, false), Some((2, 2)));
    assert_eq!(text_edit::right(id, 2, 7, true), Some((2, 8)));
    assert_eq!(text_edit::word_right(id, 0, 0, false), Some((5, 5)));
    assert_eq!(text_edit::word_left(id, 11, 11, false), Some((6, 6)));
    assert_eq!(text_edit::line_end(id, 3, 3, false), Some((11, 11)));
    assert_eq!(text_edit::line_start(id, 3, 3, false), Some((0, 0)));
    assert_eq!(text_edit::line_down(id, 1, 1, false), Some((13, 13)));
    assert_eq!(text_edit::line_up(id, 13, 13, false), Some((1, 1)));
    assert_eq!(text_edit::hit(id, 37.0, 10.0), Some(3));
    assert_eq!(text_edit::word_at(id, 78.0, 10.0), Some((6, 11)));
    assert_eq!(text_edit::extend_to(id, 2, 2, 37.0, 10.0), Some((2, 3)));
    assert!((text_edit::caret_rect(id, 3).expect("caret").x0 - 36.0).abs() <= 0.01);
    text_edit::clear(id);
    assert_eq!(text_edit::hit(id, 0.0, 0.0), None);
}

/// IME preedit (03b E13): the composition is spliced in at the caret and painted with a 1 px
/// underline spanning exactly the preedit's display range.
#[test]
fn text_field_preedit_is_underlined_at_the_caret() {
    use pebbles_render::objects::{RenderTextField, TextFieldStyle};
    use pebbles_render::paint::Scene;
    use pebbles_render::paint::display_list::Op;

    let mut field = RenderTextField::new("a", TextFieldStyle::default());
    field.focus = 1;
    field.anchor = 1;
    field.preedit = "にほ".to_string();
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let id = tree.insert(Box::new(field));
    tree.root = Some(id);
    tree.layout(&mut text, BoxConstraints::loose(Size::new(300.0, 100.0)));
    let mut scene = Scene::new();
    tree.paint(&mut text, &mut scene);

    // The display string is "aにほ": the preedit spans bytes 1..7.
    let style = ParagraphStyle { font_size: 14.0, line_height: 1.3, ..ParagraphStyle::default() };
    let layout = pebbles_render::text::layout_for_test(&mut text, "aにほ", &style, &[], None);
    let x0 = layout.caret_rect(1, pebbles_render::text::Affinity::Downstream, 0.0).x0;
    let x1 = layout.caret_rect(7, pebbles_render::text::Affinity::Downstream, 0.0).x0;
    let underline = scene.ops().iter().find_map(|op| match op {
        Op::Fill { shape, .. } => {
            let r = shape.bounding_box();
            ((r.height() - 1.0).abs() < 1e-6).then_some(r)
        }
        _ => None,
    });
    let r = underline.expect("a 1 px preedit underline");
    assert!((r.x0 - x0).abs() < 0.01 && (r.x1 - x1).abs() < 0.01, "{r:?} vs {x0}..{x1}");
}

/// 03b E12 AC2: one keystroke in a 10 000-line field lays out at most one changed line —
/// the untouched lines keep their layouts across the table rebuild.
#[test]
#[cfg(debug_assertions)] // shape counter is debug-only
fn ten_thousand_line_field_keystroke_shapes_one_line() {
    use pebbles_render::objects::{RenderTextField, TextFieldStyle, reset_shape_count, shape_count};
    let source: String = (0..10_000).map(|i| format!("line {i}\n")).collect();
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let mut field = RenderTextField::new(source.clone(), TextFieldStyle::default());
    field.multiline = true;
    field.field_id = 4250;
    field.focused = true;
    field.focus = source.find("line 5000").expect("line");
    field.anchor = field.focus;
    let id = tree.insert(Box::new(field));
    tree.root = Some(id);
    tree.layout(&mut text, tight(400.0, 300.0));
    let before = pebbles_render::text_edit::get_lines(4250).expect("table").materialized_count();

    // Type one character on the caret's line.
    reset_shape_count();
    {
        let f = tree.object_mut(id).downcast_mut::<RenderTextField>().unwrap();
        let at = f.focus;
        f.text.insert(at, 'x');
        f.focus += 1;
        f.anchor = f.focus;
    }
    tree.mark_needs_layout(id);
    tree.layout(&mut text, tight(400.0, 300.0));
    let after = pebbles_render::text_edit::get_lines(4250).expect("table").materialized_count();
    assert!(shape_count() <= 1, "one keystroke shaped {} lines", shape_count());
    assert!(after <= before + 2, "materialized {before} -> {after}");
}

/// A 10 000-line, 80-column document in a focused multi-line field (E18), laid out at 600×300.
#[cfg(debug_assertions)]
fn perf_field(id: u64) -> (RenderTree, TextEnv, pebbles_render::RenderId, String) {
    use pebbles_render::objects::{RenderTextField, TextFieldStyle};
    let line: String =
        "lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor in".into();
    let source: String = (0..10_000).map(|_| format!("{}\n", &line[..80.min(line.len())])).collect();
    let mut text = TextEnv::new();
    let mut tree = RenderTree::new();
    let mut field = RenderTextField::new(source.clone(), TextFieldStyle::default());
    field.multiline = true;
    field.field_id = id;
    field.focused = true;
    let id = tree.insert(Box::new(field));
    tree.root = Some(id);
    tree.layout(&mut text, tight(600.0, 300.0));
    (tree, text, id, source)
}

/// E18 (a): one keystroke shapes ≤ 1 line and lays out ≤ 1 line.
#[test]
#[cfg(debug_assertions)] // shape / layout counters are debug-only
fn perf_keystroke_shapes_and_lays_out_one_line() {
    use pebbles_render::objects::{
        RenderTextField, layout_count, reset_layout_count, reset_shape_count, shape_count,
    };
    let (mut tree, mut text, id, _) = perf_field(4301);
    reset_shape_count();
    reset_layout_count();
    {
        let f = tree.object_mut(id).downcast_mut::<RenderTextField>().unwrap();
        f.text.insert(3, 'x');
        f.focus = 4;
        f.anchor = 4;
    }
    tree.mark_needs_layout(id);
    tree.layout(&mut text, tight(600.0, 300.0));
    assert!(shape_count() <= 1 && layout_count() <= 1, "shapes {} layouts {}", shape_count(), layout_count());
}

/// E18 (b): 1 000 `line_down` motions shape nothing once warm.
#[test]
#[cfg(debug_assertions)]
fn perf_line_down_never_shapes() {
    use pebbles_render::objects::{reset_shape_count, shape_count};
    use pebbles_render::text_edit;
    let (_tree, _text, _, source) = perf_field(4302);
    let mut f = 0;
    f = text_edit::line_down(4302, f, f, false).expect("published").1; // warm-up
    reset_shape_count();
    for _ in 0..1_000 {
        f = text_edit::line_down(4302, f, f, false).expect("published").1;
    }
    assert!(f <= source.len());
    assert_eq!(shape_count(), 0, "vertical motion must not shape");
}

/// E18 (c): painting a select-all only materializes the visible window, never the document.
#[test]
#[cfg(debug_assertions)]
fn perf_select_all_paints_only_visible_slots() {
    use pebbles_render::objects::RenderTextField;
    use pebbles_render::paint::Scene;
    use pebbles_render::text_edit;
    let (mut tree, mut text, id, source) = perf_field(4303);
    {
        let f = tree.object_mut(id).downcast_mut::<RenderTextField>().unwrap();
        f.anchor = 0;
        f.focus = source.len();
    }
    tree.mark_needs_layout(id);
    tree.layout(&mut text, tight(600.0, 300.0));
    let mut scene = Scene::new();
    tree.paint(&mut text, &mut scene);
    let table = text_edit::get_lines(4303).expect("table");
    assert!(table.materialized_count() < 200, "materialized {} of 10 000 lines", table.materialized_count());
}

/// E18 AC2 (informative): wall time of 1 000 `right` motions.
#[test]
#[cfg(debug_assertions)]
fn perf_right_motions_timing() {
    use pebbles_render::text_edit;
    let (_tree, _text, _, _) = perf_field(4304);
    let t = web_time::Instant::now();
    let mut f = 0;
    for _ in 0..1_000 {
        f = text_edit::right(4304, f, f, false).expect("published").1;
    }
    println!("right_x1000_ms={:.3} (focus {f})", t.elapsed().as_secs_f64() * 1e3);
}

/// T45: a span background paints before the selection highlight (selection stays visible).
#[test]
fn background_under_selection() {
    use pebbles_render::objects::TextSpanStyle;
    use pebbles_render::paint::Scene;
    use pebbles_render::paint::display_list::Op;
    let mut span = TextSpanStyle::new(0..5);
    span.background = Some(pebbles_foundation::palette::RED);
    let mut para = RenderParagraph::with_spans("hello world", ParagraphStyle::default(), vec![span]);
    para.selection = Some((0, 5));
    let selection_color = para.selection_color;
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let id = tree.insert(Box::new(para));
    tree.root = Some(id);
    tree.layout(&mut text, BoxConstraints::UNBOUNDED);
    let mut scene = Scene::new();
    tree.paint(&mut text, &mut scene);
    let fills: Vec<peniko::Brush> = scene
        .ops()
        .iter()
        .filter_map(|op| match op {
            Op::Fill { brush, .. } => Some(brush.clone()),
            _ => None,
        })
        .collect();
    let bg = fills.iter().position(|b| *b == peniko::Brush::Solid(pebbles_foundation::palette::RED));
    let sel = fills.iter().position(|b| *b == peniko::Brush::Solid(selection_color));
    assert!(bg.expect("background") < sel.expect("selection"), "{fills:?}");
}

/// T49: a paragraph's intrinsic height includes its strut (sized through IntrinsicHeight).
#[test]
fn intrinsics_include_strut() {
    use pebbles_render::objects::{RenderIntrinsicHeight, StrutStyle};
    let strut = StrutStyle {
        font_family: None,
        font_size: 40.0,
        height: None,
        leading: None,
        force_strut_height: false,
    };
    let style = ParagraphStyle { strut: Some(strut), ..ParagraphStyle::default() };
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let root = tree.insert(Box::new(RenderIntrinsicHeight));
    let p = tree.insert(Box::new(RenderParagraph::new("Hi", style)));
    tree.insert_child(root, p, 0);
    tree.root = Some(root);
    tree.layout(&mut text, BoxConstraints::loose(Size::new(300.0, 1000.0)));
    let h = tree.size_of(root).height;
    assert!(h >= (1984.0 + 494.0) / 2048.0 * 40.0 - 0.01, "{h}");
}

/// T09 AC3 (automated on the host): with `SystemFonts::Background`, a paragraph naming a
/// system-only family renders in the bundled default first, then — after the background scan
/// merges (polled like the runner, once per frame) — in that family, within 2 s.
#[test]
#[ignore = "scans the host's system fonts"]
fn background_scan_switches_system_family_within_2s() {
    use pebbles_render::SystemFonts;
    let family = "DejaVu Sans";
    let width = |text: &mut TextEnv| {
        let style = ParagraphStyle { font_family: Some(family.into()), ..ParagraphStyle::default() };
        let mut tree = RenderTree::new();
        let p = tree.insert(Box::new(RenderParagraph::new("Hamburgefonstiv", style)));
        tree.root = Some(p);
        tree.layout(text, BoxConstraints::UNBOUNDED);
        tree.size_of(p).width
    };
    let mut text = TextEnv::with_system_fonts(SystemFonts::Background);
    let start = std::time::Instant::now();
    let first = width(&mut text);
    let mut merged = false;
    while start.elapsed() < std::time::Duration::from_secs(2) {
        if text.poll_fonts() {
            merged = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(16)); // one frame
    }
    assert!(merged, "the background scan did not merge within 2 s");
    let after = width(&mut text);
    let inter = width(&mut TextEnv::new());
    assert!((first - inter).abs() < 0.01, "before the merge it renders in the bundled default");
    assert!((after - first).abs() > 0.5, "after the merge it renders in {family}: {first} -> {after}");
    println!("merged in {:?}", start.elapsed());
}
