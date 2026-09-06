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

/// Every bundled Lucide `Path` primitive must parse under kurbo's SVG parser —
/// this guards the whole generated set (all ~1800 icons) against a path command
/// the renderer can't handle.
#[test]
fn all_lucide_paths_parse() {
    use pebbles_render::objects::{IconPrim, lucide};
    use vello::kurbo::BezPath;

    let mut checked = 0usize;
    for (name, data) in lucide::ALL {
        for prim in data.prims {
            if let IconPrim::Path(d) = prim {
                assert!(BezPath::from_svg(d).is_ok(), "lucide `{name}` has an unparsable path: {d}");
                checked += 1;
            }
        }
    }
    assert!(checked > 1000, "expected the full Lucide set, only saw {checked} paths");
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

/// E3: a paragraph re-shapes only when its text / style / wrap-width change; an
/// identical relayout reuses the cached shaped layout.
#[test]
fn paragraph_reshapes_only_when_inputs_change() {
    use pebbles_render::objects::{reset_shape_count, shape_count};

    reset_shape_count();
    let mut tree = RenderTree::new();
    let mut text = TextEnv::new();
    let p = tree.insert(Box::new(RenderParagraph::new("Hello world", ParagraphStyle::default())));
    tree.root = Some(p);

    tree.layout(&mut text, tight(200.0, 100.0));
    assert_eq!(shape_count(), 1, "first layout shapes once");

    // Identical constraints → cache hit, no re-shape.
    tree.layout(&mut text, tight(200.0, 100.0));
    assert_eq!(shape_count(), 1, "an identical relayout reuses the shape");

    // A narrower wrap width → re-shape.
    tree.layout(&mut text, tight(120.0, 100.0));
    assert_eq!(shape_count(), 2, "a wrap-width change re-shapes");

    // …and that new width is itself cached.
    tree.layout(&mut text, tight(120.0, 100.0));
    assert_eq!(shape_count(), 2, "the new width is cached too");
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
    let mut scene = vello::Scene::new();
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
    let mut scene = vello::Scene::new();
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
    let mut scene = vello::Scene::new();
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
    let mut scene = vello::Scene::new();
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
    let mut scene = vello::Scene::new();
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

    let mut scene = vello::Scene::new();
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
