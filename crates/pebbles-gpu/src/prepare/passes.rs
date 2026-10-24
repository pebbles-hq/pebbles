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
    /// Run the custom primitive `plan.cmds[i]` (R60) between this pass and the next.
    Custom(u32),
    /// Submit, then upload plan segment `k` before the next pass (M07).
    NextSegment(u32),
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
    /// M33 ([`hoist_layer_passes`]): draw into the render pass of the previous segment (same
    /// target) instead of opening a new one.
    pub continues: bool,
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
                continues: false,
            });
        }
        o.started = true;
    };
    let mut next_seg = 1;
    for (i, cmd) in plan.cmds.iter().enumerate() {
        // M07: a plan-segment boundary ends the current pass (reopened with Load after the
        // resubmit).
        if plan.segments.get(next_seg).is_some_and(|s| s.cmds.start == i) {
            let i = u32::try_from(i).unwrap_or(u32::MAX);
            let k = u32::try_from(next_seg).unwrap_or(u32::MAX);
            if let Some(top) = stack.last_mut() {
                close(top, i, false, false, AfterPass::NextSegment(k), out);
                top.start = i;
            }
            next_seg += 1;
        }
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
            DrawCmd::Custom { .. } => {
                // R60: end the pass, let the custom renderer record its own, reopen with Load.
                if let Some(top) = stack.last_mut() {
                    close(top, i, false, false, AfterPass::Custom(i), out);
                    top.start = i + 1;
                }
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

/// M33: render every offscreen layer's segments before its parent's, each target in ONE pass.
///
/// `segment_passes` interleaves targets in painter's order: the parent's pass ends at a push,
/// the layer renders, and the parent resumes with `Load` — 2 extra passes per layer, each a
/// full pass begin/end on the GPU and a command-buffer switch on the CPU. A layer's content
/// never reads its parent (only `Backdrop` layers do, and they are excluded), so every target
/// can instead be rendered whole, in the order the targets are finished: a child's final
/// segment precedes its parent's composite of it, so children still complete before they are
/// sampled. Each target receives exactly the same draws in the same order — the pixels are
/// unchanged; only the order *between* targets moves, and the resumed passes disappear.
///
/// Applied only when every segment is a plain one (`AfterPass::None`: no backdrop copy,
/// custom op or arena resubmit between passes) and `hoistable(layer)` holds for every layer
/// (the renderer checks that no two layers share a pool slot, and no MSAA or effect passes).
/// Returns whether the segments were rewritten.
pub(crate) fn hoist_layer_passes(segs: &mut Vec<PassSegment>, hoistable: impl Fn(u32) -> bool) -> bool {
    let layered = segs.iter().any(|s| matches!(s.target, TargetRef::Layer(_)));
    if !layered
        || segs
            .iter()
            .any(|s| s.after != AfterPass::None || matches!(s.target, TargetRef::Layer(l) if !hoistable(l)))
    {
        return false;
    }
    // Targets in the order of their final segment.
    let finals: Vec<TargetRef> = segs.iter().filter(|s| s.is_final).map(|s| s.target).collect();
    if finals.len() != segs.iter().filter(|s| s.is_final).count()
        || finals.iter().enumerate().any(|(i, t)| finals.get(..i).is_some_and(|before| before.contains(t)))
    {
        return false;
    }
    let mut out = Vec::with_capacity(segs.len());
    for t in finals {
        let mut first = true;
        for s in segs.iter().filter(|s| s.target == t) {
            out.push(PassSegment { continues: !first, ..s.clone() });
            first = false;
        }
    }
    if out.len() != segs.len() {
        return false;
    }
    *segs = out;
    true
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
    fn hoisted_layers_render_whole_before_their_parent() {
        let cmds = vec![
            solid(),
            DrawCmd::PushLayer { layer: 0 },
            solid(),
            DrawCmd::PushLayer { layer: 1 },
            solid(),
            DrawCmd::PopLayer { layer: 1 },
            DrawCmd::Composite { layer: 1, clip: ClipState::full((8, 8)) },
            solid(),
            DrawCmd::PopLayer { layer: 0 },
            DrawCmd::Composite { layer: 0, clip: ClipState::full((8, 8)) },
            solid(),
        ];
        let mut out = Vec::new();
        segment_passes(&plan(cmds, 2), &mut out);
        assert_eq!(out.len(), 5);
        let mut never = out.clone();
        assert!(!hoist_layer_passes(&mut never, |_| false), "a non-hoistable layer keeps the order");
        assert_eq!(never, out);
        assert!(hoist_layer_passes(&mut out, |_| true));
        let got: Vec<_> = out.iter().map(|s| (s.target, s.cmds.clone(), s.color_load, s.continues)).collect();
        assert_eq!(
            got,
            vec![
                (TargetRef::Layer(1), 4..5, SegmentLoad::Clear, false),
                (TargetRef::Layer(0), 2..3, SegmentLoad::Clear, false),
                (TargetRef::Layer(0), 6..8, SegmentLoad::Load, true),
                (TargetRef::Root, 0..1, SegmentLoad::Clear, false),
                (TargetRef::Root, 9..11, SegmentLoad::Load, true),
            ]
        );
        // Without layers there is nothing to hoist.
        let mut flat = Vec::new();
        segment_passes(&plan(vec![solid()], 0), &mut flat);
        assert!(!hoist_layer_passes(&mut flat, |_| true));
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

    #[derive(Debug)]
    struct Probe;
    impl pebbles_render::paint::display_list::CustomPaint for Probe {
        fn bounds(&self) -> kurbo::Rect {
            kurbo::Rect::new(0.0, 0.0, 8.0, 8.0)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn custom_cmd_keeps_order() {
        use pebbles_render::paint::{Painter, Scene};
        let mut scene = Scene::new();
        let mut p = Painter::new(&mut scene);
        let r = kurbo::Rect::new(0.0, 0.0, 8.0, 8.0);
        p.fill(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, peniko::Color::BLACK, None, &r);
        p.draw_custom(r, kurbo::Affine::IDENTITY, std::sync::Arc::new(Probe));
        p.fill(peniko::Fill::NonZero, kurbo::Affine::IDENTITY, peniko::Color::BLACK, None, &r);
        let (plan, _) = crate::prepare::test_util::prepare_scene(&scene, 16, 16);
        assert_eq!(crate::prepare::test_util::cmd_kinds(&plan), ["shapes", "custom", "shapes"]);
        assert_eq!(plan.customs.len(), 1);
        assert_eq!(plan.customs[0].scissor, ScissorRect { x0: 0, y0: 0, x1: 8, y1: 8 });
        let mut out = Vec::new();
        segment_passes(&plan, &mut out);
        assert_eq!(
            summary(&out),
            vec![
                (TargetRef::Root, SegmentLoad::Clear, false, false),
                (TargetRef::Root, SegmentLoad::Load, true, true)
            ]
        );
        assert_eq!(out[0].after, AfterPass::Custom(1));
        assert_eq!((out[0].cmds.clone(), out[1].cmds.clone()), (0..1, 2..3));
    }

    #[test]
    fn no_custom_ops_same_segments() {
        let mut out = Vec::new();
        segment_passes(&plan(vec![solid(), solid()], 0), &mut out);
        assert!(out.iter().all(|s| s.after == AfterPass::None));
    }
}
