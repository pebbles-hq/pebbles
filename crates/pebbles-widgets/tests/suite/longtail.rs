//! Long-tail widgets: `Placeholder` (a Canvas-backed dev box) honors its fixed
//! `.size(..)` and paints; `Banner` mounts, lays out and paints its message +
//! actions without panicking.

use pebbles_core::{IntoWidget, Ui, component};
use pebbles_foundation::{Size, palette};
use pebbles_render::{RenderCanvas, TextEnv};
use pebbles_widgets::{ButtonVariant, IconKind, View, banner, button, center, column, placeholder};

fn placeholder_root() -> impl IntoWidget {
    // center() loosens the root View's tight window constraints so the fixed
    // .size(..) holds (a SizedBox under TIGHT constraints is forced to fill).
    center(placeholder().size(140.0, 90.0))
}

#[test]
fn placeholder_honors_its_fixed_size_and_paints() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(placeholder_root)).into_widget());
    ui.layout(&mut env, Size::new(400.0, 400.0));

    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene);

    // Placeholder is a Canvas under the hood; the fixed size flows to the render node.
    let tree = ui.render_tree();
    let id = tree.find::<RenderCanvas>().expect("placeholder paints through a canvas");
    assert_eq!(tree.size_of(id).width, 140.0);
    assert_eq!(tree.size_of(id).height, 90.0);
}

fn banner_root() -> impl IntoWidget {
    column(vec![
        banner("Your trial ends in 3 days.")
            .icon(IconKind::Info)
            .action(button("Dismiss").variant(ButtonVariant::Ghost))
            .action(button("Upgrade"))
            .into_widget(),
    ])
}

#[test]
fn banner_mounts_and_paints() {
    let mut ui = Ui::new();
    let mut env = TextEnv::new();
    ui.mount_root(View::new(palette::WHITE, component(banner_root)).into_widget());
    ui.layout(&mut env, Size::new(500.0, 200.0));

    let mut scene = pebbles_render::Scene::new();
    ui.paint(&mut env, &mut scene); // must not panic

    // The trailing actions are real buttons and reach the semantics tree.
    let tree = ui.render_tree();
    let sem = tree.semantics_tree();
    assert!(
        sem.iter().any(|n| n.props.label == "Upgrade"),
        "the banner's action buttons are present in the semantics tree",
    );
}
