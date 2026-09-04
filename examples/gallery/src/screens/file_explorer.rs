use pebbles::prelude::*;

use crate::ui::{doc, screen};

/// The in-memory demo project the explorer starts on — including one per-node
/// override (`TODO.md` carries its own star icon + color, which always wins
/// over the installed icon theme).
fn demo_tree() -> FileTree {
    let mut t = FileTree::new();
    let src = t.insert(None, FsKind::Folder, "src");
    t.insert(Some(src), FsKind::File, "main.rs");
    t.insert(Some(src), FsKind::File, "lib.rs");
    let ui = t.insert(Some(src), FsKind::Folder, "ui");
    t.insert(Some(ui), FsKind::File, "button.rs");
    t.insert(Some(ui), FsKind::File, "menu.rs");
    let assets = t.insert(None, FsKind::Folder, "assets");
    t.insert(Some(assets), FsKind::File, "logo.png");
    t.insert(Some(assets), FsKind::File, "banner.jpg");
    let docs = t.insert(None, FsKind::Folder, "docs");
    t.insert(Some(docs), FsKind::File, "README.md");
    t.insert(Some(docs), FsKind::File, "guide.md");
    t.insert(None, FsKind::File, "Cargo.toml");
    t.insert(None, FsKind::File, ".gitignore");
    // Per-node override: this file styles ITSELF, whatever theme is installed.
    t.insert_node(None, FsNode::file("TODO.md").icon(lucide::STAR).color(palette::amber::S500));
    t
}

// ---------------------------------------------------------------------------
// Icon themes — what an IDE's icon theming would plug into `set_icon_theme`.
// Return None per node to keep the default folder/file look.
// ---------------------------------------------------------------------------

/// "Code": glyphs by file type + special folders (the VSCode-style theme).
fn code_theme(n: &FsNode, _open: bool) -> Option<(IconData, Option<Color>)> {
    if n.kind == FsKind::Folder {
        return match n.name.as_str() {
            "src" | "ui" => Some((lucide::FOLDER_COG, None)),
            "docs" => Some((lucide::BOOK_OPEN, None)),
            _ => None,
        };
    }
    match n.name.rsplit('.').next() {
        Some("rs") => Some((lucide::FILE_CODE, Some(palette::orange::S400))),
        Some("md") => Some((lucide::FILE_TEXT, Some(palette::sky::S400))),
        Some("png") | Some("jpg") => Some((lucide::FILE_IMAGE, Some(palette::violet::S400))),
        Some("toml") | Some("gitignore") => Some((lucide::COG, None)),
        _ => None,
    }
}

/// "Colorful": the default glyphs, tinted per top-level kind/name.
fn colorful_theme(n: &FsNode, open: bool) -> Option<(IconData, Option<Color>)> {
    if n.kind == FsKind::Folder {
        let d = if open { lucide::FOLDER_OPEN } else { lucide::FOLDER };
        let color = match n.name.as_str() {
            "src" => palette::sky::S500,
            "assets" => palette::emerald::S500,
            "docs" => palette::amber::S500,
            _ => palette::violet::S500,
        };
        return Some((d, Some(color)));
    }
    Some((IconKind::File.data(), Some(palette::teal::S500)))
}

/// "Minimal": quiet dots — glyphs get out of the way entirely.
fn minimal_theme(n: &FsNode, _open: bool) -> Option<(IconData, Option<Color>)> {
    let _ = n;
    Some((lucide::DOT, None))
}

pub fn file_explorer_screen() -> Element {
    let tree = create_signal(demo_tree());
    let explorer = file_explorer(tree);

    screen("File Explorer")
        .description("A standalone, VSCode-grade file explorer: the tree is the widget, the chrome around it is YOURS (no built-in open-folder UI or filter box — wire your own inputs to the control surface). It runs over REAL directories (open_folder(..): lazy loading, on-disk create/rename/delete/move/copy) or fully in-memory. Right-click menus are built in and independent of the global-menu switch. Keyboard: ↑/↓ walk (Shift extends), Mod+↑/↓ move the FOCUS ring without selecting + Mod+Space toggles it in (one-by-one multi-select), →/← expand/collapse, Home/End jump, F2 renames (prefilled, stem selected), Delete, Mod+A selects all (even from idle), Mod+C/X/V copy/cut/paste (cut rows dim; Escape cancels), Escape clears. Rows show the full state set: hover, selected (accent), focus ring, cut (dimmed), drop target.")
        .body(children![
            doc("The explorer — bring your own chrome")
                .description("Everything here is app code driving the control surface: the filter is a plain text_field bound to explorer.filter() (the widget ships no filter box), the buttons call the action closures, Reveal expands + selects a node programmatically (reveal + select_only — the 'Reveal in Explorer' hook). The demo starts on an in-memory project; Open folder switches to the real disk.")
                .body(
                    column(children![
                        text_field()
                            .placeholder("Filter files… (bound to explorer.filter())")
                            .leading(lucide::SEARCH)
                            .bind(explorer.filter())
                            .width(320.0),
                        gap_h(8.0),
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
                            button("Reset demo").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed({
                                let tree = tree;
                                move || {
                                    explorer.detach_folder();
                                    explorer.selection().set(Vec::new());
                                    explorer.renaming().set(None);
                                    tree.set(demo_tree());
                                }
                            }),
                            button("New file").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.new_file()),
                            button("New folder").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.new_folder()),
                            button("Rename").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.rename_selected()),
                            button("Delete").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(explorer.delete_selected()),
                            button("Collapse all").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).on_pressed(explorer.collapse_all()),
                            button("Expand all").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).on_pressed(move || explorer.expand_all()),
                            button("Reveal TODO.md").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).on_pressed({
                                let tree = tree;
                                move || {
                                    fn find(nodes: &[FsNode], name: &str) -> Option<u64> {
                                        nodes.iter().find_map(|n| {
                                            (n.name == name).then_some(n.id).or_else(|| find(&n.children, name))
                                        })
                                    }
                                    if let Some(id) = find(&tree.peek().root, "TODO.md") {
                                        explorer.reveal(id);
                                        explorer.select_only(id);
                                    }
                                }
                            }),
                        ])
                        .spacing(6.0),
                        gap_h(10.0),
                        container()
                            .decoration(
                                BoxDecoration::new()
                                    .border(Border::new(theme().colors.border, 1.0))
                                    .radius(BorderRadius::all(theme().radius)),
                            )
                            .padding(EdgeInsets::all(4.0))
                            .height(280.0)
                            .child(explorer.tree()),
                        gap_h(10.0),
                        muted(format!(
                            "folder: {} · selected: {} · {}",
                            explorer
                                .fs_root()
                                .get()
                                .map(|p| p.display().to_string())
                                .unwrap_or_else(|| "none (in-memory demo)".into()),
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
            doc("Icon themes — the IDE theming hook")
                .description("set_icon_theme(fn) maps every node to any of the ~1800 bundled lucide glyphs + a color — exactly the surface an IDE's icon theming needs; return None per node to keep the default look, and per-node FsNode::icon/color overrides (the starred TODO.md) always win. Switching re-renders the tree live.")
                .body(
                    column(children![
                        select(["Default", "Code (by file type)", "Colorful folders", "Minimal dots"])
                            .width(260.0)
                            .value(0)
                            .leading(lucide::PALETTE)
                            .on_changed(move |i, _| match i {
                                1 => explorer.set_icon_theme(code_theme),
                                2 => explorer.set_icon_theme(colorful_theme),
                                3 => explorer.set_icon_theme(minimal_theme),
                                _ => explorer.clear_icon_theme(),
                            }),
                        gap_h(8.0),
                        muted("Default = open/closed folder + file glyphs. The resolver receives (&FsNode, expanded) so open folders can carry their own glyph too."),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
