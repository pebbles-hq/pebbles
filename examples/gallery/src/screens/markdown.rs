use pebbles::prelude::*;

use crate::ui::{doc, screen};

const DEMO: &str = r#"# Pebbles Markdown

A GFM document rendered **live** — edit the source on the left and watch it
update. *Italic*, **bold**, ***both***, ~~strikethrough~~ and `inline code`
all flow inside wrapped paragraphs, and [links are clickable](https://github.com/pebbles-hq/pebbles).

## Task list -- click the checkboxes

- [x] Parse GFM (tables, tasks, strikethrough)
- [ ] Toggle me — the SOURCE rewrites, Obsidian-style
- [ ] Ship an IDE on Pebbles

## Lists

1. Ordered
2. Nested below
   - unordered child
   - another, with `code`

> Block quotes carry whole blocks —
> including **formatting** and nested content.

## Code

```rust
fn main() {
    println!("JetBrains Mono, bundled");
}
```

## Table

| Feature | Status |
| ------- | ------ |
| Reader  | shipped |
| Editor  | shipped |
| Themes  | `MarkdownStyle` |

---

That rule above is a `---`. Images work too (with the `image-view` feature).
"#;

/// A serif-heading, roomier variant — themes are plain data.
fn serif_style() -> MarkdownStyle {
    MarkdownStyle {
        heading_family: Some("Lora".to_string()),
        heading_scale: [2.1, 1.7, 1.4, 1.2, 1.05, 0.95],
        block_gap: 14.0,
        ..MarkdownStyle::from_theme()
    }
}

/// A dense variant for sidebars/tooltips.
fn compact_style() -> MarkdownStyle {
    MarkdownStyle { body_size: 12.5, block_gap: 6.0, ..MarkdownStyle::from_theme() }
}

pub fn markdown_screen() -> Element {
    let source = create_signal(DEMO.to_string());
    let mode = create_signal(MarkdownMode::Split);
    let style_idx = create_signal(0usize);

    // ----- the vault: a file explorer feeding the editor (open real .md files)
    let tree = create_signal(FileTree::new());
    let explorer = file_explorer(tree);
    let open_path = create_signal(Option::<std::path::PathBuf>::None);
    let loaded_id = create_signal(Option::<u64>::None);
    // Selecting a .md file in the explorer loads it into the editor.
    create_effect(move || {
        let sel = explorer.selection().get();
        let Some(&id) = sel.last() else { return };
        if loaded_id.peek() == Some(id) {
            return;
        }
        let Some(path) = explorer.path_of(id) else { return };
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            return;
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => {
                source.set(text);
                open_path.set(Some(path));
                loaded_id.set(Some(id));
            }
            Err(e) => {
                toast(format!("Could not read: {e}")).variant(ToastVariant::Destructive).show();
            }
        }
    });
    let file_label = open_path
        .get()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "demo buffer (open a folder, click a .md)".into());

    screen("Markdown")
        .description("An Obsidian-style Markdown reader + editor (feature `markdown`, GFM via pulldown-cmark): headings, emphasis, strikethrough, inline + fenced code (JetBrains Mono), clickable links, nested quotes and lists, TASK LISTS with live checkboxes that rewrite the bound source, tables, rules, and images (via `image-view`). Three modes — Edit, Split (live preview), Read — driven by a mode signal YOU own; the widget ships no chrome. Fully themable through MarkdownStyle (defaults follow the app theme, light and dark).")
        .body(children![
            doc("The workbench — a vault of real .md files")
                .description("Obsidian-style: the file explorer on the left is the stock widget over a REAL folder — Open folder, click any .md and it loads into the editor (create_effect on explorer.selection() + path_of + fs::read_to_string, all app code); Save writes the buffer back to disk. Edit/Split/Read and the theme select drive the same signals as before.")
                .body(
                    column(children![
                        row(children![
                            button("Open folder").variant(ButtonVariant::Primary).size(ButtonSize::Sm).on_pressed({
                                move || {
                                    pick_folder(move |path| {
                                        if let Some(p) = path {
                                            explorer.open_folder(p);
                                        }
                                    });
                                }
                            }),
                            gap_w(6.0),
                            button("Save").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(move || {
                                match open_path.peek() {
                                    Some(p) => match std::fs::write(&p, source.peek()) {
                                        Ok(()) => {
                                            toast(format!("saved {}", p.display())).show();
                                        }
                                        Err(e) => {
                                            toast(format!("save failed: {e}")).variant(ToastVariant::Destructive).show();
                                        }
                                    },
                                    None => {
                                        toast("no file open — the demo buffer is in-memory").show();
                                    }
                                }
                            }),
                            gap_w(12.0),
                            button_group(vec![
                                button("Edit").variant(ButtonVariant::Outline).size(ButtonSize::Sm)
                                    .on_pressed(move || mode.set(MarkdownMode::Edit)),
                                button("Split").variant(ButtonVariant::Outline).size(ButtonSize::Sm)
                                    .on_pressed(move || mode.set(MarkdownMode::Split)),
                                button("Read").variant(ButtonVariant::Outline).size(ButtonSize::Sm)
                                    .on_pressed(move || mode.set(MarkdownMode::Read)),
                            ]),
                            gap_w(12.0),
                            select(["Theme: Default", "Theme: Serif headings", "Theme: Compact"])
                                .width(220.0)
                                .value(0)
                                .on_changed(move |i, _| style_idx.set(i)),
                        ])
                        .main_axis_size(MainAxisSize::Min),
                        gap_h(6.0),
                        muted(format!("file: {file_label}")),
                        gap_h(10.0),
                        row(children![
                            // The vault pane: the stock explorer, filter bound on top.
                            Container::new()
                                .width(250.0)
                                .decoration(
                                    BoxDecoration::new()
                                        .border(Border::new(theme().colors.border, 1.0))
                                        .radius(BorderRadius::all(theme().radius)),
                                )
                                .padding(EdgeInsets::all(4.0))
                                .child(
                                    column(children![
                                        text_field()
                                            .placeholder("Filter…")
                                            .leading(lucide::SEARCH)
                                            .bind(explorer.filter()),
                                        gap_h(4.0),
                                        Container::new().height(430.0).child(explorer.tree()),
                                    ])
                                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                                    .main_axis_size(MainAxisSize::Min),
                                ),
                            gap_w(12.0),
                            Expanded::new({
                                let mut ed = markdown_editor(source)
                                    .mode_signal(mode)
                                    .lines(22)
                                    .on_link(|url| {
                                        toast(format!("link: {url}")).show();
                                    });
                                ed = match style_idx.get() {
                                    1 => ed.style(serif_style()),
                                    2 => ed.style(compact_style()),
                                    _ => ed,
                                };
                                ed.into_widget()
                            }),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                ),
            doc("Reader — drop it anywhere")
                .description("markdown(text) renders a fixed string (docs panels, changelogs, chat messages, tooltips). This one uses the Compact theme.")
                .body(
                    Container::new()
                        .decoration(
                            BoxDecoration::new()
                                .border(Border::new(theme().colors.border, 1.0))
                                .radius(BorderRadius::all(theme().radius)),
                        )
                        .padding(EdgeInsets::all(12.0))
                        .child(
                            markdown("### Release notes\n\n- **New:** `markdown` widget — reader *and* editor\n- **Fixed:** selection highlight\n- See the [docs](https://github.com/pebbles-hq/pebbles) for more\n\n> Compact style: smaller body, tighter gaps.")
                                .style(compact_style())
                                .on_link(|url| { toast(format!("link: {url}")).show(); }),
                        ),
                ),
        ])
}
