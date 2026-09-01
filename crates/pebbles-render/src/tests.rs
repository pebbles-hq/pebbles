//! Unit tests for the pure layout logic — no GPU, no window.

use pebbles_foundation::{Alignment, Axis, CrossAxisAlignment, EdgeInsets, MainAxisAlignment, MainAxisSize, Size};

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
