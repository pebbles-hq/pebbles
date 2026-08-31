use pebbles::prelude::*;

use crate::ui::{doc, screen};

fn demo_tree() -> FileTree {
    let mut t = FileTree::new();
    let src = t.insert(None, FsKind::Folder, "src");
    t.insert(Some(src), FsKind::File, "main.rs");
    let comps = t.insert(Some(src), FsKind::Folder, "components");
    t.insert(Some(comps), FsKind::File, "button.rs");
    t.insert(Some(comps), FsKind::File, "tabs.rs");
    let docs = t.insert(None, FsKind::Folder, "docs");
    t.insert(Some(docs), FsKind::File, "guide.md");
    t.insert(None, FsKind::File, "Cargo.toml");
    t.insert(None, FsKind::File, "README.md");
    t
}

pub fn file_explorer_screen() -> Element {
    let tree = create_signal(demo_tree());
    let explorer = file_explorer(tree);

    screen("File Explorer")
        .description("A VSCode-style file explorer built from composable pieces: the FileTree model (app-owned), the file_explorer() controller, and a tree that selects, expands, renames inline, context-menus, and drag-moves. Click to select, double-click to rename, right-click for actions, drag onto a folder to move.")
        .body(children![
            doc("The explorer")
                .description("The default toolbar (New File · New Folder · Collapse All) above the tree — or skip the toolbar entirely.")
                .body(
                    column(children![
                        Container::new()
                            .decoration(
                                BoxDecoration::new()
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius(BorderRadius::all(theme().radius)),
                            )
                            .padding(EdgeInsets::all(4.0))
                            .child(explorer.tree()),
                        gap_h(10.0),
                        muted(format!(
                            "selected: {}",
                            explorer
                                .selection()
                                .get()
                                .and_then(|id| tree.get().node(id).map(|n| n.name.clone()))
                                .unwrap_or_else(|| "—".into())
                        )),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Compose your own actions")
                .description("Every action is a plain closure on the controller — place the buttons wherever your layout wants them (a title bar, a footer, split across panels). The toolbar is optional sugar.")
                .body(
                    column(children![
                        wrap(children![
                            button("New file").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.new_file()),
                            button("New folder").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.new_folder()),
                            button("Rename").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.rename_selected()),
                            button("Delete").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.delete_selected()),
                            button("Collapse all").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).on_pressed(explorer.collapse_all()),
                        ])
                        .spacing(6.0),
                        gap_h(10.0),
                        Container::new()
                            .decoration(
                                BoxDecoration::new()
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius(BorderRadius::all(theme().radius)),
                            )
                            .padding(EdgeInsets::all(4.0))
                            .child(explorer.tree()),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
