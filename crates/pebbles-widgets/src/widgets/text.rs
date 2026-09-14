//! [`Text`] — a leaf widget that shapes and paints a string. Backs
//! [`pebbles_render::RenderParagraph`].

use pebbles_foundation::{Color, TextAlign};
use pebbles_render::{ParagraphStyle, RenderObject, RenderParagraph};

use pebbles_core::IntoWidget;
use pebbles_core::widget::{AnyWidget, RenderWidget};

type LinkCb = std::rc::Rc<dyn Fn(&str)>;
/// A link-hover callback: `Some(url)` while the pointer is over a link span,
/// `None` when it leaves the last link.
type LinkHoverCb = std::rc::Rc<dyn Fn(Option<&str>)>;
type LinkBoxes = std::rc::Rc<std::cell::RefCell<Vec<(pebbles_foundation::Rect, usize)>>>;

/// A document-wide selection: `(anchor_index, anchor_byte, focus_index, focus_byte)`
/// where `index` is a leaf's position in its [`SelectionGroup`].
type CrossSel = (usize, usize, usize, usize);

/// A registered leaf: `(select_id, global rect, text)`.
type LeafInfo = (u64, pebbles_foundation::Rect, String);
/// A group's leaves, keyed by document index.
type GroupLeaves = std::collections::HashMap<usize, LeafInfo>;

thread_local! {
    /// Registered selectable leaves per group: `group_id → (leaf_index → info)` —
    /// refreshed each layout so a drag can map a global point to the leaf under it
    /// and copy across leaves.
    static SEL_GROUPS: std::cell::RefCell<std::collections::HashMap<u64, GroupLeaves>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A shared handle that ties several [`RichText::selectable_in`] leaves into one
/// selection surface, so a drag (and copy) spans across them. Create one per
/// document region and give each selectable leaf a monotonic `index`.
#[derive(Clone, Copy)]
pub struct SelectionGroup {
    id: u64,
    sel: pebbles_core::Signal<Option<CrossSel>>,
}

/// Create a [`SelectionGroup`] with a stable `id` (e.g. the owning component's
/// `owner_id`). Call from a component so its selection signal is stable.
pub fn selection_group(id: u64) -> SelectionGroup {
    SelectionGroup { id, sel: pebbles_core::create_signal(None) }
}

/// A stable per-leaf hit-test id from a group + index.
fn leaf_select_id(group: u64, index: usize) -> u64 {
    group.wrapping_mul(1_000_003).wrapping_add(index as u64 + 1)
}

/// The leaf under a global point within a group: `(index, select_id, local_x, local_y)`.
fn group_leaf_at(group: u64, gx: f64, gy: f64) -> Option<(usize, u64, f64, f64)> {
    SEL_GROUPS.with(|g| {
        let g = g.borrow();
        let leaves = g.get(&group)?;
        leaves.iter().find_map(|(&idx, &(sid, rect, _))| {
            rect.contains(pebbles_foundation::Offset::new(gx, gy).to_point()).then_some((
                idx,
                sid,
                gx - rect.x0,
                gy - rect.y0,
            ))
        })
    })
}

/// Which text properties a [`Text`] set **explicitly** — the ones that win over an
/// inherited [`default_text_style`](crate::default_text_style). Unset properties fall
/// through to the ancestor's style (Flutter's `DefaultTextStyle` inheritance).
#[derive(Clone, Copy, Default)]
pub(crate) struct TextFields(u16);

impl TextFields {
    pub(crate) const FONT_SIZE: u16 = 1 << 0;
    pub(crate) const COLOR: u16 = 1 << 1;
    pub(crate) const LINE_HEIGHT: u16 = 1 << 2;
    pub(crate) const WEIGHT: u16 = 1 << 3;
    pub(crate) const ALIGN: u16 = 1 << 4;
    pub(crate) const LETTER_SPACING: u16 = 1 << 5;
    pub(crate) const ITALIC: u16 = 1 << 6;
    pub(crate) const UNDERLINE: u16 = 1 << 7;
    pub(crate) const STRIKETHROUGH: u16 = 1 << 8;
    pub(crate) const FONT_FAMILY: u16 = 1 << 9;
    pub(crate) const MAX_LINES: u16 = 1 << 10;
    pub(crate) const ELLIPSIS: u16 = 1 << 11;
    pub(crate) const SOFT_WRAP: u16 = 1 << 12;
    /// Every property (used by `.paragraph_style(..)`, which sets them all).
    pub(crate) const ALL: u16 = (1 << 13) - 1;

    pub(crate) fn mark(&mut self, bit: u16) {
        self.0 |= bit;
    }
    pub(crate) fn has(self, bit: u16) -> bool {
        self.0 & bit != 0
    }
}

/// Context value carrying the ambient text style for a subtree, provided by
/// [`default_text_style`](crate::default_text_style) and consumed by every
/// descendant [`Text`] that didn't set a given property.
#[derive(Clone)]
pub(crate) struct InheritedTextStyle(pub ParagraphStyle);

/// Overlay the properties marked in `set` (from `style`) onto `base` — the shared
/// merge used by both `Text` resolution and `default_text_style` nesting.
pub(crate) fn overlay_fields(base: &mut ParagraphStyle, style: &ParagraphStyle, set: TextFields) {
    if set.has(TextFields::FONT_SIZE) {
        base.font_size = style.font_size;
    }
    if set.has(TextFields::COLOR) {
        base.color = style.color;
    }
    if set.has(TextFields::LINE_HEIGHT) {
        base.line_height = style.line_height;
    }
    if set.has(TextFields::WEIGHT) {
        base.weight = style.weight;
    }
    if set.has(TextFields::ALIGN) {
        base.align = style.align;
    }
    if set.has(TextFields::LETTER_SPACING) {
        base.letter_spacing = style.letter_spacing;
    }
    if set.has(TextFields::ITALIC) {
        base.italic = style.italic;
    }
    if set.has(TextFields::UNDERLINE) {
        base.underline = style.underline;
    }
    if set.has(TextFields::STRIKETHROUGH) {
        base.strikethrough = style.strikethrough;
    }
    if set.has(TextFields::FONT_FAMILY) {
        base.font_family = style.font_family.clone();
    }
    if set.has(TextFields::MAX_LINES) {
        base.max_lines = style.max_lines;
    }
    if set.has(TextFields::ELLIPSIS) {
        base.ellipsis = style.ellipsis;
    }
    if set.has(TextFields::SOFT_WRAP) {
        base.soft_wrap = style.soft_wrap;
    }
}

/// A run of styled text.
#[derive(Clone)]
pub struct Text {
    pub data: String,
    pub style: ParagraphStyle,
    /// Which properties were set explicitly (the rest inherit).
    set: TextFields,
}

/// Create a [`Text`] widget. Chain `.size(..)` / `.color(..)` to style it.
pub fn text(data: impl Into<String>) -> Text {
    Text { data: data.into(), style: ParagraphStyle::default(), set: TextFields::default() }
}

/// E5 — a `Text` bound to a `Signal<String>`, isolated in its own leaf component: a
/// write re-renders ONLY this text node, not the owning component. That's the spike's
/// finding — per-component granularity, applied to a leaf, already gives fine-grained
/// text updates, so the heavier render-object-direct-write path stays unbuilt (its win
/// is unproven per the E5 charter). Style it via the closure, e.g.
/// `text_signal(count)` or wrap: `text(sig.get()).size(24.0)` inside `component(..)`.
pub fn text_signal(signal: pebbles_core::Signal<String>) -> impl IntoWidget {
    pebbles_core::component_props(render_text_signal, TextSignalProps { signal })
}

#[derive(Clone)]
struct TextSignalProps {
    signal: pebbles_core::Signal<String>,
}

fn render_text_signal(p: &TextSignalProps) -> Text {
    text(p.signal.get())
}

impl Text {
    /// Set the font size (logical px).
    pub fn size(mut self, size: f32) -> Self {
        self.style.font_size = size;
        self.set.mark(TextFields::FONT_SIZE);
        self
    }

    /// Set the text color.
    pub fn color(mut self, color: Color) -> Self {
        self.style.color = color;
        self.set.mark(TextFields::COLOR);
        self
    }

    /// Set the line height as a multiple of the font size.
    pub fn line_height(mut self, factor: f32) -> Self {
        self.style.line_height = factor;
        self.set.mark(TextFields::LINE_HEIGHT);
        self
    }

    /// Set an explicit font weight (400 normal … 700 bold).
    pub fn weight(mut self, weight: f32) -> Self {
        self.style.weight = weight;
        self.set.mark(TextFields::WEIGHT);
        self
    }

    /// Semibold (600).
    pub fn semibold(mut self) -> Self {
        self.style.weight = 600.0;
        self.set.mark(TextFields::WEIGHT);
        self
    }

    /// Bold (700).
    pub fn bold(mut self) -> Self {
        self.style.weight = 700.0;
        self.set.mark(TextFields::WEIGHT);
        self
    }

    /// Horizontal alignment within the text's width.
    pub fn align(mut self, align: TextAlign) -> Self {
        self.style.align = align;
        self.set.mark(TextFields::ALIGN);
        self
    }
    /// Extra spacing between letters (logical px).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.style.letter_spacing = px;
        self.set.mark(TextFields::LETTER_SPACING);
        self
    }
    /// Render italic.
    pub fn italic(mut self) -> Self {
        self.style.italic = true;
        self.set.mark(TextFields::ITALIC);
        self
    }
    /// Draw an underline.
    pub fn underline(mut self) -> Self {
        self.style.underline = true;
        self.set.mark(TextFields::UNDERLINE);
        self
    }
    /// Draw a strike-through line.
    pub fn strikethrough(mut self) -> Self {
        self.style.strikethrough = true;
        self.set.mark(TextFields::STRIKETHROUGH);
        self
    }
    /// Select a font family by name (system fallback if unavailable).
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.style.font_family = Some(family.into());
        self.set.mark(TextFields::FONT_FAMILY);
        self
    }
    /// Clamp to at most `n` lines (excess dropped).
    pub fn max_lines(mut self, n: u32) -> Self {
        self.style.max_lines = Some(n);
        self.set.mark(TextFields::MAX_LINES);
        self
    }
    /// With `max_lines`, append "…" to the last line when the text overflows.
    pub fn ellipsis(mut self) -> Self {
        self.style.ellipsis = true;
        self.set.mark(TextFields::ELLIPSIS);
        self
    }
    /// Disable line wrapping: the text shapes as a single unbounded line that clips
    /// to its box (combine with [`Self::ellipsis`] for a one-line "…" label).
    pub fn soft_wrap(mut self, wrap: bool) -> Self {
        self.style.soft_wrap = wrap;
        self.set.mark(TextFields::SOFT_WRAP);
        self
    }

    /// Style this text from an explicit [`ParagraphStyle`] — sets every property
    /// (so nothing inherits).
    pub fn paragraph_style(mut self, style: ParagraphStyle) -> Self {
        self.style = style;
        self.set.mark(TextFields::ALL);
        self
    }

    /// Resolve the style against an inherited `default_text_style` (if any): start
    /// from the ancestor's style and overlay the properties this `Text` set.
    fn resolved(&self) -> ParagraphStyle {
        match pebbles_core::consume_context::<InheritedTextStyle>() {
            Some(InheritedTextStyle(mut base)) => {
                overlay_fields(&mut base, &self.style, self.set);
                base
            }
            None => self.style.clone(),
        }
    }

    /// Apply a general [`Style`](crate::Style): its text properties (color, font
    /// size/weight, line height) style the text, and its box properties (padding,
    /// background, …) wrap it.
    pub fn style(mut self, s: crate::style::Style) -> AnyWidget {
        if let Some(c) = s.color {
            self.style.color = c;
            self.set.mark(TextFields::COLOR);
        }
        if let Some(fs) = s.font_size {
            self.style.font_size = fs;
            self.set.mark(TextFields::FONT_SIZE);
        }
        if let Some(w) = s.font_weight {
            self.style.weight = w;
            self.set.mark(TextFields::WEIGHT);
        }
        if let Some(lh) = s.line_height {
            self.style.line_height = lh;
            self.set.mark(TextFields::LINE_HEIGHT);
        }
        if let Some(a) = s.text_align {
            self.style.align = a;
            self.set.mark(TextFields::ALIGN);
        }
        if let Some(ls) = s.letter_spacing {
            self.style.letter_spacing = ls;
            self.set.mark(TextFields::LETTER_SPACING);
        }
        if let Some(i) = s.italic {
            self.style.italic = i;
            self.set.mark(TextFields::ITALIC);
        }
        if let Some(u) = s.underline {
            self.style.underline = u;
            self.set.mark(TextFields::UNDERLINE);
        }
        if let Some(st) = s.strikethrough {
            self.style.strikethrough = st;
            self.set.mark(TextFields::STRIKETHROUGH);
        }
        if let Some(f) = &s.font_family {
            self.style.font_family = Some(f.clone());
            self.set.mark(TextFields::FONT_FAMILY);
        }
        if let Some(m) = s.max_lines {
            self.style.max_lines = Some(m);
            self.set.mark(TextFields::MAX_LINES);
        }
        if let Some(e) = s.ellipsis {
            self.style.ellipsis = e;
            self.set.mark(TextFields::ELLIPSIS);
        }
        if let Some(sw) = s.soft_wrap {
            self.style.soft_wrap = sw;
            self.set.mark(TextFields::SOFT_WRAP);
        }
        crate::style::styled(self, s)
    }
}

pebbles_core::render_widget!(Text);

impl RenderWidget for Text {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(RenderParagraph::new(self.data.clone(), self.resolved()))
    }

    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(p) = object.downcast_mut::<RenderParagraph>() {
            p.text = self.data.clone();
            p.style = self.resolved();
        }
    }
}

// ---------------------------------------------------------------------------
// Rich text — one paragraph, many styled ranges (Flutter's TextSpan vocabulary)
// ---------------------------------------------------------------------------

/// One styled run of a rich paragraph. Build with [`span`], chain the style
/// setters, and hand a `Vec<TextSpan>` to [`text_rich`]. The whole paragraph
/// shapes as ONE layout with per-range styles — word-wrap, spacing, and BiDi are
/// the text engine's job, never a widget-per-word composition.
#[derive(Clone)]
pub struct TextSpan {
    pub text: String,
    pub weight: Option<f32>,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: Option<Color>,
    pub family: Option<String>,
    pub size: Option<f32>,
    /// Rounded background behind the run (inline code chips).
    pub chip: Option<Color>,
    /// Navigation target — taps resolve through [`RichText::on_link`].
    pub link: Option<String>,
}

/// A plain [`TextSpan`] over `text` — chain setters to style it.
pub fn span(text: impl Into<String>) -> TextSpan {
    TextSpan {
        text: text.into(),
        weight: None,
        italic: false,
        underline: false,
        strikethrough: false,
        color: None,
        family: None,
        size: None,
        chip: None,
        link: None,
    }
}

impl TextSpan {
    pub fn weight(mut self, w: f32) -> Self {
        self.weight = Some(w);
        self
    }
    /// Semibold (600).
    pub fn semibold(mut self) -> Self {
        self.weight = Some(600.0);
        self
    }
    /// Bold (700).
    pub fn bold(mut self) -> Self {
        self.weight = Some(700.0);
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.family = Some(family.into());
        self
    }
    /// Absolute font-size override (px).
    pub fn size(mut self, px: f32) -> Self {
        self.size = Some(px);
        self
    }
    /// Rounded background chip behind the run (inline code).
    pub fn chip(mut self, bg: Color) -> Self {
        self.chip = Some(bg);
        self
    }
    /// Mark the run as a link to `url` (style it explicitly — links don't
    /// auto-underline).
    pub fn link(mut self, url: impl Into<String>) -> Self {
        self.link = Some(url.into());
        self
    }
}

/// A rich paragraph: many styled ranges, one shaped layout. Links resolve by
/// GEOMETRY — the render paragraph publishes each link range's laid-out boxes and
/// the tap handler hit-tests them — so a link that wraps across lines is exactly
/// as clickable as its glyphs, with no per-word widgets.
pub struct RichText {
    base: ParagraphStyle,
    spans: Vec<TextSpan>,
    on_link: Option<LinkCb>,
    on_link_hover: Option<LinkHoverCb>,
    selectable: bool,
    on_selection: Option<LinkCb>,
    group: Option<(SelectionGroup, usize)>,
    keyboard: bool,
}

/// Build a rich paragraph from spans. Base style via the setters.
pub fn text_rich(spans: Vec<TextSpan>) -> RichText {
    RichText {
        base: ParagraphStyle::default(),
        spans,
        on_link: None,
        on_link_hover: None,
        selectable: false,
        on_selection: None,
        group: None,
        keyboard: false,
    }
}

impl RichText {
    /// Base font size (spans may override per-range).
    pub fn size(mut self, px: f32) -> Self {
        self.base.font_size = px;
        self
    }
    /// Base text color.
    pub fn color(mut self, color: Color) -> Self {
        self.base.color = color;
        self
    }
    pub fn line_height(mut self, factor: f32) -> Self {
        self.base.line_height = factor;
        self
    }
    /// Extra spacing between glyphs in logical px (0 = none).
    pub fn letter_spacing(mut self, px: f32) -> Self {
        self.base.letter_spacing = px;
        self
    }
    pub fn align(mut self, align: TextAlign) -> Self {
        self.base.align = align;
        self
    }
    pub fn font_family(mut self, family: impl Into<String>) -> Self {
        self.base.font_family = Some(family.into());
        self
    }
    /// Base weight (spans may override).
    pub fn weight(mut self, w: f32) -> Self {
        self.base.weight = w;
        self
    }
    /// Style the whole paragraph from an explicit [`ParagraphStyle`].
    pub fn paragraph_style(mut self, style: ParagraphStyle) -> Self {
        self.base = style;
        self
    }
    /// Disable soft wrapping: lines break only at `\n` (code blocks).
    pub fn soft_wrap(mut self, wrap: bool) -> Self {
        self.base.soft_wrap = wrap;
        self
    }
    /// Called with the URL when any link span is tapped.
    pub fn on_link(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.on_link = Some(std::rc::Rc::new(f));
        self
    }
    /// Called as the pointer moves over the paragraph: `Some(url)` when it is over
    /// a link span, `None` when it leaves the last link. The hook for link
    /// hover-previews / status-bar URL display; resolves by the same laid-out link
    /// boxes as [`on_link`](Self::on_link), so multi-line links hover exactly.
    pub fn on_link_hover(mut self, f: impl Fn(Option<&str>) + 'static) -> Self {
        self.on_link_hover = Some(std::rc::Rc::new(f));
        self
    }
    /// Make the paragraph's text selectable: drag to select, and the selection is
    /// copied to the clipboard on release. Links still tap (a click) while a drag
    /// selects. See [`on_selection`](Self::on_selection) to observe the text.
    pub fn selectable(mut self) -> Self {
        self.selectable = true;
        self
    }
    /// Join this leaf into a [`SelectionGroup`] at document position `index`, so a
    /// drag — and the copy on release — spans **across** all leaves in the group
    /// (document-wide selection), not just this one.
    pub fn selectable_in(mut self, group: SelectionGroup, index: usize) -> Self {
        self.selectable = true;
        self.group = Some((group, index));
        self
    }
    /// Called with the selected text when a selection drag ends (implies
    /// [`selectable`](Self::selectable)).
    pub fn on_selection(mut self, f: impl Fn(&str) + 'static) -> Self {
        self.selectable = true;
        self.on_selection = Some(std::rc::Rc::new(f));
        self
    }
    /// Make the paragraph a keyboard focus stop that traverses its links: Tab
    /// focuses it, Left/Right (or Up/Down) cycle between link spans painting a
    /// focus ring, and Enter activates the focused link through
    /// [`on_link`](Self::on_link). Arrowing past the last/first link releases
    /// focus so Tab moves on. A no-op unless the paragraph has link spans and an
    /// [`on_link`](Self::on_link) handler.
    pub fn keyboard_nav(mut self) -> Self {
        self.keyboard = true;
        self
    }
}

/// The leaf render widget behind [`RichText`] (post-resolution: byte ranges,
/// URL indices, and the shared link-box cell are already computed).
#[derive(Clone)]
struct RichTextLeaf {
    text: String,
    style: ParagraphStyle,
    spans: Vec<pebbles_render::TextSpanStyle>,
    boxes: Option<LinkBoxes>,
    /// When set, the paragraph publishes its shaped layout under this id for
    /// point→byte selection hit-testing, and paints `selection`.
    select_id: Option<u64>,
    selection: Option<(usize, usize)>,
    /// When set, the paragraph paints a keyboard-focus ring behind the link span
    /// with this URL index (keyboard link traversal).
    focused_link: Option<usize>,
}

pebbles_core::render_widget!(RichTextLeaf);

impl RenderWidget for RichTextLeaf {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut p = RenderParagraph::with_spans(self.text.clone(), self.style.clone(), self.spans.clone());
        p.link_boxes = self.boxes.clone();
        p.select_id = self.select_id;
        p.selection = self.selection;
        p.focused_link = self.focused_link;
        Box::new(p)
    }

    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(p) = object.downcast_mut::<RenderParagraph>() {
            p.text = self.text.clone();
            p.style = self.style.clone();
            p.spans = self.spans.clone();
            p.link_boxes = self.boxes.clone();
            p.select_id = self.select_id;
            p.selection = self.selection;
            p.focused_link = self.focused_link;
        }
    }
}

/// Resolve spans → concatenated text + byte-ranged style overrides + link URLs.
fn resolve_spans(spans: &[TextSpan]) -> (String, Vec<pebbles_render::TextSpanStyle>, Vec<String>) {
    let mut text = String::new();
    let mut rspans: Vec<pebbles_render::TextSpanStyle> = Vec::with_capacity(spans.len());
    let mut urls: Vec<String> = Vec::new();
    for s in spans {
        let start = text.len();
        text.push_str(&s.text);
        let mut rs = pebbles_render::TextSpanStyle::new(start..text.len());
        rs.weight = s.weight;
        rs.italic = s.italic;
        rs.underline = s.underline;
        rs.strikethrough = s.strikethrough;
        rs.color = s.color;
        rs.family = s.family.clone();
        rs.size = s.size;
        rs.chip = s.chip;
        if let Some(url) = &s.link {
            rs.link = Some(urls.len());
            urls.push(url.clone());
        }
        rspans.push(rs);
    }
    (text, rspans, urls)
}

/// Wire link tap + hover onto a [`GestureDetector`] over a link-bearing paragraph.
fn wire_links(
    mut gd: crate::widgets::GestureDetector,
    boxes: LinkBoxes,
    urls: std::rc::Rc<Vec<String>>,
    on_link: Option<LinkCb>,
    on_link_hover: Option<LinkHoverCb>,
) -> crate::widgets::GestureDetector {
    if let Some(f) = on_link {
        let (boxes, urls) = (boxes.clone(), urls.clone());
        gd = gd.on_tap(pebbles_core::action_event(move |e| {
            let hit =
                boxes.borrow().iter().find(|(r, _)| r.contains(e.position.to_point())).map(|&(_, ix)| ix);
            if let Some(ix) = hit
                && let Some(url) = urls.get(ix)
            {
                f(url);
            }
        }));
    }
    if let Some(h) = on_link_hover {
        let last = std::rc::Rc::new(std::cell::Cell::new(None::<usize>));
        let (boxes, urls, last_m, h_m) = (boxes.clone(), urls.clone(), last.clone(), h.clone());
        gd = gd.on_hover_move(pebbles_core::action_event(move |e| {
            let hit =
                boxes.borrow().iter().find(|(r, _)| r.contains(e.position.to_point())).map(|&(_, ix)| ix);
            if hit != last_m.get() {
                last_m.set(hit);
                h_m(hit.and_then(|ix| urls.get(ix)).map(String::as_str));
            }
        }));
        gd = gd.on_hover_exit(move || {
            if last.get().is_some() {
                last.set(None);
                h(None);
            }
        });
    }
    gd
}

/// Props for the selectable rich-text component (needs per-widget selection state).
#[derive(Clone)]
struct SelectableRichProps {
    base: ParagraphStyle,
    spans: Vec<TextSpan>,
    on_link: Option<LinkCb>,
    on_link_hover: Option<LinkHoverCb>,
    on_selection: Option<LinkCb>,
    group: Option<(SelectionGroup, usize)>,
}

/// Normalize a cross-selection into `(lo, hi)` document positions.
fn cross_bounds(s: CrossSel) -> ((usize, usize), (usize, usize)) {
    let (ai, ab, fi, fb) = s;
    if (ai, ab) <= (fi, fb) { ((ai, ab), (fi, fb)) } else { ((fi, fb), (ai, ab)) }
}

/// A leaf that participates in a document-wide [`SelectionGroup`]: it registers
/// its shaped layout + global rect, paints its slice of the group selection, and
/// its drag maps global points to whichever leaf is under the pointer.
fn render_group_leaf(p: &SelectableRichProps, group: SelectionGroup, index: usize) -> AnyWidget {
    let sid = leaf_select_id(group.id, index);
    let gid = group.id;
    pebbles_core::create_cleanup(move || {
        pebbles_render::text_edit::clear(sid);
        SEL_GROUPS.with(|g| {
            if let Some(m) = g.borrow_mut().get_mut(&gid) {
                m.remove(&index);
            }
        });
    });
    let (text, rspans, urls) = resolve_spans(&p.spans);
    let text = std::rc::Rc::new(text);
    let rect = pebbles_core::use_bounds();
    SEL_GROUPS.with(|g| {
        g.borrow_mut().entry(gid).or_default().insert(index, (sid, rect, (*text).clone()));
    });
    // This leaf's slice of the group selection.
    let local = group.sel.get().and_then(|s| {
        let (lo, hi) = cross_bounds(s);
        if index < lo.0 || index > hi.0 {
            return None;
        }
        let start = if index == lo.0 { lo.1 } else { 0 };
        let end = if index == hi.0 { hi.1 } else { text.len() };
        (start != end).then_some((start, end))
    });
    let has_links = (p.on_link.is_some() || p.on_link_hover.is_some()) && !urls.is_empty();
    let boxes = has_links.then(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
    let leaf = RichTextLeaf {
        text: (*text).clone(),
        style: p.base.clone(),
        spans: rspans,
        boxes: boxes.clone(),
        select_id: Some(sid),
        selection: local,
        focused_link: None,
    };
    let mut gd = crate::widgets::GestureDetector::new(leaf);
    if let Some(boxes) = boxes {
        gd = wire_links(gd, boxes, std::rc::Rc::new(urls), p.on_link.clone(), p.on_link_hover.clone());
    }
    let selg = group.sel;
    gd = gd.on_pan_start(pebbles_core::action_event(move |e| {
        if let Some(b) = pebbles_render::text_edit::hit(sid, e.position.x, e.position.y) {
            selg.set(Some((index, b, index, b)));
        }
    }));
    gd = gd.on_pan_update(pebbles_core::action_event(move |e| {
        // The drag is captured to this leaf, but the pointer may be over another —
        // map the GLOBAL point to whichever group leaf is under it. When global
        // rects aren't published (no shell bounds), fall back to this leaf locally.
        let target = group_leaf_at(gid, e.global.x, e.global.y)
            .and_then(|(tidx, tsid, lx, ly)| pebbles_render::text_edit::hit(tsid, lx, ly).map(|b| (tidx, b)))
            .or_else(|| pebbles_render::text_edit::hit(sid, e.position.x, e.position.y).map(|b| (index, b)));
        if let Some((tidx, b)) = target {
            selg.update(|s| {
                let (ai, ab) = s.map(|(ai, ab, _, _)| (ai, ab)).unwrap_or((tidx, b));
                *s = Some((ai, ab, tidx, b));
            });
        }
    }));
    let on_selection = p.on_selection.clone();
    gd = gd.on_pan_end(move || {
        let Some(s) = selg.peek() else { return };
        let (lo, hi) = cross_bounds(s);
        let mut out = String::new();
        SEL_GROUPS.with(|g| {
            if let Some(leaves) = g.borrow().get(&gid) {
                for i in lo.0..=hi.0 {
                    let Some((_, _, t)) = leaves.get(&i) else { continue };
                    let start = if i == lo.0 { lo.1.min(t.len()) } else { 0 };
                    let end = if i == hi.0 { hi.1.min(t.len()) } else { t.len() };
                    if start < end && t.is_char_boundary(start) && t.is_char_boundary(end) {
                        if !out.is_empty() {
                            out.push('\n');
                        }
                        out.push_str(&t[start..end]);
                    }
                }
            }
        });
        if !out.is_empty() {
            pebbles_core::clipboard::write(&out);
            if let Some(cb) = &on_selection {
                cb(&out);
            }
        }
    });
    gd.cursor(pebbles_render::Cursor::Text).into_widget()
}

/// Selectable rich text: drag selects (published layout → point→byte via
/// `text_edit`), the selection paints, and it copies to the clipboard on release.
fn render_selectable_rich(p: &SelectableRichProps) -> AnyWidget {
    if let Some((group, index)) = p.group {
        return render_group_leaf(p, group, index);
    }
    let sel = pebbles_core::create_signal(None::<(usize, usize)>);
    let id = pebbles_core::owner_id().unwrap_or(0) ^ 0x5E1E_C700_0000_0000;
    pebbles_core::create_cleanup(move || pebbles_render::text_edit::clear(id));

    let (text, rspans, urls) = resolve_spans(&p.spans);
    let text = std::rc::Rc::new(text);
    let has_links = (p.on_link.is_some() || p.on_link_hover.is_some()) && !urls.is_empty();
    let boxes = has_links.then(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
    let leaf = RichTextLeaf {
        text: (*text).clone(),
        style: p.base.clone(),
        spans: rspans,
        boxes: boxes.clone(),
        select_id: Some(id),
        selection: sel.get(),
        focused_link: None,
    };
    let mut gd = crate::widgets::GestureDetector::new(leaf);
    if let Some(boxes) = boxes {
        gd = wire_links(gd, boxes, std::rc::Rc::new(urls), p.on_link.clone(), p.on_link_hover.clone());
    }

    // Drag → selection (anchor at press, focus follows), copy on release.
    gd = gd.on_pan_start(pebbles_core::action_event(move |e| {
        let b = pebbles_render::text_edit::hit(id, e.position.x, e.position.y);
        sel.set(b.map(|b| (b, b)));
    }));
    gd = gd.on_pan_update(pebbles_core::action_event(move |e| {
        if let Some(b) = pebbles_render::text_edit::hit(id, e.position.x, e.position.y) {
            sel.update(|s| {
                let anchor = s.map(|(a, _)| a).unwrap_or(b);
                *s = Some((anchor, b));
            });
        }
    }));
    let on_selection = p.on_selection.clone();
    gd = gd.on_pan_end(move || {
        if let Some((a, f)) = sel.peek() {
            let (lo, hi) = (a.min(f), a.max(f));
            if lo < hi && text.is_char_boundary(lo) && text.is_char_boundary(hi) {
                let picked = &text[lo..hi];
                pebbles_core::clipboard::write(picked);
                if let Some(cb) = &on_selection {
                    cb(picked);
                }
            }
        }
    });
    gd.cursor(pebbles_render::Cursor::Text).into_widget()
}

/// Props for the keyboard-navigable rich-text component.
#[derive(Clone)]
struct KeyboardRichProps {
    base: ParagraphStyle,
    spans: Vec<TextSpan>,
    on_link: Option<LinkCb>,
    on_link_hover: Option<LinkHoverCb>,
}

/// Keyboard link traversal: a focus stop that cycles its link spans with the
/// arrow keys (painting a focus ring) and activates the focused link on
/// Enter/Space. Arrowing past the first/last link returns `false` so Tab moves
/// focus on. Mouse taps still resolve links by geometry (via [`wire_links`]).
fn render_keyboard_rich(p: &KeyboardRichProps) -> AnyWidget {
    let node = pebbles_core::create_focus();
    let focused_idx = pebbles_core::create_signal(0usize);
    let (text, rspans, urls) = resolve_spans(&p.spans);
    let urls = std::rc::Rc::new(urls);
    let n = urls.len();

    // No links, or no tap handler to activate them → an inert, non-focusable
    // paragraph (registering a focus stop with nothing to do would be a dead Tab).
    if n == 0 || p.on_link.is_none() {
        let leaf = RichTextLeaf {
            text,
            style: p.base.clone(),
            spans: rspans,
            boxes: None,
            select_id: None,
            selection: None,
            focused_link: None,
        };
        return leaf.into_widget();
    }

    let is_focused = node.is_focused();

    // Activate the currently-focused link through `on_link`.
    let act_urls = urls.clone();
    let act_link = p.on_link.clone();
    let activate: std::rc::Rc<dyn Fn()> = std::rc::Rc::new(move || {
        if let Some(url) = act_urls.get(focused_idx.peek())
            && let Some(f) = &act_link
        {
            f(url);
        }
    });

    // Arrow keys cycle links; Enter activates. Returning false at an edge (or on
    // an unrelated key) releases the key so Tab/scroll can claim it.
    let keys_activate = activate.clone();
    node.register_keys(std::rc::Rc::new(move |k: pebbles_core::KeyInput| -> bool {
        use pebbles_core::{KeyInput, Motion};
        match k {
            KeyInput::Move { motion: Motion::Right | Motion::Down | Motion::WordRight, .. } => {
                let i = focused_idx.peek();
                if i + 1 < n {
                    focused_idx.set(i + 1);
                    true
                } else {
                    false
                }
            }
            KeyInput::Move { motion: Motion::Left | Motion::Up | Motion::WordLeft, .. } => {
                let i = focused_idx.peek();
                if i > 0 {
                    focused_idx.set(i - 1);
                    true
                } else {
                    false
                }
            }
            KeyInput::Enter => {
                keys_activate();
                true
            }
            _ => false,
        }
    }));

    // `register` makes this a Tab focus stop and handles Space activation (the
    // shell routes Space to `dispatch_activate`; Enter is claimed above).
    node.register(activate, None, false);

    let boxes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let leaf = RichTextLeaf {
        text,
        style: p.base.clone(),
        spans: rspans,
        boxes: Some(boxes.clone()),
        select_id: None,
        selection: None,
        focused_link: is_focused.then(|| focused_idx.get().min(n - 1)),
    };
    let mut gd = crate::widgets::GestureDetector::new(leaf);
    gd = wire_links(gd, boxes, urls, p.on_link.clone(), p.on_link_hover.clone());
    // A pointer press focuses the paragraph so keyboard traversal picks up there.
    gd = gd.on_pointer_down(move || node.request_focus());
    gd.cursor(pebbles_render::Cursor::Pointer).into_widget()
}

impl IntoWidget for RichText {
    fn into_widget(self) -> AnyWidget {
        if self.keyboard && !self.selectable {
            return pebbles_core::component_props(
                render_keyboard_rich,
                KeyboardRichProps {
                    base: self.base,
                    spans: self.spans,
                    on_link: self.on_link,
                    on_link_hover: self.on_link_hover,
                },
            )
            .into_widget();
        }
        if self.selectable {
            return pebbles_core::component_props(
                render_selectable_rich,
                SelectableRichProps {
                    base: self.base,
                    spans: self.spans,
                    on_link: self.on_link,
                    on_link_hover: self.on_link_hover,
                    on_selection: self.on_selection,
                    group: self.group,
                },
            )
            .into_widget();
        }
        let (text, rspans, urls) = resolve_spans(&self.spans);
        // Link geometry is needed for tap AND/OR hover; publish boxes if either is wired.
        let interactive = (self.on_link.is_some() || self.on_link_hover.is_some()) && !urls.is_empty();
        let boxes = interactive.then(|| std::rc::Rc::new(std::cell::RefCell::new(Vec::new())));
        let leaf = RichTextLeaf {
            text,
            style: self.base,
            spans: rspans,
            boxes: boxes.clone(),
            select_id: None,
            selection: None,
            focused_link: None,
        };
        let Some(boxes) = boxes else {
            return leaf.into_widget();
        };
        let gd = crate::widgets::GestureDetector::new(leaf);
        let gd = wire_links(gd, boxes, std::rc::Rc::new(urls), self.on_link, self.on_link_hover);
        gd.cursor(pebbles_render::Cursor::Default).into_widget()
    }
}
