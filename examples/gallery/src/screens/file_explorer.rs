use pebbles::prelude::*;

use crate::ui::{doc, screen};

pub fn file_explorer_screen() -> Element {
    let tree = create_signal(FileTree::new());
    let explorer = file_explorer(tree);

    screen("File Explorer")
        .description("A VSCode-style file explorer over REAL directories: open a folder (or set one programmatically with open_folder(..)) and it reads the disk — expand loads lazily, and create/rename/delete/move happen on the real filesystem. Without a folder it is an in-memory model you can drive yourself. Right-click menus are built in (independent of the global-menu switch). Keyboard, while a row is selected: ↑/↓ walk (Shift extends), → expands / steps in, ← collapses / jumps to the parent, F2 renames, Delete deletes, Mod+A selects all, Escape clears. Every node takes its own icon/color (FsNode::icon/color or FileTree::node_mut).")
        .body(children![
            doc("Real folders")
                .description("Starts empty. Open a folder — its real contents appear (folders load lazily on expand); the mutations hit the actual filesystem. The same explorer works in-memory when no folder is set.")
                .body(
                    column(children![
                        wrap(children![
                            button("Open folder").variant(ButtonVariant::Primary).size(ButtonSize::Sm).on_pressed({
                                let explorer = explorer;
                                move || {
                                    pick_folder(move |path| {
                                        if let Some(p) = path {
                                            explorer.open_folder(p);
                                        }
                                    });
                                }
                            }),
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
                            .height(260.0)
                            .child(explorer.tree()),
                        gap_h(10.0),
                        muted(format!(
                            "folder: {} · selected: {} · {}",
                            explorer
                                .fs_root()
                                .get()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "none (in-memory)".into()),
                            explorer.selection().get().len(),
                            explorer
                                .last_error()
                                .get()
                                .map(|e| format!("error: {e}"))
                                .unwrap_or_else(|| "no errors".into()),
                        )),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
