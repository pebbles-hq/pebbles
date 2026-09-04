//! The sizing tier: FittedBox (BoxFit), FractionallySizedBox, IntrinsicWidth/
//! IntrinsicHeight, LimitedBox, OverflowBox — layout probes through a real `Ui`,
//! plus a sweep-gradient paint smoke test. Copy of the house frame loop.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Alignment, BoxFit, Point, Size, palette};
use pebbles_render::{
    BorderRadius, BoxDecoration, Gradient, RenderFittedBox, RenderFractionallySizedBox,
    RenderIntrinsicHeight, RenderIntrinsicWidth, RenderLimitedBox, RenderOverflowBox,
    RenderParagraph, TextEnv,
};
use pebbles_widgets::{column, container, fitted_box, fractionally_sized_box, intrinsic_height, intrinsic_width, limited_box, overflow_box, row, SizedBox, text, View};

fn probe_size<T: pebbles_render::RenderObject>(ui: &Ui) -> Size {
    let id = ui.render_tree().find::<T>().expect("render object present");
    ui.render_tree().size_of(id)
}

fn transform_of<T: pebbles_render::RenderObject>(ui: &Ui, size: Size) -> Option<pebbles_render::Affine> {
    let id = ui.render_tree().find::<T>().unwrap();
    ui.render_tree().object_ref(id).transform(size)
}

#[test]
fn fitted_box_contain_scales_a_large_child_into_a_tight_box() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![SizedBox::exact(
                    100.0,
                    100.0,
                    fitted_box(SizedBox::exact(200.0, 100.0, text("x".to_string()))),
                )
                .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(200.0, 200.0));

    // Tight constraints: the box itself fills 100×100 (Flutter parity).
    assert_eq!(probe_size::<RenderFittedBox>(&ui), Size::new(100.0, 100.0));
    // Contain scale = min(100/200, 100/100) = 0.5; the 200×100 child lands at
    // 100×50, centered vertically → paint transform moves it down 25px.
    let t = transform_of::<RenderFittedBox>(&ui, Size::new(100.0, 100.0)).expect("a scale is applied");
    let p = t * Point::new(0.0, 0.0);
    assert!(p.x.abs() < 1e-6 && (p.y - 25.0).abs() < 1e-6, "centered contain: {p:?}");
    // The scaled child's top-right maps to the box's top-right (100, 25).
    let q = t * Point::new(200.0, 100.0);
    assert!((q.x - 100.0).abs() < 1e-6 && (q.y - 75.0).abs() < 1e-6, "scale 0.5 both axes: {q:?}");
}

#[test]
fn fitted_box_fill_scales_each_axis_independently() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![SizedBox::exact(
                    100.0,
                    100.0,
                    fitted_box(SizedBox::exact(200.0, 100.0, text("x".to_string()))).fit(BoxFit::Fill),
                )
                .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(200.0, 200.0));
    // Fill: 200×100 → 100×100 with per-axis scales (0.5, 1.0), no offset.
    let t = transform_of::<RenderFittedBox>(&ui, Size::new(100.0, 100.0)).expect("fill scales");
    let q = t * Point::new(200.0, 0.0);
    assert!((q.x - 100.0).abs() < 1e-6 && q.y.abs() < 1e-6, "x scaled by 0.5 only: {q:?}");
}

#[test]
fn fitted_box_scale_down_never_grows_a_small_child() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![SizedBox::exact(
                    100.0,
                    100.0,
                    fitted_box(SizedBox::exact(40.0, 40.0, text("y".to_string())))
                        .fit(BoxFit::ScaleDown),
                )
                .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(200.0, 200.0));
    // ScaleDown = min(1, contain) → no scaling; the 40×40 child is merely
    // centered (a pure translate of (30, 30) inside the 100×100 box).
    let t = transform_of::<RenderFittedBox>(&ui, Size::new(100.0, 100.0))
        .expect("centering offset is applied");
    let p = t * Point::new(0.0, 0.0);
    assert!((p.x - 30.0).abs() < 1e-6 && (p.y - 30.0).abs() < 1e-6, "centered, unscaled: {p:?}");
    let q = t * Point::new(40.0, 40.0);
    assert!((q.x - 70.0).abs() < 1e-6 && (q.y - 70.0).abs() < 1e-6, "no scaling applied: {q:?}");
}

#[test]
fn fractionally_sized_box_takes_a_fraction_and_aligns() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                // Loose constraints so the fractions take effect (under a tight
                // parent, `enforce` correctly overrides them — Flutter parity).
                column(vec![pebbles_widgets::constrained_box(
                    pebbles_render::BoxConstraints::loose(Size::new(200.0, 100.0)),
                    fractionally_sized_box(text("x".to_string()))
                        .width_factor(0.5)
                        .height_factor(0.5)
                        .alignment(Alignment::TOP_RIGHT),
                )
                .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert_eq!(probe_size::<RenderFractionallySizedBox>(&ui), Size::new(200.0, 100.0));
    // The child is tight 100×50 (half of each axis).
    assert_eq!(probe_size::<RenderParagraph>(&ui), Size::new(100.0, 50.0));
    // …positioned top-right: a pure translate of (100, 0).
    let t = transform_of::<RenderFractionallySizedBox>(&ui, Size::new(200.0, 100.0))
        .expect("offset is applied");
    let p = t * Point::new(0.0, 0.0);
    assert!((p.x - 100.0).abs() < 1e-6 && p.y.abs() < 1e-6, "top-right offset: {p:?}");
}

#[test]
fn intrinsic_width_shrink_wraps_a_column_to_its_widest_word() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                row(vec![intrinsic_width(
                    column(vec![
                        text("Alpha".to_string()).into_widget(),
                        text("Beta".to_string()).into_widget(),
                        text("Omega-3".to_string()).into_widget(),
                    ]),
                )
                .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(400.0, 200.0));
    let w = probe_size::<RenderIntrinsicWidth>(&ui).width;
    assert!(w > 0.0 && w < 100.0, "shrink-wraps to the widest word, not the 400px row: {w}");
}

#[test]
fn intrinsic_height_is_the_tallest_sibling() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![intrinsic_height(
                    row(vec![
                        SizedBox::square(40.0, container()).into_widget(),
                        SizedBox::exact(40.0, 90.0, container()).into_widget(),
                    ])
                    .main_axis_size(pebbles_foundation::MainAxisSize::Min),
                )
                .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(300.0, 200.0));
    assert_eq!(probe_size::<RenderIntrinsicHeight>(&ui).height, 90.0, "height = tallest sibling");
}

#[test]
fn limited_box_caps_only_the_unbounded_axis() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                // A Row gives its children unbounded width; the LimitedBox caps it.
                row(vec![limited_box(SizedBox::exact(400.0, 30.0, container()))
                    .max_width(160.0)
                    .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(500.0, 100.0));
    assert_eq!(probe_size::<RenderLimitedBox>(&ui), Size::new(160.0, 30.0));
}

#[test]
fn overflow_box_lets_the_child_exceed_the_box() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                column(vec![SizedBox::exact(
                    100.0,
                    100.0,
                    overflow_box(SizedBox::exact(200.0, 60.0, container()))
                        .alignment(Alignment::CENTER),
                )
                .into_widget()])
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(200.0, 200.0));
    assert_eq!(probe_size::<RenderOverflowBox>(&ui), Size::new(100.0, 100.0));
    // A 200×60 child centered in 100×100 overhangs 50px on each side.
    let t = transform_of::<RenderOverflowBox>(&ui, Size::new(100.0, 100.0))
        .expect("centered overflow offsets");
    let p = t * Point::new(0.0, 0.0);
    assert!((p.x + 50.0).abs() < 1e-6 && (p.y - 20.0).abs() < 1e-6, "overflow offset: {p:?}");
}

#[test]
fn sweep_gradient_paints_without_panic() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(
        View::new(
            palette::WHITE,
            component(|| {
                SizedBox::exact(
                    120.0,
                    120.0,
                    container().decoration(
                        BoxDecoration::new()
                            .gradient(Gradient::sweep([
                                palette::red::S400, palette::amber::S400, palette::blue::S400,
                                palette::red::S400,
                            ]))
                            .radius(BorderRadius::all(16.0)),
                    ),
                )
            }),
        )
        .into_widget(),
    );
    ui.layout(&mut env, Size::new(200.0, 200.0));
    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene); // encodes the sweep brush — must not panic
}
