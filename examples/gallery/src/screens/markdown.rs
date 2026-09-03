use pebbles::prelude::*;

use crate::ui::{doc, screen};

const DEMO: &str = r#"# Pebbles Markdown

A GFM document rendered **live** — edit the source on the left and watch it
update. *Italic*, **bold**, ***both***, ~~strikethrough~~ and `inline code`
all flow inside wrapped paragraphs, and [links are clickable](https://github.com/pebbles-hq/pebbles).

## Task list — click the checkboxes

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

    screen("Markdown")
        .description("An Obsidian-style Markdown reader + editor (feature `markdown`, GFM via pulldown-cmark): headings, emphasis, strikethrough, inline + fenced code (JetBrains Mono), clickable links, nested quotes and lists, TASK LISTS with live checkboxes that rewrite the bound source, tables, rules, and images (via `image-view`). Three modes — Edit, Split (live preview), Read — driven by a mode signal YOU own; the widget ships no chrome. Fully themable through MarkdownStyle (defaults follow the app theme, light and dark).")
        .body(children![
            doc("Editor — Edit / Split / Read")
                .description("markdown_editor(source).mode_signal(mode): the switcher below is app code writing a plain signal. The source is a Signal<String> — every keystroke re-renders the preview, and clicking a task checkbox rewrites the source text itself (watch the left pane).")
                .body(
                    column(children![
                        row(children![
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
                        gap_h(10.0),
                        {
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
                        },
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
