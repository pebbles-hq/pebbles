//! Custom primitives (R60, 02 §11): painter's order across the custom pass, unregistered types.

use std::sync::Arc;

use pebbles_gpu::{CustomPaint, CustomPrepare, CustomRender, CustomRenderer, Unsupported};

use super::*;

/// Paint data: fill the op's scissor with one premultiplied color.
struct Flat(Rect);

impl CustomPaint for Flat {
    fn bounds(&self) -> Rect {
        self.0
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

struct Unregistered;

impl CustomPaint for Unregistered {
    fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, 60.0, 20.0)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Draws a full-target triangle in blue, restricted to `ctx.scissor`.
#[derive(Default)]
struct BlueFill {
    pipeline: Option<(wgpu::TextureFormat, u32, wgpu::RenderPipeline)>,
    prepared: u32,
}

const BLUE_WGSL: &str = "
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs() -> @location(0) vec4<f32> { return vec4<f32>(0.0, 0.0, 1.0, 1.0); }
";

impl CustomRenderer for BlueFill {
    fn prepare(&mut self, _ctx: &mut CustomPrepare<'_>, _paint: &dyn CustomPaint) {
        self.prepared += 1;
    }

    fn render(&mut self, ctx: &mut CustomRender<'_>, _paint: &dyn CustomPaint) {
        if self.pipeline.as_ref().is_none_or(|(f, s, _)| *f != ctx.format || *s != ctx.sample_count) {
            let module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("test.custom.blue"),
                source: wgpu::ShaderSource::Wgsl(BLUE_WGSL.into()),
            });
            let pipeline = ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("test.custom.blue"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState { count: ctx.sample_count, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(ctx.format.into())],
                }),
                multiview_mask: None,
                cache: None,
            });
            self.pipeline = Some((ctx.format, ctx.sample_count, pipeline));
        }
        let Some((_, _, pipeline)) = &self.pipeline else { return };
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test.custom.pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (x, y, w, h) = ctx.scissor;
        pass.set_scissor_rect(x, y, w, h);
        pass.set_pipeline(pipeline);
        pass.draw(0..3, 0..1);
    }
}

fn fill(p: &mut Painter<'_>, c: Color, r: Rect) {
    p.fill(Fill::NonZero, Affine::IDENTITY, c, None, &r);
}

const RED: Color = Color::from_rgb8(255, 0, 0);
const GREEN: Color = Color::from_rgb8(0, 255, 0);

#[test]
#[ignore = "gpu"]
fn custom_renders_in_painter_order() {
    let Some((device, queue, mut renderer)) = setup("custom_renders_in_painter_order") else { return };
    renderer.register_custom::<Flat>(Box::new(BlueFill::default()));
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    fill(&mut p, RED, Rect::new(0.0, 0.0, 60.0, 20.0));
    let b = Rect::new(20.0, 0.0, 60.0, 20.0);
    p.draw_custom(b, Affine::IDENTITY, Arc::new(Flat(b)));
    fill(&mut p, GREEN, Rect::new(40.0, 0.0, 60.0, 20.0));
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 24), Color::WHITE);
    assert_eq!(pixel(&rgba, 64, 10, 10), [255, 0, 0, 255], "red under nothing");
    assert_eq!(pixel(&rgba, 64, 30, 10), [0, 0, 255, 255], "blue over red");
    assert_eq!(pixel(&rgba, 64, 50, 10), [0, 255, 0, 255], "green over blue");
    assert!(is_white(pixel(&rgba, 64, 62, 10)), "scissor honored");
    let stats = renderer.last_frame_stats();
    assert_eq!(stats.render_passes, 2, "the root pass splits around the custom op");
}

#[test]
#[ignore = "gpu"]
fn custom_inside_translucent_layer_gets_group_opacity() {
    let Some((device, queue, mut renderer)) = setup("custom_inside_translucent_layer") else { return };
    renderer.register_custom::<Flat>(Box::new(BlueFill::default()));
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let b = Rect::new(8.0, 4.0, 40.0, 20.0);
    p.push_layer(Fill::NonZero, peniko::Mix::Normal, 0.5, Affine::IDENTITY, &Rect::new(0.0, 0.0, 64.0, 24.0));
    p.draw_custom(b, Affine::IDENTITY, Arc::new(Flat(b)));
    p.pop_layer();
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 24), Color::WHITE);
    let c = pixel(&rgba, 64, 20, 10);
    assert!((i32::from(c[0]) - 128).abs() <= 2 && c[2] == 255, "half-transparent blue over white: {c:?}");
}

#[test]
#[ignore = "gpu"]
fn custom_unregistered_is_skipped_and_counted() {
    let Some((device, queue, mut renderer)) = setup("custom_unregistered_is_skipped_and_counted") else {
        return;
    };
    renderer.register_custom::<Flat>(Box::new(BlueFill::default()));
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    p.draw_custom(Rect::new(0.0, 0.0, 60.0, 20.0), Affine::IDENTITY, Arc::new(Unregistered));
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 24), Color::WHITE);
    assert!(is_white(pixel(&rgba, 64, 30, 10)));
    let stats = renderer.last_frame_stats();
    assert_eq!(stats.unsupported[Unsupported::CustomUnregistered as usize], 1);
}

#[test]
#[ignore = "gpu"]
fn custom_free_frame_keeps_one_pass() {
    let Some((device, queue, mut renderer)) = setup("custom_free_frame_keeps_one_pass") else { return };
    renderer.register_custom::<Flat>(Box::new(BlueFill::default()));
    let mut scene = Scene::new();
    fill(&mut Painter::new(&mut scene), RED, Rect::new(0.0, 0.0, 60.0, 20.0));
    render_scene(&device, &queue, &mut renderer, &scene, (64, 24), Color::WHITE);
    assert_eq!(renderer.last_frame_stats().render_passes, 1);
}
