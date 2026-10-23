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
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
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
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
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
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(is_white(pixel(&rgba, 64, 30, 10)));
    let stats = renderer.last_frame_stats();
    assert_eq!(stats.unsupported[Unsupported::CustomUnregistered as usize], 1);
    assert_eq!(stats.unsupported_kinds(), [Unsupported::CustomUnregistered], "S50: exact key set");
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

/// A 16×16 texture filled by a GPU render pass with premultiplied `color`.
fn gpu_texture(device: &wgpu::Device, queue: &wgpu::Queue, color: wgpu::Color) -> wgpu::TextureView {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("test.gpu_image"),
        size: wgpu::Extent3d { width: 16, height: 16, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("test.gpu_image") });
    drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("test.gpu_image.fill"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &view,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations { load: wgpu::LoadOp::Clear(color), store: wgpu::StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    }));
    queue.submit([encoder.finish()]);
    view
}

#[test]
#[ignore = "gpu"]
fn gpu_image_composites_with_alpha() {
    let Some((device, queue, mut renderer)) = setup("gpu_image_composites_with_alpha") else { return };
    // Premultiplied half-transparent blue.
    let view = gpu_texture(&device, &queue, wgpu::Color { r: 0.0, g: 0.0, b: 0.5, a: 0.5 });
    let id = renderer.register_texture(view, (16, 16));
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(renderer.texture_size(id), Some((16, 16)));
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let s = peniko::ImageSampler::default();
    p.draw_gpu_image(id, Affine::IDENTITY, Rect::new(8.0, 8.0, 24.0, 24.0), 1.0, s);
    p.draw_gpu_image(id, Affine::translate((32.0, 0.0)), Rect::new(8.0, 8.0, 24.0, 24.0), 0.5, s);
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (64, 32), Color::WHITE);
    let near = |c: [u8; 4], e: [u8; 3]| (0..3).all(|k| (i32::from(c[k]) - i32::from(e[k])).abs() <= 2);
    let c = pixel(&rgba, 64, 16, 16);
    assert!(near(c, [128, 128, 255]), "half-blue over white: {c:?}");
    let c = pixel(&rgba, 64, 48, 16);
    assert!(near(c, [191, 191, 255]), "quarter-blue over white: {c:?}");
    assert!(is_white(pixel(&rgba, 64, 4, 16)) && is_white(pixel(&rgba, 64, 26, 16)), "placed at the rect");
    assert_eq!(renderer.last_frame_stats().unsupported[Unsupported::GpuTextureUnregistered as usize], 0);
}

#[test]
#[ignore = "gpu"]
fn gpu_image_unknown_and_unregistered_are_skipped() {
    let Some((device, queue, mut renderer)) = setup("gpu_image_unknown_and_unregistered") else { return };
    let view = gpu_texture(&device, &queue, wgpu::Color::BLACK);
    let id = renderer.register_texture(view, (16, 16));
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert!(renderer.unregister_texture(id));
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    let s = peniko::ImageSampler::default();
    p.draw_gpu_image(id, Affine::IDENTITY, Rect::new(0.0, 0.0, 16.0, 16.0), 1.0, s);
    p.draw_gpu_image(
        pebbles_gpu::TextureId(9_999),
        Affine::IDENTITY,
        Rect::new(16.0, 0.0, 32.0, 16.0),
        1.0,
        s,
    );
    let rgba = render_scene(&device, &queue, &mut renderer, &scene, (32, 16), Color::WHITE);
    assert!(is_white(pixel(&rgba, 32, 8, 8)) && is_white(pixel(&rgba, 32, 24, 8)), "nothing drawn");
    assert_eq!(renderer.last_frame_stats().unsupported[Unsupported::GpuTextureUnregistered as usize], 2);
    assert_eq!(
        renderer.last_frame_stats().unsupported_kinds(),
        [Unsupported::GpuTextureUnregistered],
        "S50: exact key set"
    );
}

/// Paint data for the depth test: two overlapping quads at different depths.
struct Tris;

impl CustomPaint for Tris {
    fn bounds(&self) -> Rect {
        Rect::new(0.0, 0.0, 64.0, 32.0)
    }
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Draws a near red quad (z 0.2), then a far blue quad (z 0.8) overlapping it: drawn later,
/// blue would win the overlap without the depth test.
#[derive(Default)]
struct DepthTris {
    pipeline: Option<wgpu::RenderPipeline>,
    saw_depth: bool,
}

const DEPTH_WGSL: &str = "
struct V { @builtin(position) p: vec4<f32>, @location(0) c: vec4<f32> }
@vertex fn vs(@builtin(vertex_index) i: u32, @builtin(instance_index) k: u32) -> V {
    // Instance 0: red at z 0.2 over x ∈ [-1, 0.25]; instance 1: blue at z 0.8 over x ∈ [-0.25, 1].
    let corner = vec2<f32>(f32(i & 1u), f32((i >> 1u) & 1u));
    var o: V;
    if k == 0u {
        o.p = vec4<f32>(mix(-1.0, 0.25, corner.x), corner.y * 2.0 - 1.0, 0.2, 1.0);
        o.c = vec4<f32>(1.0, 0.0, 0.0, 1.0);
    } else {
        o.p = vec4<f32>(mix(-0.25, 1.0, corner.x), corner.y * 2.0 - 1.0, 0.8, 1.0);
        o.c = vec4<f32>(0.0, 0.0, 1.0, 1.0);
    }
    return o;
}
@fragment fn fs(v: V) -> @location(0) vec4<f32> { return v.c; }
";

impl CustomRenderer for DepthTris {
    fn prepare(&mut self, _ctx: &mut CustomPrepare<'_>, _paint: &dyn CustomPaint) {}

    fn render(&mut self, ctx: &mut CustomRender<'_>, _paint: &dyn CustomPaint) {
        let Some(depth) = ctx.depth else { return };
        self.saw_depth = true;
        let pipeline = self.pipeline.get_or_insert_with(|| {
            let module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("test.custom.depth"),
                source: wgpu::ShaderSource::Wgsl(DEPTH_WGSL.into()),
            });
            ctx.device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("test.custom.depth"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleStrip,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: Some(true),
                    depth_compare: Some(wgpu::CompareFunction::Less),
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState { count: ctx.sample_count, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(ctx.format.into())],
                }),
                multiview_mask: None,
                cache: None,
            })
        });
        let mut pass = ctx.encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("test.custom.depth"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: ctx.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: depth,
                depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Discard }),
                stencil_ops: None,
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        let (x, y, w, h) = ctx.scissor;
        pass.set_scissor_rect(x, y, w, h);
        pass.set_pipeline(pipeline);
        pass.draw(0..4, 0..1);
        pass.draw(0..4, 1..2);
    }
}

fn render_with(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    renderer: &mut Renderer,
    targets: &mut FrameTargets,
    scene: &Scene,
    size: (u32, u32),
) -> Vec<u8> {
    let texture = target_texture(device, size.0, size.1);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let target = RenderTarget::new(&view, wgpu::TextureFormat::Rgba8Unorm, targets);
    renderer.render(target, scene, &FrameParams::new(Color::WHITE)).expect("render");
    readback(device, queue, &texture)
}

#[test]
#[ignore = "gpu"]
fn depth_nearer_triangle_wins_in_order_with_2d() {
    let Some((device, queue, mut renderer)) = setup("depth_nearer_triangle_wins") else { return };
    renderer.register_custom_with_depth::<Tris>(
        Box::new(DepthTris::default()),
        wgpu::TextureFormat::Depth32Float,
    );
    let mut targets = FrameTargets::new();
    let mut scene = Scene::new();
    let mut p = Painter::new(&mut scene);
    fill(&mut p, GREEN, Rect::new(0.0, 0.0, 64.0, 32.0));
    p.draw_custom(Rect::new(0.0, 0.0, 64.0, 32.0), Affine::IDENTITY, Arc::new(Tris));
    fill(&mut p, GREEN, Rect::new(56.0, 0.0, 64.0, 32.0));
    let rgba = render_with(&device, &queue, &mut renderer, &mut targets, &scene, (64, 32));
    // Red covers x < 40, blue x > 24; the overlap 24..40 must be red (nearer).
    if support::noop_backend() {
        return; // V09: noop renders no pixels
    }
    assert_eq!(pixel(&rgba, 64, 10, 16), [255, 0, 0, 255], "red alone");
    assert_eq!(pixel(&rgba, 64, 32, 16), [255, 0, 0, 255], "nearer red wins the overlap");
    assert_eq!(pixel(&rgba, 64, 48, 16), [0, 0, 255, 255], "blue alone");
    assert_eq!(pixel(&rgba, 64, 60, 16), [0, 255, 0, 255], "2D after the custom op draws on top");
    assert_eq!(targets.depth_targets(), 1);
}

#[test]
#[ignore = "gpu"]
fn depth_not_allocated_without_request() {
    let Some((device, queue, mut renderer)) = setup("depth_not_allocated_without_request") else { return };
    renderer.register_custom::<Flat>(Box::new(BlueFill::default()));
    let mut targets = FrameTargets::new();
    let mut scene = Scene::new();
    let b = Rect::new(0.0, 0.0, 32.0, 16.0);
    Painter::new(&mut scene).draw_custom(b, Affine::IDENTITY, Arc::new(Flat(b)));
    render_with(&device, &queue, &mut renderer, &mut targets, &scene, (32, 16));
    assert_eq!(targets.depth_targets(), 0);
    assert_eq!(targets.frame_target_bytes(), 0);
}
