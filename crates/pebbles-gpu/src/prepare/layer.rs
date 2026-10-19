//! Layers (09 §3): the subtree facts (§3.2), the classification (§3.3) and the walk's layer
//! frames. A layer is a pure clip, a folded alpha (§3.4), an offscreen target composited on pop
//! (§3.5), or skipped. Non-`Over` blend paths keep the C17 interim (content skipped) until
//! L21/L24 route them offscreen.

use pebbles_render::paint::display_list::Op;

use super::plan::ClipState;

/// Facts about a layer's subtree (09 §3.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct LayerInfo {
    /// Drawing ops in the subtree, saturating at 2.
    pub(crate) draw_ops: u8,
    /// This layer's own blend is `BlendMode::default()`.
    pub(crate) default_blend: bool,
    /// Every PushLayer nested anywhere in the subtree has `BlendMode::default()`.
    pub(crate) nested_default_blend: bool,
    /// Some PushLayer whose nearest enclosing PushLayer is THIS layer has a non-default blend.
    pub(crate) direct_child_non_default_blend: bool,
}

impl LayerInfo {
    /// A layer with an empty subtree.
    const EMPTY: Self = Self {
        draw_ops: 0,
        default_blend: true,
        nested_default_blend: true,
        direct_child_non_default_blend: false,
    };
}

fn is_draw(op: &Op) -> bool {
    matches!(
        op,
        Op::Fill { .. }
            | Op::Stroke { .. }
            | Op::Image { .. }
            | Op::ImageNine { .. }
            | Op::DynamicImage { .. }
            | Op::BlurredRect { .. }
            | Op::BlurredFill { .. }
            | Op::BlurredGlyphs { .. }
            | Op::Glyphs { .. }
    )
}

/// The §3.2 facts of every PushLayer of `ops`, in walk order (depth-first through fragments).
/// O(n) with one stack of open layers; unbalanced layers close at the end of their fragment.
#[cfg(test)]
pub(crate) fn scan_layers(ops: &[Op], out: &mut Vec<LayerInfo>) {
    let mut open: Vec<usize> = Vec::new();
    scan_into(ops, &mut open, out);
    while !open.is_empty() {
        close(&mut open, out);
    }
}

fn close(open: &mut Vec<usize>, out: &mut [LayerInfo]) {
    let Some(i) = open.pop() else { return };
    if let (Some(&p), Some(&child)) = (open.last(), out.get(i))
        && let Some(parent) = out.get_mut(p)
    {
        parent.draw_ops = (parent.draw_ops + child.draw_ops).min(2);
        parent.nested_default_blend &= child.default_blend && child.nested_default_blend;
    }
}

fn scan_into(ops: &[Op], open: &mut Vec<usize>, out: &mut Vec<LayerInfo>) {
    for op in ops {
        match op {
            Op::PushLayer { blend, .. } => {
                let default = *blend == peniko::BlendMode::default();
                if !default && let Some(parent) = open.last().and_then(|&p| out.get_mut(p)) {
                    parent.direct_child_non_default_blend = true;
                }
                out.push(LayerInfo { default_blend: default, ..LayerInfo::EMPTY });
                open.push(out.len() - 1);
            }
            Op::PopLayer => close(open, out),
            Op::Fragment { ops, .. } => {
                let depth = open.len();
                scan_into(ops, open, out);
                while open.len() > depth {
                    close(open, out);
                }
            }
            op if is_draw(op) => {
                if let Some(info) = open.last().and_then(|&i| out.get_mut(i)) {
                    info.draw_ops = info.draw_ops.saturating_add(1).min(2);
                }
            }
            _ => {}
        }
    }
}

/// The §3.2 facts of the PushLayer at `ops[push]`, from its own subtree (up to the matching pop
/// or the end of `ops`). Computed where the walk meets the layer, so layers inside spliced
/// fragments (COLR glyphs, R29) line up too; O(subtree) per layer.
pub(crate) fn scan_one(ops: &[Op], push: usize) -> LayerInfo {
    let mut out = Vec::new();
    let mut open = Vec::new();
    let Some(first) = ops.get(push..push + 1) else {
        return LayerInfo::EMPTY;
    };
    scan_into(first, &mut open, &mut out);
    for op in ops.get(push + 1..).unwrap_or_default() {
        if open.is_empty() {
            break;
        }
        scan_into(std::slice::from_ref(op), &mut open, &mut out);
    }
    while !open.is_empty() {
        close(&mut open, &mut out);
    }
    out.first().copied().unwrap_or(LayerInfo::EMPTY)
}

/// How a layer is realised (09 §3.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LayerPlan {
    /// Draw nothing until the matching pop.
    Skip,
    /// A clip only (§2); content continues in the same target.
    PureClip,
    /// A clip plus a multiplier on the subtree's single draw (§3.4).
    Fold { alpha: f32 },
    /// Render the subtree into its own target, then composite (§3.5).
    Offscreen { promoted: bool },
}

/// 09 §3.3, verbatim.
pub(crate) fn classify_layer(info: LayerInfo, blend: peniko::BlendMode, alpha: f32) -> LayerPlan {
    let alpha = super::hygiene::sanitize_layer_alpha(alpha).0;
    let default_blend = blend == peniko::BlendMode::default();
    let destructive = blend.is_destructive();
    if !destructive && (alpha <= 0.0 || info.draw_ops == 0) {
        return LayerPlan::Skip;
    }
    if default_blend && alpha >= 1.0 {
        return if info.direct_child_non_default_blend {
            LayerPlan::Offscreen { promoted: true }
        } else {
            LayerPlan::PureClip
        };
    }
    if default_blend && info.draw_ops <= 1 && info.nested_default_blend {
        return LayerPlan::Fold { alpha };
    }
    LayerPlan::Offscreen { promoted: false }
}

/// One open layer in the walk; its pop undoes exactly what its push did.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Frame {
    /// A skipped layer (or a non-finite / hidden one): one culled clip frame.
    Skip,
    /// A pure clip or a fold: one clip frame; `prev_fold` restores the fold alpha.
    Clip { prev_fold: f32 },
    /// The C17 interim for non-`Over` blend paths: one clip frame, content skipped.
    Interim { prev_fold: f32 },
    /// An offscreen layer: its own clip frame on the parent (`comp` = the composite's clip
    /// state) plus a fresh frame for the layer target.
    Offscreen { layer: u32, comp: ClipState, prev_fold: f32 },
}

/// The open layer frames, the fold alpha (§3.4) and the skipped depth.
#[derive(Default)]
pub(crate) struct LayerStack {
    frames: Vec<Frame>,
    skipped: u32,
    fold: f32,
}

impl LayerStack {
    pub(crate) fn reset(&mut self) {
        self.frames.clear();
        self.skipped = 0;
        self.fold = 1.0;
    }

    /// Open entries (tests: the stack is balanced after a walk).
    #[cfg(test)]
    pub(crate) fn depth(&self) -> usize {
        self.frames.len()
    }

    pub(crate) fn push(&mut self, f: Frame) {
        self.skipped += u32::from(matches!(f, Frame::Skip | Frame::Interim { .. }));
        self.frames.push(f);
    }

    pub(crate) fn pop(&mut self) -> Option<Frame> {
        let f = self.frames.pop()?;
        self.skipped -= u32::from(matches!(f, Frame::Skip | Frame::Interim { .. }));
        Some(f)
    }

    /// Whether the current position is inside a skipped layer.
    pub(crate) fn is_skipped(&self) -> bool {
        self.skipped > 0
    }

    /// The fold alpha (§3.4): the product of the open `Fold` layers' alphas in this target.
    pub(crate) fn fold(&self) -> f32 {
        self.fold
    }

    pub(crate) fn set_fold(&mut self, a: f32) {
        self.fold = a;
    }

    pub(crate) fn open(&self) -> usize {
        self.frames.len()
    }

    /// The innermost open offscreen layer (the current target), or `None` for the root.
    pub(crate) fn current_target(&self) -> Option<u32> {
        self.frames.iter().rev().find_map(|f| match f {
            Frame::Offscreen { layer, .. } => Some(*layer),
            _ => None,
        })
    }
}

#[cfg(test)]
#[path = "layer_tests.rs"]
mod tests;
