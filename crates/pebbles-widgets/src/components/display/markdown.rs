//! [`markdown`] / [`markdown_editor`] — an Obsidian-style Markdown reader + editor
//! (feature `markdown`).
//!
//! **Reading** ([`markdown`]): full GFM via pulldown-cmark — headings, emphasis /
//! strong / strikethrough, inline code, fenced code blocks (JetBrains Mono),
//! links (clickable — wire [`Markdown::on_link`]), block quotes (nested), ordered +
//! unordered lists (nested), **task lists with live checkboxes** (toggling rewrites
//! the bound source, Obsidian-style), tables, horizontal rules, and images (rendered
//! with the `image-view` feature, alt-text otherwise).
//!
//! **Editing** ([`markdown_editor`]): a `Signal<String>`-bound editor with three
//! modes — [`MarkdownMode::Edit`] (source), [`MarkdownMode::Split`] (source + live
//! preview), [`MarkdownMode::Read`] (rendered only). The mode is a signal you can
//! own ([`MarkdownEditor::mode_signal`]) — build your own mode switcher, the widget
//! ships no chrome (the same philosophy as the file explorer).
//!
//! **Theming**: everything visual lives in [`MarkdownStyle`] — heading scale, body
//! size/color, code block/inline colors + family, quote bar, link color, rules,
//! tables. Defaults derive from the live [`theme()`](crate::theme::theme), so it
//! follows light/dark automatically; pass your own via [`Markdown::style`].

use std::rc::Rc;

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

use pebbles_foundation::{Color, CrossAxisAlignment, EdgeInsets, MainAxisSize};
use pebbles_render::{Border, BorderRadius, BoxDecoration, Cursor, lucide};

use crate::theme::{mix, theme};
use crate::widgets::{Container, Expanded, GestureDetector, Padding, column, gap_w, row, text, wrap};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, children, component_props};

// ---------------------------------------------------------------------------
// Style — the theming surface
// ---------------------------------------------------------------------------

/// Every visual knob of the Markdown renderer. Build one with
/// [`MarkdownStyle::from_theme`] and override fields, or construct it fully.
#[derive(Clone)]
pub struct MarkdownStyle {
    /// Body font size (px); headings scale from it via `heading_scale`.
    pub body_size: f32,
    pub body_color: Color,
    pub heading_color: Color,
    /// Per-level multipliers over `body_size` (H1..H6).
    pub heading_scale: [f32; 6],
    /// Optional font family for headings (`None` = the default family).
    pub heading_family: Option<String>,
    pub code_bg: Color,
    pub code_color: Color,
    /// The monospace family for code ("JetBrains Mono" is bundled).
    pub code_family: String,
    pub quote_bar: Color,
    pub quote_color: Color,
    pub link_color: Color,
    pub rule_color: Color,
    pub table_border: Color,
    /// Vertical gap between blocks (px).
    pub block_gap: f64,
}

impl MarkdownStyle {
    /// Defaults derived from the live theme (follows light/dark).
    pub fn from_theme() -> Self {
        let c = theme().colors;
        MarkdownStyle {
            body_size: 14.5,
            body_color: c.foreground,
            heading_color: c.foreground,
            heading_scale: [1.9, 1.55, 1.3, 1.15, 1.0, 0.9],
            heading_family: None,
            code_bg: mix(c.background, c.foreground, 0.07),
            code_color: c.foreground,
            code_family: "JetBrains Mono".to_string(),
            quote_bar: c.border,
            quote_color: c.muted_foreground,
            link_color: c.primary,
            rule_color: c.border,
            table_border: c.border,
            block_gap: 10.0,
        }
    }
}

// ---------------------------------------------------------------------------
// The parsed document model
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Run {
    text: String,
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}

#[derive(Clone)]
enum Inline {
    Run(Run),
    Image { url: String, alt: String },
}

#[derive(Clone)]
struct ListItem {
    /// `Some(checked)` for GFM task items; the usize is the document-order
    /// task ordinal (the handle [`toggle_task`] flips).
    task: Option<(bool, usize)>,
    blocks: Vec<Block>,
}

#[derive(Clone)]
enum Block {
    Heading(u8, Vec<Inline>),
    Para(Vec<Inline>),
    Code { lang: String, text: String },
    Quote(Vec<Block>),
    List { ordered: bool, start: u64, items: Vec<ListItem> },
    Rule,
    Table { header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
}

/// Parse GFM into the block model (tables + strikethrough + task lists on).
fn parse_blocks(src: &str) -> Vec<Block> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    // Frames of nested block containers (document / quote bodies / list items).
    let mut stack: Vec<Vec<Block>> = vec![Vec::new()];
    // In-flight list frames: (ordered, start, finished items).
    let mut lists: Vec<(bool, u64, Vec<ListItem>)> = Vec::new();
    // The pending task marker for the item currently open.
    let mut item_task: Vec<Option<(bool, usize)>> = Vec::new();
    let mut task_ordinal = 0usize;

    // Inline state.
    let mut inline: Vec<Inline> = Vec::new();
    let (mut bold, mut italic, mut strike) = (0u32, 0u32, 0u32);
    let mut link: Vec<String> = Vec::new();
    let mut heading: Option<u8> = None;
    let mut in_para = false;
    // Code blocks.
    let mut code: Option<(String, String)> = None;
    // Image capture (alt text accumulates between Start/End).
    let mut image: Option<(String, String)> = None;
    // Tables.
    let mut table: Option<(Vec<Vec<Inline>>, Vec<Vec<Vec<Inline>>>)> = None;
    let mut table_row: Vec<Vec<Inline>> = Vec::new();
    let mut in_head = false;

    let push_run = |inline: &mut Vec<Inline>,
                        text: String,
                        bold: u32,
                        italic: u32,
                        strike: u32,
                        code: bool,
                        link: &[String]| {
        if text.is_empty() {
            return;
        }
        inline.push(Inline::Run(Run {
            text,
            bold: bold > 0,
            italic: italic > 0,
            strike: strike > 0,
            code,
            link: link.last().cloned(),
        }));
    };

    for ev in Parser::new_ext(src, opts) {
        match ev {
            Event::Start(tag) => match tag {
                Tag::Paragraph => in_para = true,
                Tag::Heading { level, .. } => {
                    heading = Some(match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    });
                }
                Tag::BlockQuote(_) => stack.push(Vec::new()),
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(l) => l.to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code = Some((lang, String::new()));
                }
                Tag::List(start) => lists.push((start.is_some(), start.unwrap_or(1), Vec::new())),
                Tag::Item => {
                    stack.push(Vec::new());
                    item_task.push(None);
                }
                Tag::Emphasis => italic += 1,
                Tag::Strong => bold += 1,
                Tag::Strikethrough => strike += 1,
                Tag::Link { dest_url, .. } => link.push(dest_url.to_string()),
                Tag::Image { dest_url, .. } => image = Some((dest_url.to_string(), String::new())),
                Tag::Table(_) => table = Some((Vec::new(), Vec::new())),
                Tag::TableHead => in_head = true,
                Tag::TableRow => table_row = Vec::new(),
                Tag::TableCell => inline.clear(),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Paragraph => {
                    in_para = false;
                    let content = std::mem::take(&mut inline);
                    if !content.is_empty()
                        && let Some(top) = stack.last_mut()
                    {
                        top.push(Block::Para(content));
                    }
                }
                TagEnd::Heading(_) => {
                    let lvl = heading.take().unwrap_or(1);
                    let content = std::mem::take(&mut inline);
                    if let Some(top) = stack.last_mut() {
                        top.push(Block::Heading(lvl, content));
                    }
                }
                TagEnd::BlockQuote(_) => {
                    let body = stack.pop().unwrap_or_default();
                    if let Some(top) = stack.last_mut() {
                        top.push(Block::Quote(body));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some((lang, text)) = code.take()
                        && let Some(top) = stack.last_mut()
                    {
                        top.push(Block::Code { lang, text });
                    }
                }
                TagEnd::Item => {
                    let blocks = stack.pop().unwrap_or_default();
                    let task = item_task.pop().flatten();
                    if let Some((_, _, items)) = lists.last_mut() {
                        items.push(ListItem { task, blocks });
                    }
                }
                TagEnd::List(_) => {
                    if let Some((ordered, start, items)) = lists.pop()
                        && let Some(top) = stack.last_mut()
                    {
                        top.push(Block::List { ordered, start, items });
                    }
                }
                TagEnd::Emphasis => italic = italic.saturating_sub(1),
                TagEnd::Strong => bold = bold.saturating_sub(1),
                TagEnd::Strikethrough => strike = strike.saturating_sub(1),
                TagEnd::Link => {
                    link.pop();
                }
                TagEnd::Image => {
                    if let Some((url, alt)) = image.take() {
                        inline.push(Inline::Image { url, alt });
                    }
                }
                TagEnd::TableCell => {
                    table_row.push(std::mem::take(&mut inline));
                }
                TagEnd::TableHead => {
                    in_head = false;
                    if let Some((header, _)) = table.as_mut() {
                        *header = std::mem::take(&mut table_row);
                    }
                }
                TagEnd::TableRow => {
                    if let Some((_, rows)) = table.as_mut() {
                        rows.push(std::mem::take(&mut table_row));
                    }
                }
                TagEnd::Table => {
                    if let Some((header, rows)) = table.take()
                        && let Some(top) = stack.last_mut()
                    {
                        top.push(Block::Table { header, rows });
                    }
                }
                _ => {}
            },
            Event::Text(t) => {
                if let Some((_, buf)) = code.as_mut() {
                    buf.push_str(&t);
                } else if let Some((_, alt)) = image.as_mut() {
                    alt.push_str(&t);
                } else {
                    push_run(&mut inline, t.to_string(), bold, italic, strike, false, &link);
                }
            }
            Event::Code(t) => {
                push_run(&mut inline, t.to_string(), bold, italic, strike, true, &link);
            }
            Event::SoftBreak | Event::HardBreak => {
                push_run(&mut inline, " ".to_string(), bold, italic, strike, false, &link);
            }
            Event::Rule => {
                if let Some(top) = stack.last_mut() {
                    top.push(Block::Rule);
                }
            }
            Event::TaskListMarker(checked) => {
                if let Some(slot) = item_task.last_mut() {
                    *slot = Some((checked, task_ordinal));
                    task_ordinal += 1;
                }
            }
            Event::Html(t) | Event::InlineHtml(t) => {
                push_run(&mut inline, t.to_string(), bold, italic, strike, true, &link);
            }
            _ => {}
        }
        let _ = in_para;
        let _ = in_head;
    }
    stack.pop().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Task toggling in the SOURCE (the Obsidian behavior)
// ---------------------------------------------------------------------------

/// Flip the `ordinal`-th task checkbox (document order) in `source`, returning
/// the rewritten text — `- [ ]` ↔ `- [x]` (also `*`/`+` bullets and `1.`/`1)`
/// ordered tasks). `None` if there is no such task.
pub fn toggle_task(source: &str, ordinal: usize) -> Option<String> {
    let mut seen = 0usize;
    let mut lines: Vec<String> = source.split('\n').map(str::to_string).collect();
    for line in lines.iter_mut() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        // Bullet or ordered-list prefix.
        let after_marker = if let Some(rest) =
            trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("+ "))
        {
            Some((trimmed.len() - rest.len(), rest))
        } else {
            let digits = trimmed.chars().take_while(char::is_ascii_digit).count();
            if digits > 0
                && let Some(sep) = trimmed[digits..].strip_prefix('.').or_else(|| trimmed[digits..].strip_prefix(')'))
                && let Some(rest) = sep.strip_prefix(' ')
            {
                Some((trimmed.len() - rest.len(), rest))
            } else {
                None
            }
        };
        let Some((prefix_len, rest)) = after_marker else { continue };
        let checked = if rest.starts_with("[ ] ") || rest == "[ ]" {
            Some(false)
        } else if rest.starts_with("[x] ") || rest.starts_with("[X] ") || rest == "[x]" || rest == "[X]" {
            Some(true)
        } else {
            None
        };
        let Some(checked) = checked else { continue };
        if seen == ordinal {
            let box_at = indent + prefix_len + 1; // inside the '['
            line.replace_range(box_at..box_at + 1, if checked { " " } else { "x" });
            return Some(lines.join("\n"));
        }
        seen += 1;
    }
    None
}

// ---------------------------------------------------------------------------
// The reader widget
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum Source {
    Fixed(String),
    Bound(Signal<String>),
}

/// A rendered Markdown document. Build with [`markdown`] (fixed text) or
/// [`markdown().bind(sig)`](Markdown::bind) (live source: edits re-render, task
/// checkboxes rewrite the source).
pub struct Markdown {
    source: Source,
    style: Option<MarkdownStyle>,
    on_link: Option<Rc<dyn Fn(&str)>>,
    on_task: Option<Rc<dyn Fn(usize, bool)>>,
}

/// Render `source` as Markdown (GFM: tables, task lists, strikethrough).
pub fn markdown(source: impl Into<String>) -> Markdown {
    Markdown { source: Source::Fixed(source.into()), style: None, on_link: None, on_task: None }
}

impl Markdown {
    /// Render from (and write task toggles back to) a live `Signal<String>`.
    pub fn bind(mut self, source: Signal<String>) -> Self {
        self.source = Source::Bound(source);
        self
    }
    /// Override the visual style (default: [`MarkdownStyle::from_theme`]).
    pub fn style(mut self, s: MarkdownStyle) -> Self {
        self.style = Some(s);
        self
    }
    /// Called with the URL when a link is clicked (open a browser, route
    /// internally, …). Links render inert without it.
    pub fn on_link(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_link = Some(Rc::new(f));
        self
    }
    /// Called after a task checkbox toggles with `(ordinal, now_checked)` —
    /// the bound source (if any) has already been rewritten.
    pub fn on_task(mut self, f: impl Fn(usize, bool) + 'static) -> Self {
        self.on_task = Some(Rc::new(f));
        self
    }
}

struct MdProps {
    source: Source,
    style: Option<MarkdownStyle>,
    on_link: Option<Rc<dyn Fn(&str)>>,
    on_task: Option<Rc<dyn Fn(usize, bool)>>,
}

impl IntoWidget for Markdown {
    fn into_widget(mut self) -> AnyWidget {
        component_props(
            render_markdown,
            MdProps {
                source: self.source.clone(),
                style: self.style.take(),
                on_link: self.on_link.take(),
                on_task: self.on_task.take(),
            },
        )
        .into_widget()
    }
}

/// Render context threaded through the block renderers.
#[derive(Clone)]
struct Cx {
    style: Rc<MarkdownStyle>,
    on_link: Option<Rc<dyn Fn(&str)>>,
    on_task: Option<Rc<dyn Fn(usize, bool)>>,
    bound: Option<Signal<String>>,
    /// Text color override (block quotes mute their body).
    color: Color,
}

fn render_markdown(p: &MdProps) -> AnyWidget {
    let src = match &p.source {
        Source::Fixed(s) => s.clone(),
        Source::Bound(sig) => sig.get(), // subscribe: edits re-render the view
    };
    let style = Rc::new(p.style.clone().unwrap_or_else(MarkdownStyle::from_theme));
    let cx = Cx {
        color: style.body_color,
        style,
        on_link: p.on_link.clone(),
        on_task: p.on_task.clone(),
        bound: match &p.source {
            Source::Bound(sig) => Some(*sig),
            Source::Fixed(_) => None,
        },
    };
    let blocks = parse_blocks(&src);
    render_blocks(&blocks, &cx)
}

fn render_blocks(blocks: &[Block], cx: &Cx) -> AnyWidget {
    let kids: Vec<AnyWidget> = blocks.iter().map(|b| render_block(b, cx)).collect();
    column(kids)
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min)
        .spacing(cx.style.block_gap)
        .into_widget()
}

fn render_block(b: &Block, cx: &Cx) -> AnyWidget {
    let s = &cx.style;
    match b {
        Block::Heading(lvl, inlines) => {
            let size = s.body_size * s.heading_scale[(*lvl as usize - 1).min(5)];
            inline_flow(inlines, cx, size, s.heading_color, true)
        }
        Block::Para(inlines) => inline_flow(inlines, cx, s.body_size, cx.color, false),
        Block::Code { lang, text: code } => {
            let mut kids: Vec<AnyWidget> = Vec::new();
            if !lang.is_empty() {
                kids.push(
                    text(lang.clone())
                        .size(s.body_size * 0.78)
                        .color(mix(s.code_color, s.code_bg, 0.45))
                        .font_family(s.code_family.clone())
                        .into_widget(),
                );
            }
            kids.push(
                text(code.trim_end().to_string())
                    .size(s.body_size * 0.92)
                    .color(s.code_color)
                    .font_family(s.code_family.clone())
                    .into_widget(),
            );
            Container::new()
                .decoration(BoxDecoration::new().color(s.code_bg).radius(BorderRadius::all(6.0)))
                .padding(EdgeInsets::all(10.0))
                .child(
                    column(kids)
                        .cross_axis_alignment(CrossAxisAlignment::Start)
                        .main_axis_size(MainAxisSize::Min)
                        .spacing(4.0),
                )
                .into_widget()
        }
        Block::Quote(body) => {
            let quoted = Cx { color: s.quote_color, ..cx.clone() };
            row(children![
                Container::new().width(3.0).color(s.quote_bar),
                gap_w(10.0),
                Expanded::new(render_blocks(body, &quoted)),
            ])
            .main_axis_size(MainAxisSize::Min)
            .into_widget()
        }
        Block::List { ordered, start, items } => {
            let mut rows: Vec<AnyWidget> = Vec::new();
            for (i, item) in items.iter().enumerate() {
                let marker: AnyWidget = match item.task {
                    Some((checked, ordinal)) => {
                        let (bound, on_task) = (cx.bound, cx.on_task.clone());
                        GestureDetector::new(
                            crate::components::icon(if checked {
                                lucide::SQUARE_CHECK
                            } else {
                                lucide::SQUARE
                            })
                            .size(15.0)
                            .color(if checked { s.link_color } else { cx.color }),
                        )
                        .cursor(Cursor::Pointer)
                        .on_tap(move || {
                            // Rewrite the bound source (Obsidian behavior), then report.
                            if let Some(sig) = bound
                                && let Some(new) = toggle_task(&sig.peek(), ordinal)
                            {
                                sig.set(new);
                            }
                            if let Some(f) = &on_task {
                                f(ordinal, !checked);
                            }
                        })
                        .into_widget()
                    }
                    None if *ordered => text(format!("{}.", start + i as u64))
                        .size(s.body_size)
                        .color(cx.color)
                        .into_widget(),
                    None => text("•").size(s.body_size).color(cx.color).into_widget(),
                };
                rows.push(
                    row(children![
                        Container::new().width(22.0).child(marker),
                        Expanded::new(render_blocks(&item.blocks, cx)),
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min)
                    .into_widget(),
                );
            }
            Padding::new(
                EdgeInsets::only(8.0, 0.0, 0.0, 0.0),
                column(rows)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min)
                    .spacing(4.0),
            )
            .into_widget()
        }
        Block::Rule => Container::new().height(1.0).color(s.rule_color).into_widget(),
        Block::Table { header, rows } => {
            let cell = |inlines: &Vec<Inline>, bold: bool| -> AnyWidget {
                Padding::new(
                    EdgeInsets::symmetric(8.0, 5.0),
                    inline_flow(inlines, cx, cx.style.body_size * 0.95, cx.color, bold),
                )
                .into_widget()
            };
            let mut lines: Vec<AnyWidget> = Vec::new();
            let head_cells: Vec<AnyWidget> =
                header.iter().map(|c| Expanded::new(cell(c, true)).into_widget()).collect();
            lines.push(
                Container::new()
                    .color(mix(theme().colors.background, theme().colors.foreground, 0.04))
                    .child(row(head_cells).main_axis_size(MainAxisSize::Min))
                    .into_widget(),
            );
            for r in rows {
                let cells: Vec<AnyWidget> =
                    r.iter().map(|c| Expanded::new(cell(c, false)).into_widget()).collect();
                lines.push(
                    Container::new()
                        .decoration(BoxDecoration::new().border(Border::new(s.table_border, 0.5)))
                        .child(row(cells).main_axis_size(MainAxisSize::Min))
                        .into_widget(),
                );
            }
            Container::new()
                .decoration(
                    BoxDecoration::new()
                        .border(Border::new(s.table_border, 1.0))
                        .radius(BorderRadius::all(6.0)),
                )
                .child(
                    column(lines)
                        .cross_axis_alignment(CrossAxisAlignment::Stretch)
                        .main_axis_size(MainAxisSize::Min),
                )
                .into_widget()
        }
    }
}

/// Lay inline runs out as a wrapping flow of word chunks (how rich inline text
/// composes without a spans API — code spans and images stay whole).
fn inline_flow(inlines: &[Inline], cx: &Cx, size: f32, color: Color, bold_all: bool) -> AnyWidget {
    let s = &cx.style;
    let mut chunks: Vec<AnyWidget> = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Run(run) => {
                if run.code {
                    chunks.push(
                        Container::new()
                            .decoration(
                                BoxDecoration::new().color(s.code_bg).radius(BorderRadius::all(4.0)),
                            )
                            .padding(EdgeInsets::symmetric(4.0, 1.0))
                            .child(
                                text(run.text.clone())
                                    .size(size * 0.9)
                                    .color(s.code_color)
                                    .font_family(s.code_family.clone()),
                            )
                            .into_widget(),
                    );
                    continue;
                }
                let link_color = run.link.as_ref().map(|_| s.link_color);
                for word in run.text.split_whitespace() {
                    let mut t = text(format!("{word} "))
                        .size(size)
                        .color(link_color.unwrap_or(color));
                    if bold_all || run.bold {
                        t = t.semibold();
                    }
                    if run.italic {
                        t = t.italic();
                    }
                    if run.strike {
                        t = t.strikethrough();
                    }
                    if run.link.is_some() {
                        t = t.underline();
                    }
                    if let Some(fam) = (size != s.body_size)
                        .then(|| s.heading_family.clone())
                        .flatten()
                    {
                        t = t.font_family(fam);
                    }
                    let chunk: AnyWidget = match (&run.link, &cx.on_link) {
                        (Some(url), Some(f)) => {
                            let (url, f) = (url.clone(), f.clone());
                            GestureDetector::new(t)
                                .cursor(Cursor::Pointer)
                                .on_tap(move || f(&url))
                                .into_widget()
                        }
                        _ => t.into_widget(),
                    };
                    chunks.push(chunk);
                }
            }
            Inline::Image { url, alt } => {
                chunks.push(render_image(url, alt, cx));
            }
        }
    }
    wrap(chunks).spacing(0.0).run_spacing(3.0).into_widget()
}

#[cfg(feature = "image-view")]
fn render_image(url: &str, _alt: &str, _cx: &Cx) -> AnyWidget {
    let view = if url.starts_with("http://") || url.starts_with("https://") {
        crate::ImageView::network(url)
    } else {
        crate::ImageView::asset(url)
    };
    view.height(200.0).radius(BorderRadius::all(6.0)).into_widget()
}

#[cfg(not(feature = "image-view"))]
fn render_image(_url: &str, alt: &str, cx: &Cx) -> AnyWidget {
    // Without the image stack, show the alt text as a quiet placeholder.
    text(format!("[{alt}]"))
        .size(cx.style.body_size)
        .color(cx.style.quote_color)
        .italic()
        .into_widget()
}

// ---------------------------------------------------------------------------
// The editor
// ---------------------------------------------------------------------------

/// The editor's display mode.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MarkdownMode {
    /// Source only.
    Edit,
    /// Source + live preview side by side.
    Split,
    /// Rendered only.
    Read,
}

/// A `Signal<String>`-bound Markdown editor with Edit / Split / Read modes.
/// Build with [`markdown_editor`]; own the mode via [`mode_signal`](Self::mode_signal)
/// and build your own switcher — the widget ships no chrome.
pub struct MarkdownEditor {
    source: Signal<String>,
    mode: Option<Signal<MarkdownMode>>,
    style: Option<MarkdownStyle>,
    on_link: Option<Rc<dyn Fn(&str)>>,
    lines: u32,
}

/// A Markdown editor over a live `Signal<String>` (default mode: Split).
pub fn markdown_editor(source: Signal<String>) -> MarkdownEditor {
    MarkdownEditor { source, mode: None, style: None, on_link: None, lines: 16 }
}

impl MarkdownEditor {
    /// Drive the mode from YOUR signal (build any switcher UI around it).
    pub fn mode_signal(mut self, mode: Signal<MarkdownMode>) -> Self {
        self.mode = Some(mode);
        self
    }
    /// Override the preview style.
    pub fn style(mut self, s: MarkdownStyle) -> Self {
        self.style = Some(s);
        self
    }
    /// Link-click handler for the preview.
    pub fn on_link(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_link = Some(Rc::new(f));
        self
    }
    /// The source pane's visible line count (default 16).
    pub fn lines(mut self, n: u32) -> Self {
        self.lines = n;
        self
    }
}

struct EdProps {
    source: Signal<String>,
    mode: Option<Signal<MarkdownMode>>,
    style: Option<MarkdownStyle>,
    on_link: Option<Rc<dyn Fn(&str)>>,
    lines: u32,
}

impl IntoWidget for MarkdownEditor {
    fn into_widget(mut self) -> AnyWidget {
        component_props(
            render_editor,
            EdProps {
                source: self.source,
                mode: self.mode.take(),
                style: self.style.take(),
                on_link: self.on_link.take(),
                lines: self.lines,
            },
        )
        .into_widget()
    }
}

fn render_editor(p: &EdProps) -> AnyWidget {
    use crate::components::text_area;
    let mode = match p.mode {
        Some(sig) => sig.get(),
        None => MarkdownMode::Split,
    };
    let preview = || {
        let mut md = markdown("").bind(p.source);
        if let Some(s) = p.style.clone() {
            md = md.style(s);
        }
        if let Some(f) = p.on_link.clone() {
            let f = f.clone();
            md = md.on_link(move |u| f(u));
        }
        md.into_widget()
    };
    let editor = || {
        text_area(p.lines)
            .bind(p.source)
            .placeholder("Write some Markdown…")
            .into_widget()
    };
    match mode {
        MarkdownMode::Edit => editor(),
        MarkdownMode::Read => preview(),
        MarkdownMode::Split => row(children![
            Expanded::new(editor()),
            gap_w(12.0),
            Expanded::new(preview()),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min)
        .into_widget(),
    }
}
