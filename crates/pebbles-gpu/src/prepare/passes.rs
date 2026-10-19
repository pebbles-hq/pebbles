//! Pass segmentation (09 §3.6): the ordered cmds split into render-pass segments per target,
//! never reordered (DR-04). Pure CPU; unit-tested without a device.

use super::plan::{DrawCmd, FramePlan, TargetRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SegmentLoad {
    Clear,
    Load,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AfterPass {
    None,
    /// Copy the parent's image under the layer bounds into a backdrop texture (09 §4.4, L23).
    CopyBackdrop(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PassSegment {
    pub target: TargetRef,
    /// Indices into `plan.cmds`; never contains `PushLayer` / `PopLayer`.
    pub cmds: std::ops::Range<u32>,
    pub color_load: SegmentLoad,
    /// The target's last pass this frame.
    pub is_final: bool,
    /// Resolve MSAA at the end of this pass (final pass, or a backdrop capture).
    pub resolve: bool,
    pub after: AfterPass,
}

struct Open {
    target: TargetRef,
    started: bool,
    start: u32,
}

/// 09 §3.6: split `plan.cmds` into segments.
pub(crate) fn segment_passes(plan: &FramePlan, out: &mut Vec<PassSegment>) {
    out.clear();
    let mut stack = vec![Open { target: TargetRef::Root, started: false, start: 0 }];
    let close = |o: &mut Open,
                 end: u32,
                 is_final: bool,
                 resolve: bool,
                 after: AfterPass,
                 out: &mut Vec<PassSegment>| {
        let load = if o.started { SegmentLoad::Load } else { SegmentLoad::Clear };
        let empty = o.start >= end;
        // Drop only a pure no-op segment.
        if !(empty && load == SegmentLoad::Load && !resolve && after == AfterPass::None) {
            out.push(PassSegment {
                target: o.target,
                cmds: o.start..end,
                color_load: load,
                is_final,
                resolve,
                after,
            });
        }
        o.started = true;
    };
    for (i, cmd) in plan.cmds.iter().enumerate() {
        let i = u32::try_from(i).unwrap_or(u32::MAX);
        match cmd {
            DrawCmd::PushLayer { layer } => {
                let backdrop = plan
                    .layers
                    .get(*layer as usize)
                    .is_some_and(|l| l.path == super::plan::CompositePath::Backdrop);
                let after = if backdrop { AfterPass::CopyBackdrop(*layer) } else { AfterPass::None };
                if let Some(top) = stack.last_mut() {
                    close(top, i, false, backdrop, after, out);
                }
                stack.push(Open { target: TargetRef::Layer(*layer), started: false, start: i + 1 });
            }
            DrawCmd::PopLayer { .. } => {
                if let Some(mut top) = stack.pop() {
                    close(&mut top, i, true, true, AfterPass::None, out);
                }
                if let Some(parent) = stack.last_mut() {
                    parent.start = i + 1;
                }
            }
            _ => {}
        }
    }
    let end = u32::try_from(plan.cmds.len()).unwrap_or(u32::MAX);
    while let Some(mut top) = stack.pop() {
        close(&mut top, end, true, true, AfterPass::None, out);
        if let Some(parent) = stack.last_mut() {
            parent.start = end;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prepare::plan::{ClipState, CompositePath, LayerDesc, ScissorRect};

    fn plan(cmds: Vec<DrawCmd>, layers: usize) -> FramePlan {
        let mut p = FramePlan::default();
        p.clear();
        p.cmds = cmds;
        for _ in 0..layers {
            p.layers.push(LayerDesc {
                bounds: ScissorRect { x0: 0, y0: 0, x1: 8, y1: 8 },
                alpha: 0.5,
                mix: peniko::Mix::Normal,
                compose: peniko::Compose::SrcOver,
                path: CompositePath::Over,
                parent: TargetRef::Root,
                effect: None,
            });
        }
        p
    }

    fn solid() -> DrawCmd {
        DrawCmd::Solid { indices: 0..3, clip: ClipState::full((8, 8)) }
    }

    fn summary(s: &[PassSegment]) -> Vec<(TargetRef, SegmentLoad, bool, bool)> {
        s.iter().map(|x| (x.target, x.color_load, x.is_final, x.resolve)).collect()
    }

    #[test]
    fn no_layers_single_root_segment() {
        let mut out = Vec::new();
        segment_passes(&plan(vec![solid(), solid()], 0), &mut out);
        assert_eq!(summary(&out), vec![(TargetRef::Root, SegmentLoad::Clear, true, true)]);
        assert_eq!(out[0].cmds, 0..2);
    }

    #[test]
    fn over_layer_splits_root() {
        let cmds = vec![
            solid(),
            DrawCmd::PushLayer { layer: 0 },
            solid(),
            DrawCmd::PopLayer { layer: 0 },
            DrawCmd::Composite { layer: 0, clip: ClipState::full((8, 8)) },
        ];
        let mut out = Vec::new();
        segment_passes(&plan(cmds, 1), &mut out);
        assert_eq!(
            summary(&out),
            vec![
                (TargetRef::Root, SegmentLoad::Clear, false, false),
                (TargetRef::Layer(0), SegmentLoad::Clear, true, true),
                (TargetRef::Root, SegmentLoad::Load, true, true),
            ]
        );
        assert_eq!(out[2].cmds, 4..5, "the composite runs in the resumed root pass");
    }

    #[test]
    fn nested_layers_order() {
        let cmds = vec![
            DrawCmd::PushLayer { layer: 0 },
            DrawCmd::PushLayer { layer: 1 },
            solid(),
            DrawCmd::PopLayer { layer: 1 },
            DrawCmd::Composite { layer: 1, clip: ClipState::full((8, 8)) },
            DrawCmd::PopLayer { layer: 0 },
            DrawCmd::Composite { layer: 0, clip: ClipState::full((8, 8)) },
        ];
        let mut out = Vec::new();
        segment_passes(&plan(cmds, 2), &mut out);
        let targets: Vec<TargetRef> = out.iter().map(|s| s.target).collect();
        assert_eq!(
            targets,
            vec![
                TargetRef::Root,
                TargetRef::Layer(0),
                TargetRef::Layer(1),
                TargetRef::Layer(0),
                TargetRef::Root
            ]
        );
        assert_eq!(out[3].color_load, SegmentLoad::Load);
        assert!(out[3].is_final && out[4].is_final);
    }

    #[test]
    fn empty_load_segment_dropped() {
        // Two sibling layers back to back: the root's empty Load segment between them is dropped.
        let cmds = vec![
            DrawCmd::PushLayer { layer: 0 },
            solid(),
            DrawCmd::PopLayer { layer: 0 },
            DrawCmd::PushLayer { layer: 1 },
            solid(),
            DrawCmd::PopLayer { layer: 1 },
        ];
        let mut out = Vec::new();
        segment_passes(&plan(cmds, 2), &mut out);
        let root: Vec<_> = out.iter().filter(|s| s.target == TargetRef::Root).collect();
        assert_eq!(root.len(), 2, "the leading Clear and the final pass; the empty Load between is dropped");
    }

    #[test]
    fn backdrop_layer_resolves_and_copies_parent() {
        let mut p = plan(
            vec![
                DrawCmd::PushLayer { layer: 0 },
                DrawCmd::PopLayer { layer: 0 },
                DrawCmd::Composite { layer: 0, clip: ClipState::full((8, 8)) },
            ],
            1,
        );
        p.layers[0].path = CompositePath::Backdrop;
        let mut out = Vec::new();
        segment_passes(&p, &mut out);
        assert_eq!(out[0].target, TargetRef::Root);
        assert!(out[0].resolve);
        assert_eq!(out[0].after, AfterPass::CopyBackdrop(0));
        assert_eq!(out[1].target, TargetRef::Layer(0));
        assert_eq!(out[2].color_load, SegmentLoad::Load);
    }
}
