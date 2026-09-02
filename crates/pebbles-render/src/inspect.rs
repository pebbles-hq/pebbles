//! F2 — the widget-inspector data layer: the root→deepest render chain under a point,
//! each with its `debug_name`, owning element id, and window-space bounds. Pure; the
//! shell's interactive inspect mode (toggle, hover outline, click-to-print) is built on
//! top of this.

use pebbles_foundation::{Offset, Rect};

use crate::tree::RenderTree;

/// One node in the inspected chain.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectNode {
    /// The render object's `debug_name` (e.g. `"RenderFlex"`).
    pub name: &'static str,
    /// The owning widget-layer element id, if the node is tagged.
    pub source: Option<u64>,
    /// Window-space bounds (logical px).
    pub bounds: Rect,
}

/// The render chain at `point`, root → deepest (the last entry is the topmost hit).
/// Empty when nothing is under the point.
pub fn inspect_at(tree: &RenderTree, point: Offset) -> Vec<InspectNode> {
    tree.hit_test(point)
        .into_iter()
        .map(|id| {
            let o = tree.absolute_offset(id);
            let s = tree.size_of(id);
            InspectNode {
                name: tree.debug_name(id),
                source: tree.source_of(id),
                bounds: Rect::new(o.x, o.y, o.x + s.width, o.y + s.height),
            }
        })
        .collect()
}

/// Format the chain as an indented ancestor tree (what the shell prints on click).
pub fn format_chain(chain: &[InspectNode]) -> String {
    let mut out = String::new();
    for (depth, n) in chain.iter().enumerate() {
        let src = n.source.map(|s| format!(" #{s}")).unwrap_or_default();
        out.push_str(&format!(
            "{:indent$}{}{}  {:.0}×{:.0}\n",
            "",
            n.name,
            src,
            n.bounds.width(),
            n.bounds.height(),
            indent = depth * 2,
        ));
    }
    out
}
