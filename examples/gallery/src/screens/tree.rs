use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn trees() -> Element {
    let open = create_signal([true, false, true]);
    let sel = create_signal(1usize);
    let picked = create_signal(String::from("Cargo.toml"));

    screen("Tree")
        .description("A file-explorer tree — tree_view(tree_node(..)): nested nodes with icons, expand/collapse chevrons and a controlled selection.")
        .body(children![
            doc("File explorer")
                .description("Deep nesting with mixed folders, files and a controlled expanded state; clicking a row selects it.")
                .body(
                    card().child(
                        tree_view(vec![
                            tree_node("src")
                                .expanded(open.get()[0])
                                .on_toggle(move || open.update(|o| o[0] = !o[0]))
                                .children(vec![
                                    tree_node("main.rs")
                                        .icon(IconKind::Dot)
                                        .selected(sel.get() == 0)
                                        .on_select(move || sel.set(0)),
                                    tree_node("components")
                                        .expanded(open.get()[1])
                                        .on_toggle(move || open.update(|o| o[1] = !o[1]))
                                        .children(vec![
                                            tree_node("button.rs").icon(IconKind::Dot).selected(sel.get() == 2).on_select(move || sel.set(2)),
                                            tree_node("tabs.rs").icon(IconKind::Dot).selected(sel.get() == 3).on_select(move || sel.set(3)),
                                            tree_node("input")
                                                .expanded(open.get()[2])
                                                .on_toggle(move || open.update(|o| o[2] = !o[2]))
                                                .children(vec![
                                                    tree_node("text_field.rs").icon(IconKind::Dot).selected(sel.get() == 4).on_select(move || sel.set(4)),
                                                    tree_node("select.rs").icon(IconKind::Dot).selected(sel.get() == 5).on_select(move || sel.set(5)),
                                                ]),
                                        ]),
                                ]),
                            tree_node("Cargo.toml").icon(IconKind::Dot).selected(sel.get() == 1).on_select(move || sel.set(1)),
                        ]),
                    )
                    .padding(EdgeInsets::all(4.0)),
                ),
            doc("Controlled selection")
                .description("The tree only reports — the app owns the selection and can reflect it anywhere.")
                .body(
                    column(children![
                        card().child(
                            tree_view(vec![
                                tree_node("app").icon(IconKind::Dot).selected(sel.get() == 6).on_select(move || { sel.set(6); picked.set("app".into()); }),
                                tree_node("lib").icon(IconKind::Dot).selected(sel.get() == 7).on_select(move || { sel.set(7); picked.set("lib".into()); }),
                                tree_node("README.md").icon(IconKind::Dot).selected(sel.get() == 8).on_select(move || { sel.set(8); picked.set("README.md".into()); }),
                            ]),
                        )
                        .padding(EdgeInsets::all(4.0)),
                        gap_h(10.0),
                        muted(format!("selected: {}", picked.get())),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
