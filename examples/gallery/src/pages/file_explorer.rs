use pebbles::prelude::*;

use crate::ui::{doc, screen};

/// The in-memory demo project the explorer starts on — including one per-node
/// override (`TODO.md` carries its own star icon + color, which wins over the
/// installed icon theme).
fn demo_tree() -> FileTree {
    let mut t = FileTree::new();
    let src = t.insert(None, FsKind::Folder, "src");
    t.insert(Some(src), FsKind::File, "main.rs");
    t.insert(Some(src), FsKind::File, "lib.rs");
    let ui = t.insert(Some(src), FsKind::Folder, "ui");
    t.insert(Some(ui), FsKind::File, "button.rs");
    t.insert(Some(ui), FsKind::File, "menu.rs");
    let tests = t.insert(None, FsKind::Folder, "tests");
    t.insert(Some(tests), FsKind::File, "smoke.rs");
    let assets = t.insert(None, FsKind::Folder, "assets");
    t.insert(Some(assets), FsKind::File, "logo.png");
    t.insert(Some(assets), FsKind::File, "banner.jpg");
    let docs = t.insert(None, FsKind::Folder, "docs");
    t.insert(Some(docs), FsKind::File, "guide.md");
    let deep = t.insert(None, FsKind::Folder, "crates");
    let a = t.insert(Some(deep), FsKind::Folder, "core");
    let b = t.insert(Some(a), FsKind::Folder, "src");
    t.insert(Some(b), FsKind::File, "lib.rs");
    t.insert(None, FsKind::File, "Cargo.toml");
    t.insert(None, FsKind::File, "Cargo.lock");
    t.insert(None, FsKind::File, "README.md");
    t.insert(None, FsKind::File, "LICENSE");
    t.insert(None, FsKind::File, "Dockerfile");
    t.insert(None, FsKind::File, ".gitignore");
    t.insert(None, FsKind::File, "index.d.ts");
    t.insert(None, FsKind::File, "app.test.ts");
    // Per-node override: this file styles ITSELF, whatever theme is installed.
    t.insert_node(None, FsNode::file("TODO.md").icon(tabler::STAR).color(palette::amber::S500));
    t
}

/// A custom icon theme built on top of the built-in one.
fn custom_theme() -> IconTheme {
    IconTheme::default_theme()
        .file_extension("rs", ExplorerIcon::colored(tabler::FLAME, palette::orange::S500))
        .folder_name("src", ExplorerIcon::colored(tabler::FOLDER_STAR, palette::sky::S500), None)
        .folder_name(
            "assets",
            ExplorerIcon::colored(tabler::PHOTO, palette::violet::S500),
            Some(ExplorerIcon::colored(tabler::PHOTO_SEARCH, palette::violet::S500)),
        )
        .file_name("Cargo.toml", ExplorerIcon::widget(|size| {
            // Any widget can be an icon.
            container()
                .width(size)
                .height(size)
                .decoration(BoxDecoration::new().color(palette::orange::S500).radius(BorderRadius::all(3.0)))
                .into_widget()
        }))
}

fn framed(child: impl IntoWidget, height: f64) -> impl IntoWidget {
    container()
        .decoration(
            BoxDecoration::new()
                .border(Border::new(theme().colors.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .padding(EdgeInsets::all(4.0))
        .height(height)
        .child(child)
}

pub fn file_explorer_screen() -> Element {
    let tree = create_signal(demo_tree());
    let explorer = file_explorer(tree);
    explorer.set_icon_theme(IconTheme::default_theme());
    // Decorations: a pretend git status + problems (the app owns the data).
    let dirty = create_signal(false);
    explorer.set_decorations(move |e| match e.name.as_str() {
        "main.rs" => Some(Decoration { git: Some(GitStatus::Modified), dirty: dirty.get(), ..Default::default() }),
        "lib.rs" => Some(Decoration { git: Some(GitStatus::Added), ..Default::default() }),
        "menu.rs" => Some(Decoration { errors: 2, ..Default::default() }),
        "smoke.rs" => Some(Decoration { warnings: 1, ..Default::default() }),
        "Cargo.lock" => Some(Decoration { git: Some(GitStatus::Ignored), faded: true, ..Default::default() }),
        _ => None,
    });
    // An app action plugged into the built-in menu, plus a custom item.
    let opened = create_signal(String::from("—"));
    explorer.on_open(move |e, mode| opened.set(format!("{} ({mode:?})", e.name)));
    explorer.set_menu_handler(menu_ids::OPEN_TO_SIDE, move |t| {
        opened.set(format!("{} (to the side)", t.entries.first().map(|e| e.name.clone()).unwrap_or_default()));
    });
    explorer.insert_menu_item_after(
        menu_ids::OPEN,
        ExplorerMenuItem::new("gallery.run", "Run File", move |t| {
            opened.set(format!("ran {}", t.entries.first().map(|e| e.name.clone()).unwrap_or_default()));
        })
        .visible_when(|t| t.entry().is_some_and(|e| e.name.ends_with(".rs"))),
    );
    let compact = create_signal(false);

    // The read-only variant: browse, select, open — nothing else.
    let ro_tree = create_signal(demo_tree());
    let ro = file_explorer(ro_tree).read_only();
    ro.set_icon_theme(IconTheme::default_theme());

    // The custom-icon variant.
    let ci_tree = create_signal(demo_tree());
    let ci = file_explorer(ci_tree);
    ci.set_icon_theme(custom_theme());

    screen("File Explorer")
        .body(children![
            doc("VS Code-grade explorer — bring your own chrome")
                .body(
                    column(children![
                        text_field()
                            .placeholder("Filter files… (bound to explorer.filter())")
                            .leading(tabler::SEARCH)
                            .bind(explorer.filter())
                            .width(320.0),
                        gap_h(8.0),
                        wrap(children![
                            button("Reset demo").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed({
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
                            button("Undo").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(move || {
                                explorer.undo();
                            }),
                            button("Collapse all").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).on_pressed(explorer.collapse_all()),
                            button("Expand all").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).on_pressed(move || explorer.expand_all()),
                            button("Toggle dirty main.rs").variant(ButtonVariant::Ghost).size(ButtonSize::Sm).on_pressed(move || dirty.set(!dirty.peek())),
                            button(if compact.get() { "Compact folders: on" } else { "Compact folders: off" })
                                .variant(ButtonVariant::Ghost)
                                .size(ButtonSize::Sm)
                                .on_pressed(move || {
                                    compact.set(!compact.peek());
                                    let _ = explorer.compact_folders(compact.peek());
                                }),
                        ])
                        .spacing(6.0),
                        gap_h(10.0),
                        framed(explorer.tree(), 320.0),
                        gap_h(10.0),
                        muted(format!(
                            "opened: {} · selected: {} · {}",
                            opened.get(),
                            explorer.selection().get().len(),
                            explorer.last_error().get().map(|e| format!("error: {e}")).unwrap_or_else(|| "no errors".into()),
                        )),
                        muted("Click previews, double-click/Enter opens, F2 renames, Delete trashes (asks first), Ctrl+Z undoes, Ctrl/Shift-click multi-selects, type letters to jump, drag past 8 px onto a highlighted folder to move (Ctrl-drag copies). Right-click rows or empty space for the VS Code menus."),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Read-only — browse, select and open only")
                .body(
                    column(children![
                        muted("file_explorer(tree).read_only(): no create/rename/delete/move/copy; mutating menu items are greyed and the API refuses them."),
                        gap_h(8.0),
                        framed(ro.tree(), 220.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Icon themes — VS Code precedence, any icon")
                .body(
                    column(children![
                        select(["Built-in theme", "Custom theme (glyphs + a widget icon)", "Resolver: dots for everything", "No theme"])
                            .width(320.0)
                            .value(1)
                            .leading(tabler::PALETTE)
                            .on_changed(move |i, _| {
                                ci.clear_icon_resolver();
                                match i {
                                    0 => ci.set_icon_theme(IconTheme::default_theme()),
                                    1 => ci.set_icon_theme(custom_theme()),
                                    2 => ci.set_icon_resolver(|_, _| Some(ExplorerIcon::glyph(tabler::POINT))),
                                    _ => ci.clear_icon_theme(),
                                }
                            }),
                        gap_h(8.0),
                        muted("Files: exact name > longest compound extension (d.ts, test.ts) > extension > language id > default. Folders: name (open/closed) > root > default. A resolver wins over everything; per-path and per-node overrides win over the theme."),
                        gap_h(8.0),
                        framed(ci.tree(), 260.0),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
        ])
}
