//! Unit tests for the pure layout logic — no GPU, no window.

use pebbles_foundation::{Alignment, Axis, CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize, Size, TextBaseline, VerticalDirection};

use crate::constraints::BoxConstraints;
use crate::objects::{RenderColoredBox, RenderConstrainedBox, RenderFlex, RenderPadding, RenderParagraph, ParagraphStyle};
use crate::text::TextEnv;
use crate::tree::RenderTree;

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
    let fams = crate::fonts::available_families();
    assert_eq!(&fams[..crate::fonts::BUILTIN_FAMILIES.len()], crate::fonts::BUILTIN_FAMILIES);
    assert!(crate::fonts::has_family("inter")); // case-insensitive
    assert!(crate::fonts::is_builtin("SPACE GROTESK"));

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
    let n = text.register_font(crate::fonts::builtin_fonts()[0].1.to_vec());
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
    let child = tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(
        30.0, 30.0,
    )))));
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
    use crate::objects::{IconPrim, lucide};
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
    use crate::objects::FlexParentData;
    use pebbles_foundation::FlexFit;

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
    let fixed = tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(
        40.0, 20.0,
    )))));
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
    let a =
        tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    let b =
        tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
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

    crate::set_text_direction(TextDirection::Rtl);

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

    crate::set_text_direction(TextDirection::Ltr);
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
    let child = tree.insert(Box::new(RenderConstrainedBox::new(BoxConstraints::tight(Size::new(40.0, 20.0)))));
    tree.insert_child(flex, child, 0);
    tree.root = Some(flex);
    tree.layout(&mut text, BoxConstraints::UNBOUNDED);

    let chain = crate::inspect::inspect_at(&tree, pebbles_foundation::Offset::new(20.0, 10.0));
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
    use crate::objects::{reset_shape_count, shape_count};

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
    use crate::objects::RenderScroll;
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
        let p = tree
            .insert(Box::new(RenderParagraph::new(format!("row {i}"), ParagraphStyle::default())));
        tree.insert_child(col, p, i);
    }
    tree.root = Some(scroll);
    tree.layout(&mut text, tight(300.0, 200.0));

    crate::stats::reset_frame();
    let mut scene = vello::Scene::new();
    tree.paint(&mut scene);
    let painted = crate::stats::painted_nodes();
    let culled = crate::stats::culled_nodes();
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
    crate::stats::reset_frame();
    let mut scene = vello::Scene::new();
    tree.paint(&mut scene);
    let painted = crate::stats::painted_nodes();
    assert!(painted < 30, "mid-scroll window stays bounded (painted {painted})");
}

#[test]
fn shadow_bleeding_into_view_survives_culling() {
    use crate::decoration::{BoxDecoration, BoxShadow};
    use crate::objects::RenderDecoratedBox;
    use pebbles_foundation::{Color, Offset};

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
    let mut push = |tree: &mut RenderTree, node: crate::RenderId, idx: usize| {
        tree.insert_child(col, node, idx);
    };
    let spacer1 = tree
        .insert(Box::new(RenderConstrainedBox::new(tight(300.0, 220.0))));
    push(&mut tree, spacer1, 0);
    let wrap1 = tree.insert(Box::new(RenderConstrainedBox::new(tight(300.0, 40.0))));
    let card1 = tree.insert(Box::new(RenderDecoratedBox::new(shadow())));
    tree.insert_child(wrap1, card1, 0);
    push(&mut tree, wrap1, 1);
    let spacer2 = tree
        .insert(Box::new(RenderConstrainedBox::new(tight(300.0, 300.0))));
    push(&mut tree, spacer2, 2);
    let wrap2 = tree.insert(Box::new(RenderConstrainedBox::new(tight(300.0, 40.0))));
    let card2 = tree.insert(Box::new(RenderDecoratedBox::new(shadow())));
    tree.insert_child(wrap2, card2, 0);
    push(&mut tree, wrap2, 3);

    tree.root = Some(col);
    tree.layout(&mut text, tight(300.0, 200.0));
    crate::stats::reset_frame();
    let mut scene = vello::Scene::new();
    tree.paint(&mut scene);
    // Painted: col + spacer1 + wrap1 + card1 (the shadow reaches into view).
    // Culled: spacer2 + wrap2 (card2 never visited — its parent culled).
    assert_eq!(crate::stats::painted_nodes(), 4, "the offscreen shadowed card still paints");
    assert_eq!(crate::stats::culled_nodes(), 2, "the far-away card culls at its wrapper");
}
