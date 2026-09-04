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

/// A serif-heading, roomier variant with a WARM syntax-highlight palette — shows
/// that code coloring is fully themeable, not fixed.
fn serif_style() -> MarkdownStyle {
    MarkdownStyle {
        heading_family: Some("Lora".to_string()),
        heading_scale: [2.1, 1.7, 1.4, 1.2, 1.05, 0.95],
        block_gap: 14.0,
        syntax: SyntaxColors {
            keyword: palette::rose::S500,
            string: palette::amber::S600,
            comment: palette::stone::S400,
            number: palette::orange::S600,
            ident: palette::teal::S600,
            ..SyntaxColors::from_theme()
        },
        ..MarkdownStyle::from_theme()
    }
}

/// A dense variant for sidebars/tooltips, with a COOL syntax palette.
fn compact_style() -> MarkdownStyle {
    MarkdownStyle {
        body_size: 12.5,
        block_gap: 6.0,
        syntax: SyntaxColors {
            keyword: palette::indigo::S500,
            string: palette::emerald::S500,
            comment: palette::slate::S400,
            number: palette::cyan::S600,
            ident: palette::blue::S600,
            ..SyntaxColors::from_theme()
        },
        ..MarkdownStyle::from_theme()
    }
}

pub fn markdown_screen() -> Element {
    // Dev: GALLERY_MD_FILE=<path> loads that file as the initial source (perf test);
    // GALLERY_MD_HUGE=1 loads the generated stress document instead.
    let initial = if std::env::var("GALLERY_MD_HUGE").is_ok_and(|v| v == "1" || v == "true") {
        huge_document()
    } else {
        std::env::var("GALLERY_MD_FILE")
            .ok()
            .and_then(|f| std::fs::read_to_string(f).ok())
            .unwrap_or_else(|| DEMO.to_string())
    };
    let source = create_signal(initial);
    // Two clean single-pane modes: View = read-only formatted (Read); Edit = the
    // source editor (Edit). Read here so the toggle highlights the active mode and
    // the whole screen re-renders when it flips.
    let mode = create_signal(MarkdownMode::Edit);
    let is_view = mode.get() == MarkdownMode::Read;
    let style_idx = create_signal(0usize);

    // ----- the vault: a file explorer feeding the editor (open real .md files)
    let tree = create_signal(FileTree::new());
    let explorer = file_explorer(tree);
    // Dev/burn-in hook: GALLERY_MD_VAULT=<dir> opens that folder at mount —
    // real files without the OS dialog (the input storm suppresses dialogs).
    if let Ok(dir) = std::env::var("GALLERY_MD_VAULT")
        && !dir.is_empty()
    {
        explorer.open_folder(std::path::PathBuf::from(dir));
    }
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
        .description("An Obsidian-style Markdown reader + editor (feature `markdown`, GFM via pulldown-cmark): headings, emphasis, strikethrough, inline + fenced code (JetBrains Mono), clickable links, nested quotes and lists, TASK LISTS with live checkboxes that rewrite the bound source, tables, rules, and images (via `image-view`). Two single-pane modes toggled by a segmented control — View (read-only formatted) and Edit (source editor) — driven by a mode signal YOU own; the widget ships no chrome. Fully themable through MarkdownStyle (defaults follow the app theme, light and dark).")
        .body(children![
            doc("The workbench — a vault of real .md files")
                .description("Obsidian-style: the file explorer on the left is the stock widget over a REAL folder — Open folder, click any .md and it loads into the editor (create_effect on explorer.selection() + path_of + fs::read_to_string, all app code); Save writes the buffer back to disk. The View/Edit toggle and the theme select drive the same signals as before.")
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
                            gap_w(6.0),
                            button("Huge demo").variant(ButtonVariant::Outline).size(ButtonSize::Sm).on_pressed(move || {
                                source.set(huge_document());
                                open_path.set(None);
                                toast("loaded the generated stress document").show();
                            }),
                            gap_w(12.0),
                            // A clean 2-mode segmented toggle, Obsidian-style: the
                            // active mode is filled (Primary), the other outlined.
                            button_group(vec![
                                button("View")
                                    .variant(if is_view { ButtonVariant::Primary } else { ButtonVariant::Outline })
                                    .size(ButtonSize::Sm)
                                    .on_pressed(move || mode.set(MarkdownMode::Read)),
                                button("Edit")
                                    .variant(if is_view { ButtonVariant::Outline } else { ButtonVariant::Primary })
                                    .size(ButtonSize::Sm)
                                    .on_pressed(move || mode.set(MarkdownMode::Edit)),
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
                                // Huge sources read through the VIRTUALIZED reader
                                // (bounded box; only line-of-sight blocks build).
                                // Normal sizes keep the editor workbench.
                                if is_view && source.peek().len() > 200_000 {
                                    let mut md = markdown("").bind(source).virtualized().on_link(
                                        |url| {
                                            toast(format!("link: {url}")).show();
                                        },
                                    );
                                    md = match style_idx.get() {
                                        1 => md.style(serif_style()),
                                        2 => md.style(compact_style()),
                                        _ => md,
                                    };
                                    Container::new().height(560.0).child(md).into_widget()
                                } else {
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
                                }
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

// ---------------------------------------------------------------------------
// The stress fixture — a deterministic worst-case GFM document (~1.5 MB).
// Loaded by the "Huge demo" button or GALLERY_MD_HUGE=1. No randomness, no I/O:
// the same document every run, so perf numbers are comparable across sessions.
// ---------------------------------------------------------------------------

/// Generate the huge deterministic GFM stress document: thousands of styled
/// paragraphs, 100+ fenced code blocks (one ~1000 lines), tables, nested
/// quotes/lists, task lists, and two pathological cases — a single ~100k-char
/// paragraph and a single ~20k-char code line.
pub fn huge_document() -> String {
    const WORDS: [&str; 16] = [
        "viewport", "render", "signal", "widget", "layout", "scene", "glyph", "arena",
        "frame", "paint", "scroll", "anchor", "extent", "cache", "measure", "pebbles",
    ];
    let mut s = String::with_capacity(1_600_000);
    s.push_str("# Stress document\n\nGenerated fixture: deterministic worst-case GFM.\n\n");
    for p in 0..4000usize {
        // Section headings + rules to exercise block variety.
        if p % 100 == 0 {
            s.push_str(&format!("\n## Section {}\n\n", p / 100));
        }
        if p % 200 == 199 {
            s.push_str("\n---\n\n");
        }
        // The flowing paragraph: ~40 words with periodic inline styles.
        for w in 0..40usize {
            let word = WORDS[(p + w) % WORDS.len()];
            match (p + w) % 23 {
                0 => s.push_str(&format!("**{word}** ")),
                7 => s.push_str(&format!("*{word}* ")),
                11 => s.push_str(&format!("`{word}` ")),
                17 => s.push_str(&format!("[{word}](https://example.com/{word}) ")),
                19 => s.push_str(&format!("~~{word}~~ ")),
                _ => {
                    s.push_str(word);
                    s.push(' ');
                }
            }
        }
        s.push_str("\n\n");
        // Fenced code every 40th block (100 total, 30 lines each).
        if p % 40 == 0 {
            s.push_str("```rust\n");
            for l in 0..30usize {
                s.push_str(&format!(
                    "fn item_{p}_{l}(x: u64) -> u64 {{ x * {l} + {p} }} // {}\n",
                    WORDS[l % WORDS.len()]
                ));
            }
            s.push_str("```\n\n");
        }
        // A table every 80th block.
        if p % 80 == 0 {
            s.push_str("| col a | col b | col c | col d |\n|---|---|---|---|\n");
            for r in 0..6usize {
                s.push_str(&format!("| a{p}r{r} | `b{r}` | **c{r}** | d{r} |\n"));
            }
            s.push('\n');
        }
        // Nested quote + list every 60th, task list every 50th.
        if p % 60 == 0 {
            s.push_str("> quoted **block** with nesting\n> > deeper quote\n\n");
            s.push_str(&format!("1. ordered {p}\n2. next\n   - nested child\n   - `code` child\n\n"));
        }
        if p % 50 == 0 {
            s.push_str(&format!("- [ ] open task {p}\n- [x] done task {p}\n\n"));
        }
    }
    // Pathological case 1: one enormous single paragraph (~100k chars, no breaks).
    s.push_str("\n## Pathological paragraph\n\n");
    for i in 0..12_500usize {
        s.push_str(WORDS[i % WORDS.len()]);
        s.push(' ');
    }
    s.push_str("\n\n");
    // Pathological case 2: one enormous single code line (~20k chars).
    s.push_str("## Pathological code line\n\n```\n");
    for i in 0..2_500usize {
        s.push_str(&format!("x{i};"));
    }
    s.push_str("\n```\n");
    s
}
