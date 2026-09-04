//! [`ListView`] — a **virtualized**, build-on-demand list (Flutter's
//! `ListView.builder` with a fixed item extent). Only the items intersecting the
//! viewport (plus a small overscan) are built each frame, so a list of a million
//! rows costs the same as a screenful.
//!
//! It is a *controlled* scroll view: the component owns the offset as a signal, so
//! a wheel/scrollbar scroll re-renders it and the visible window rebuilds. Wheel +
//! scrollbar drag are routed to the signal via `pebbles_core::scroll`, and the
//! viewport extent is read back from `pebbles_render::scroll_metrics`.

use std::cell::RefCell;
use std::rc::Rc;

use pebbles_foundation::Axis;
use pebbles_render::{RenderList, RenderMeasureProbe, RenderObject, ScrollbarStyle, scroll_metrics};

use crate::widgets::{Positioned, stack};
use pebbles_core::scroll::{self, ScrollTo};
use pebbles_core::widget::{AnyWidget, IntoWidget, RenderWidget};
use pebbles_core::reactive::request_frame;
use pebbles_core::{
    Signal, animate_to, component_props, create_cleanup, create_signal, owner_id,
};

/// How many extra items to build above/below the viewport (smooths fast flings).
const OVERSCAN: isize = 3;

// ---------------------------------------------------------------------------
// ExtentCache — per-item extents for auto-measured lists (A1).
// ---------------------------------------------------------------------------

/// The live extent bookkeeping of a [`ListView::builder_auto`] list: real
/// measurements replace the estimate as items lay out, and lazy prefix sums give
/// item positions (`offset_of`), content extent (`total`) and index lookup
/// (`index_at`).
#[derive(Default)]
struct ExtentCache {
    /// `Some(e)` = measured; `None` = use `estimate`.
    measured: Vec<Option<f64>>,
    /// Lazily-rebuilt prefix sums of `measured` (with estimates filled in).
    prefix: Vec<f64>,
    total_cache: f64,
    dirty: bool,
    /// Fallback extent for unmeasured items (the `.estimated_extent` knob).
    estimate: f64,
    /// Optional per-index estimator — a caller who knows each item's KIND can
    /// seed far better first guesses than one global number (stable scrollbar,
    /// fewer corrective passes after deep jumps).
    estimates: Option<Rc<dyn Fn(usize) -> f64>>,
}

impl ExtentCache {
    fn extent_of(&self, i: usize) -> f64 {
        self.measured
            .get(i)
            .copied()
            .flatten()
            .unwrap_or_else(|| {
                self.estimates.as_ref().map(|f| f(i)).unwrap_or(self.estimate)
            })
            .max(1.0)
    }

    fn rebuild(&mut self) {
        let mut pre = Vec::with_capacity(self.measured.len() + 1);
        pre.push(0.0);
        let mut acc = 0.0;
        for i in 0..self.measured.len() {
            acc += self.extent_of(i);
            pre.push(acc);
        }
        self.total_cache = acc;
        self.prefix = pre;
        self.dirty = false;
    }

    /// The content-space top of item `i`.
    fn offset_of(&mut self, i: usize) -> f64 {
        if self.dirty {
            self.rebuild();
        }
        self.prefix.get(i).copied().unwrap_or(self.total_cache)
    }

    /// The index of the item containing `offset` (binary search over the prefix).
    fn index_at(&mut self, offset: f64) -> usize {
        if self.dirty {
            self.rebuild();
        }
        self.prefix.partition_point(|&top| top <= offset).saturating_sub(1)
    }

    /// The current total content extent.
    fn total(&mut self) -> f64 {
        if self.dirty {
            self.rebuild();
        }
        self.total_cache
    }

    /// Record a real measurement. Returns the extent delta (vs the estimate or
    /// the previous measurement) when it changed by more than 0.5px.
    fn set(&mut self, i: usize, v: f64) -> Option<f64> {
        if i >= self.measured.len() {
            return None;
        }
        let old = self.measured[i];
        let changed = match old {
            Some(o) => (o - v).abs() > 0.5,
            None => true,
        };
        if changed {
            let was = old.unwrap_or_else(|| {
                self.estimates.as_ref().map(|f| f(i)).unwrap_or(self.estimate)
            });
            self.measured[i] = Some(v);
            self.dirty = true;
            Some(v - was)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// MeasureProbe — reports an item's laid-out extent into the cache (A1).
// ---------------------------------------------------------------------------

/// A layout pass-through that reports its child's main-axis extent into the
/// list's [`ExtentCache`] after every layout (the cache ignores no-op updates).
#[derive(Clone)]
struct MeasureProbe {
    axis: Axis,
    cache: Rc<RefCell<ExtentCache>>,
    bump: Signal<u64>,
    offset: Signal<f64>,
    index: usize,
    child: Option<AnyWidget>,
}

pebbles_core::render_widget!(MeasureProbe);

impl RenderWidget for MeasureProbe {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        let mut probe = RenderMeasureProbe::new(self.axis, Some(self.report()));
        probe.unbound = true;
        Box::new(probe)
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(p) = object.downcast_mut::<RenderMeasureProbe>() {
            p.axis = self.axis;
            p.report = Some(self.report());
            p.unbound = true;
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

impl MeasureProbe {
    /// The post-layout callback: writes the measurement into the cache, keeps
    /// content under the cursor anchored when an item ABOVE the viewport grows,
    /// and schedules one corrective rebuild.
    fn report(&self) -> Rc<dyn Fn(f64)> {
        let cache = self.cache.clone();
        let bump = self.bump;
        let offset = self.offset;
        let index = self.index;
        Rc::new(move |extent: f64| {
            let delta = cache.borrow_mut().set(index, extent);
            if let Some(delta) = delta {
                let above = cache.borrow_mut().offset_of(index) < offset.peek();
                if above {
                    offset.set((offset.peek() + delta).max(0.0));
                }
                bump.set(bump.peek().wrapping_add(1));
                request_frame();
            }
        })
    }
}

// ---------------------------------------------------------------------------
// ScrollController — programmatic control of a list's offset.
// ---------------------------------------------------------------------------

/// A handle to a scrollable list's position. Create one with
/// [`use_scroll_controller`], pass it to [`ListView::controller`], and call
/// [`jump_to`](ScrollController::jump_to) / [`animate_to`](ScrollController::animate_to)
/// / [`scroll_to_index`](ScrollController::scroll_to_index) to drive it.
#[derive(Clone, Copy)]
pub struct ScrollController {
    id: u64,
    offset: Signal<f64>,
}

/// Create a [`ScrollController`] (call at the top level of a component, like a
/// signal — it persists across renders).
pub fn use_scroll_controller() -> ScrollController {
    let offset = create_signal(0.0_f64);
    ScrollController { id: offset.raw_id(), offset }
}

impl ScrollController {
    /// Build a controller over an existing offset signal (internal — composite
    /// widgets like the carousel drive a list through their own signals).
    #[doc(hidden)]
    pub fn from_parts(id: u64, offset: Signal<f64>) -> Self {
        ScrollController { id, offset }
    }
    fn max(&self) -> f64 {
        scroll_metrics::get(self.id).map(|m| (m.content - m.viewport).max(0.0)).unwrap_or(0.0)
    }
    /// The current pixel offset.
    pub fn offset(&self) -> f64 {
        self.offset.peek()
    }
    /// The reactive offset signal (doc-hidden — composite widgets like the
    /// sticky/collapsing headers read it to re-render on every scroll).
    #[doc(hidden)]
    pub fn offset_signal(&self) -> Signal<f64> {
        self.offset
    }
    /// The viewport registry id (doc-hidden — tests/tooling).
    #[doc(hidden)]
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Jump instantly to a pixel offset.
    pub fn jump_to(&self, px: f64) {
        self.offset.set(px.clamp(0.0, self.max()));
    }
    /// Smoothly animate to a pixel offset.
    pub fn animate_to(&self, px: f64) {
        animate_to(self.offset, px.clamp(0.0, self.max()), 0.35);
    }
    /// Animate so item `index` (of `item_extent` each) reaches the top.
    pub fn scroll_to_index(&self, index: usize, item_extent: f64) {
        self.animate_to(index as f64 * item_extent);
    }
    /// Animate so item `index` of an AUTO-MEASURED list reaches the top — the
    /// offset resolves through that list's live extent cache (estimates for
    /// unmeasured items; the list self-corrects as they measure). Only works
    /// while the list is mounted.
    pub fn scroll_to_index_auto(&self, index: usize) {
        if let Some(top) = scroll::index_of(self.id, index) {
            self.animate_to(top);
        }
    }
}

// ---------------------------------------------------------------------------
// Low-level render widget backing RenderList.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct ListViewport {
    axis: Axis,
    offset: f64,
    content_extent: f64,
    id: u64,
    scrollbar: ScrollbarStyle,
    child: Option<AnyWidget>,
}

impl ListViewport {
    fn make(&self) -> RenderList {
        let mut r = RenderList::new(self.axis);
        r.offset = self.offset;
        r.content_extent = self.content_extent;
        r.id = self.id;
        r.scrollbar = self.scrollbar;
        r
    }
}

pebbles_core::render_widget!(ListViewport);

impl RenderWidget for ListViewport {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(self.make())
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(r) = object.downcast_mut::<RenderList>() {
            *r = self.make();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

// ---------------------------------------------------------------------------
// ListView — the virtualized public widget.
// ---------------------------------------------------------------------------

/// A virtualized, fixed-extent list.
pub struct ListView {
    count: usize,
    item_extent: f64,
    axis: Axis,
    scrollbar: ScrollbarStyle,
    controller: Option<ScrollController>,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    /// `(separator extent, builder)` when built with [`separated`](ListView::separated).
    separator: Option<(f64, Rc<dyn Fn(usize) -> AnyWidget>)>,
    /// Per-item extents when built with [`variable`](ListView::variable).
    extents: Option<Rc<dyn Fn(usize) -> f64>>,
    /// Auto-MEASURED extents when built with [`builder_auto`](ListView::builder_auto).
    auto: bool,
    /// Fallback extent for unmeasured items in auto mode.
    estimate: f64,
    /// Per-index extent estimator for auto mode (`.estimated_extent_of`).
    estimate_fn: Option<Rc<dyn Fn(usize) -> f64>>,
    /// Extra build margin (logical px) on EACH side of the viewport: items within
    /// it exist before they scroll into view, so flings don't pop blanks.
    cache_extent: f64,
    /// Snap the controlled offset to multiples of this (0 = off) — paged lists
    /// and carousels.
    snap: f64,
    reverse: bool,
    padding: Option<pebbles_foundation::EdgeInsets>,
}

impl ListView {
    /// Build a vertical list of `count` rows, each `item_extent` tall; `builder`
    /// is called only for the visible rows.
    pub fn builder<W: IntoWidget>(
        count: usize,
        item_extent: f64,
        builder: impl Fn(usize) -> W + 'static,
    ) -> Self {
        ListView {
            count,
            item_extent,
            axis: Axis::Vertical,
            scrollbar: ScrollbarStyle::default(),
            controller: None,
            builder: Rc::new(move |i| builder(i).into_widget()),
            separator: None,
            extents: None,
            auto: false,
            estimate: 40.0,
            estimate_fn: None,
            cache_extent: 250.0,
            snap: 0.0,
            reverse: false,
            padding: None,
        }
    }

    /// Build a list whose rows MEASURE themselves (A1): no extent argument —
    /// items are laid out at their natural size and the virtualization learns
    /// their real extents as they scroll into view (corrective passes converge
    /// in a frame or two). Use when row heights can't be precomputed; prefer the
    /// fixed [`builder`](ListView::builder) when they can. Items with local
    /// state must be their own components (the builder re-runs per rebuild
    /// window).
    pub fn builder_auto<W: IntoWidget>(
        count: usize,
        builder: impl Fn(usize) -> W + 'static,
    ) -> Self {
        ListView {
            count,
            item_extent: 1.0,
            axis: Axis::Vertical,
            scrollbar: ScrollbarStyle::default(),
            controller: None,
            builder: Rc::new(move |i| builder(i).into_widget()),
            separator: None,
            extents: None,
            auto: true,
            estimate: 40.0,
            estimate_fn: None,
            cache_extent: 250.0,
            snap: 0.0,
            reverse: false,
            padding: None,
        }
    }

    /// A list whose items have their OWN extents (Flutter's variable-extent
    /// delegate, Rust-style): `extents(i)` returns item `i`'s height (or width,
    /// when horizontal). Virtualization stays — only the visible items build.
    /// Best for feeds/messages; prefer the fixed [`builder`](ListView::builder)
    /// for uniform rows (O(1) window math).
    pub fn variable<W: IntoWidget>(
        count: usize,
        extents: impl Fn(usize) -> f64 + 'static,
        builder: impl Fn(usize) -> W + 'static,
    ) -> Self {
        ListView {
            count,
            item_extent: 1.0,
            axis: Axis::Vertical,
            scrollbar: ScrollbarStyle::default(),
            controller: None,
            builder: Rc::new(move |i| builder(i).into_widget()),
            separator: None,
            extents: Some(Rc::new(extents)),
            auto: false,
            estimate: 40.0,
            estimate_fn: None,
            cache_extent: 250.0,
            snap: 0.0,
            reverse: false,
            padding: None,
        }
    }

    /// A list with separators between the items (Flutter's `ListView.separated`):
    /// each item is `item_extent` tall, each separator `separator_extent` tall —
    /// both virtualized, no separator after the last item.
    pub fn separated<W: IntoWidget, S: IntoWidget>(
        count: usize,
        item_extent: f64,
        separator_extent: f64,
        item: impl Fn(usize) -> W + 'static,
        separator: impl Fn(usize) -> S + 'static,
    ) -> Self {
        ListView {
            count,
            item_extent,
            axis: Axis::Vertical,
            scrollbar: ScrollbarStyle::default(),
            controller: None,
            builder: Rc::new(move |i| item(i).into_widget()),
            separator: Some((separator_extent, Rc::new(move |i| separator(i).into_widget()))),
            extents: None,
            auto: false,
            estimate: 40.0,
            estimate_fn: None,
            cache_extent: 250.0,
            snap: 0.0,
            reverse: false,
            padding: None,
        }
    }

    /// Lay the list out horizontally instead (each item `item_extent` wide).
    pub fn horizontal(mut self) -> Self {
        self.axis = Axis::Horizontal;
        self
    }
    /// Customize the scrollbar.
    pub fn scrollbar(mut self, style: ScrollbarStyle) -> Self {
        self.scrollbar = style;
        self
    }
    /// Drive this list programmatically with a [`ScrollController`].
    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }
    /// Reverse the list: item 0 sits at the END (bottom / right) and the list
    /// starts scrolled there — Flutter's `reverse` (chat logs, consoles).
    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }
    /// Outer padding around the scrollable content (part of the scrollable
    /// extent — the first/last items don't touch the edges).
    pub fn padding(mut self, insets: pebbles_foundation::EdgeInsets) -> Self {
        self.padding = Some(insets);
        self
    }
    /// The extent assumed for not-yet-measured rows in auto mode (default 40).
    /// A good guess reduces the corrective passes after deep jumps.
    pub fn estimated_extent(mut self, extent: f64) -> Self {
        self.estimate = extent.max(1.0);
        self
    }
    /// A PER-INDEX extent estimate for auto mode: when the caller knows each
    /// item's kind (a heading vs a code block vs a table), per-kind guesses keep
    /// the scrollbar stable and deep jumps accurate before measurement.
    pub fn estimated_extent_of(mut self, f: impl Fn(usize) -> f64 + 'static) -> Self {
        self.estimate_fn = Some(Rc::new(f));
        self
    }
    /// Extra build margin (logical px) on each side of the viewport (default
    /// 250): items inside it are built before they scroll into view, so flings
    /// reveal content instead of blanks.
    pub fn cache_extent(mut self, px: f64) -> Self {
        self.cache_extent = px.max(0.0);
        self
    }
    /// Snap the scroll offset to multiples of `extent` (paged lists, carousels).
    /// Each scroll settles on the nearest page. `0` disables snapping.
    pub fn snap(mut self, extent: f64) -> Self {
        self.snap = extent.max(0.0);
        self
    }
}

struct Props {
    count: usize,
    item_extent: f64,
    axis: Axis,
    scrollbar: ScrollbarStyle,
    controller: Option<ScrollController>,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    separator: Option<(f64, Rc<dyn Fn(usize) -> AnyWidget>)>,
    extents: Option<Rc<dyn Fn(usize) -> f64>>,
    auto: bool,
    estimate: f64,
    estimate_fn: Option<Rc<dyn Fn(usize) -> f64>>,
    cache_extent: f64,
    snap: f64,
    reverse: bool,
    padding: Option<pebbles_foundation::EdgeInsets>,
}

impl IntoWidget for ListView {
    fn into_widget(self) -> AnyWidget {
        let viewport = component_props(
            render_list,
            Props {
                count: self.count,
                item_extent: self.item_extent,
                axis: self.axis,
                scrollbar: self.scrollbar,
                controller: self.controller,
                builder: self.builder,
                separator: self.separator,
                extents: self.extents,
                auto: self.auto,
                estimate: self.estimate,
                estimate_fn: self.estimate_fn,
                cache_extent: self.cache_extent,
                snap: self.snap,
                reverse: self.reverse,
                padding: self.padding,
            },
        );
        // C7: a ListView is a List container (its ListTile rows are ListItems).
        crate::widgets::semantics(pebbles_render::SemanticsRole::List, "", viewport).into_widget()
    }
}

fn render_list(p: &Props) -> ListViewport {
    // Use the controller's offset signal if one was supplied, else an internal one.
    let (id, offset) = match &p.controller {
        Some(c) => (c.id, c.offset),
        None => {
            let offset = create_signal(0.0_f64);
            (owner_id().unwrap_or(0), offset)
        }
    };
    // The auto-mode extent cache + its corrective-rebuild bump. Hooks at stable
    // positions for every list (cheap in the non-auto modes).
    let cache_rc = create_signal(Rc::new(RefCell::new(ExtentCache {
        estimate: p.estimate.max(1.0),
        ..Default::default()
    })));
    let bump = create_signal(0_u64);
    bump.get(); // subscribe: a measurement re-renders the list for one corrective pass

    let sep_ext = p.separator.as_ref().map(|(e, _)| *e).unwrap_or(0.0);
    let stride = p.item_extent + sep_ext;
    let auto = p.auto;
    let variable = p.extents.is_some() || auto;
    let ext = p.item_extent.max(1.0);

    // Auto mode: refresh the cache's estimate/size for this render.
    if auto {
        let rc = cache_rc.get();
        let mut cache = rc.borrow_mut();
        cache.estimate = p.estimate.max(1.0);
        if cache.estimates.is_some() != p.estimate_fn.is_some() {
            cache.dirty = true;
        }
        cache.estimates = p.estimate_fn.clone();
        if cache.measured.len() != p.count {
            cache.measured.resize(p.count, None);
            cache.dirty = true;
        }
    }

    // The content extent + item positions: auto uses the live cache, variable
    // uses a running prefix over the caller-supplied extents.
    let prefix: Vec<f64> = match &p.extents {
        Some(ext) => {
            let mut pre = Vec::with_capacity(p.count + 1);
            pre.push(0.0);
            let mut acc = 0.0;
            for i in 0..p.count {
                acc += ext(i).max(1.0);
                pre.push(acc);
            }
            pre
        }
        None => Vec::new(),
    };
    let content_extent = if auto {
        cache_rc.get().borrow_mut().total()
    } else if variable {
        prefix[p.count]
    } else if sep_ext > 0.0 {
        p.count as f64 * p.item_extent + (p.count.saturating_sub(1)) as f64 * sep_ext
    } else {
        p.count as f64 * p.item_extent
    };
    let pad = p.padding.unwrap_or(pebbles_foundation::EdgeInsets::ZERO);
    let (pad_lead, pad_trail) = match p.axis {
        Axis::Vertical => (pad.top, pad.bottom),
        Axis::Horizontal => (pad.left, pad.right),
    };
    let padded_extent = content_extent + pad_lead + pad_trail;

    // Drop this list's registry entries when it unmounts.
    create_cleanup(move || {
        scroll::clear(id);
        scroll_metrics::clear(id);
    });

    // The auto-mode index→offset function (for `scroll_to_index_auto`).
    if auto {
        let cache = cache_rc.get();
        scroll::install_index(id, Rc::new(move |i| cache.borrow_mut().offset_of(i)));
    }

    // Route wheel + scrollbar drag into the offset signal (clamped to the live
    // viewport). Re-installed each render (idempotent) so `content_extent` stays
    // current if `count` changes.
    {
        let ce = padded_extent;
        let snap = p.snap;
        scroll::install(
            id,
            Rc::new(move |to| {
                let vp = scroll_metrics::get(id).map(|m| m.viewport).unwrap_or(0.0);
                let max = (ce - vp).max(0.0);
                let mut next = match to {
                    ScrollTo::By(d) => offset.peek() + d,
                    ScrollTo::ToFraction(f) => f * max,
                }
                .clamp(0.0, max);
                if snap > 0.0 {
                    next = ((next / snap).round() * snap).clamp(0.0, max);
                }
                offset.set(next);
            }),
        );
    }

    // Visible window from the current offset + last-known viewport extent.
    let o = offset.get();
    let viewport = scroll_metrics::get(id).map(|m| m.viewport).unwrap_or(800.0);
    let unit = if sep_ext > 0.0 { stride } else { ext };

    // The visible window: binary search over the prefix when variable, a walk
    // over the extent cache when auto, else the fixed-extent arithmetic.
    let ce = p.cache_extent;
    let (first, last) = if auto {
        let lo = (o - ce).max(0.0);
        let hi = (o + viewport + ce).max(0.0);
        let rc = cache_rc.get();
        let mut cache = rc.borrow_mut();
        let first = cache.index_at(lo).saturating_sub(OVERSCAN as usize);
        let mut last = first;
        let mut acc = cache.offset_of(first);
        while last < p.count && acc < hi {
            acc += cache.extent_of(last);
            last += 1;
        }
        (first, (last + OVERSCAN as usize).min(p.count))
    } else if variable {
        let lo = (o - ce).max(0.0);
        let hi = (o + viewport + ce).max(0.0);
        let first = prefix.partition_point(|&top| top < lo).saturating_sub(OVERSCAN as usize);
        let last = prefix.partition_point(|&top| top <= hi).min(p.count).saturating_add(OVERSCAN as usize).min(p.count);
        (first, last)
    } else {
        let first = ((((o - ce) / unit).floor() as isize) - OVERSCAN).max(0) as usize;
        let last = ((((o + viewport + ce) / unit).ceil() as isize) + OVERSCAN).max(0) as usize;
        (first, last.min(p.count))
    };

    let position = |i: usize| -> (f64, f64) {
        // (top/left in content space, item extent)
        if auto {
            let rc = cache_rc.get();
            let mut cache = rc.borrow_mut();
            (cache.offset_of(i), cache.extent_of(i))
        } else if variable {
            (prefix[i], (prefix[i + 1] - prefix[i]).max(1.0))
        } else {
            let at = if sep_ext > 0.0 { i as f64 * stride } else { i as f64 * ext };
            (at, ext)
        }
    };

    let mut items: Vec<AnyWidget> = Vec::new();
    for i in first..last {
        // Every item is its own repaint boundary: a clean item re-appends its
        // retained fragment each frame instead of re-encoding glyphs/paths.
        let item = crate::widgets::repaint_boundary((p.builder)(i));
        // Auto mode: probe the item so its real extent lands in the cache.
        let item: AnyWidget = if auto {
            MeasureProbe {
                axis: p.axis,
                cache: cache_rc.get(),
                bump,
                offset,
                index: i,
                child: Some(item.into_widget()),
            }
            .into_widget()
        } else {
            item.into_widget()
        };
        let (at, item_ext) = position(i);
        let at = at + pad_lead;
        let at = if p.reverse { padded_extent - at - item_ext } else { at };
        // Auto mode: the measure probe sizes the item naturally — no fixed
        // extent on the scroll axis (the cached extent only decides the top).
        let placed = if auto {
            match p.axis {
                Axis::Vertical => Positioned::new(item)
                    .top(at)
                    .left(pad.left)
                    .right(pad.right),
                Axis::Horizontal => Positioned::new(item)
                    .left(at)
                    .top(pad.top)
                    .bottom(pad.bottom),
            }
        } else {
            match p.axis {
                Axis::Vertical => Positioned::new(item)
                    .top(at)
                    .left(pad.left)
                    .right(pad.right)
                    .height(item_ext),
                Axis::Horizontal => Positioned::new(item)
                    .left(at)
                    .top(pad.top)
                    .bottom(pad.bottom)
                    .width(item_ext),
            }
        };
        items.push(placed.into_widget());
        if let Some((se, sep_builder)) = &p.separator {
            if i + 1 < p.count {
                let sep_widget = (sep_builder)(i);
                let sep_at = if p.reverse { padded_extent - (at - pad_lead) - item_ext - *se + pad_lead } else { at + item_ext };
                let placed_sep = match p.axis {
                    Axis::Vertical => Positioned::new(sep_widget)
                        .top(sep_at)
                        .left(pad.left)
                        .right(pad.right)
                        .height(*se),
                    Axis::Horizontal => Positioned::new(sep_widget)
                        .left(sep_at)
                        .top(pad.top)
                        .bottom(pad.bottom)
                        .width(*se),
                };
                items.push(placed_sep.into_widget());
            }
        }
    }

    let max = (padded_extent - viewport).max(0.0);
    ListViewport {
        axis: p.axis,
        offset: o.clamp(0.0, max),
        content_extent: padded_extent,
        id,
        scrollbar: p.scrollbar,
        child: Some(stack(items).into_widget()),
    }
}

// ---------------------------------------------------------------------------
// GridView — 2D virtualized grid (fixed columns × fixed row height).
// ---------------------------------------------------------------------------

/// A virtualized grid: `columns` cells per row, each `row_extent` tall, cell width
/// = viewport width / columns. Only the visible rows are built. Cells may SPAN
/// columns and rows ([`spans`](GridView::spans)) — the CSS-grid-style
/// `rowspan`/`colspan` (the layout packs around them).
pub struct GridView {
    count: usize,
    columns: usize,
    row_extent: f64,
    scrollbar: ScrollbarStyle,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    spans: Option<Rc<dyn Fn(usize) -> (u32, u32)>>,
    spacing: f64,
    aspect_ratio: Option<f64>,
    max_extent: Option<f64>,
    controller: Option<ScrollController>,
    reverse: bool,
    padding: Option<pebbles_foundation::EdgeInsets>,
}

impl GridView {
    /// Build a grid of `count` cells in `columns` columns, each `row_extent` tall.
    pub fn builder<W: IntoWidget>(
        count: usize,
        columns: usize,
        row_extent: f64,
        builder: impl Fn(usize) -> W + 'static,
    ) -> Self {
        GridView {
            count,
            columns: columns.max(1),
            row_extent,
            scrollbar: ScrollbarStyle::default(),
            builder: Rc::new(move |i| builder(i).into_widget()),
            spans: None,
            spacing: 0.0,
            aspect_ratio: None,
            max_extent: None,
            controller: None,
            reverse: false,
            padding: None,
        }
    }
    /// Customize the scrollbar.
    pub fn scrollbar(mut self, style: ScrollbarStyle) -> Self {
        self.scrollbar = style;
        self
    }
    /// The gap between rows AND between columns (Flutter's
    /// `mainAxisSpacing` + `crossAxisSpacing` in one). Default 0.
    pub fn spacing(mut self, px: f64) -> Self {
        self.spacing = px.max(0.0);
        self
    }
    /// Derive the row height from the cell width: `row = width / ratio`
    /// (Flutter's `childAspectRatio`) — square cells = 1.0. Overrides the
    /// fixed `row_extent`.
    pub fn aspect_ratio(mut self, ratio: f64) -> Self {
        self.aspect_ratio = Some(ratio.max(0.01));
        self
    }
    /// Derive the column count from the available width: as many columns as
    /// fit at `extent` px each (Flutter's `maxCrossAxisExtent`) — the grid
    /// turns responsive. Overrides the fixed `columns`.
    pub fn max_extent(mut self, extent: f64) -> Self {
        self.max_extent = Some(extent.max(1.0));
        self
    }
    /// Drive the grid programmatically with a [`ScrollController`].
    pub fn controller(mut self, controller: ScrollController) -> Self {
        self.controller = Some(controller);
        self
    }
    /// Let each cell declare its (columns, rows) span — the CSS-grid
    /// `colspan`/`rowspan`. Spanning cells occupy that many grid cells; the
    /// packing wraps to the next row when a span doesn't fit. Defaults to
    /// (1, 1) per cell.
    pub fn spans(mut self, spans: impl Fn(usize) -> (u32, u32) + 'static) -> Self {
        self.spans = Some(Rc::new(spans));
        self
    }
    /// Reverse the grid: row 0 sits at the bottom and the grid starts
    /// scrolled there (Flutter's `reverse`).
    pub fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }
    /// Outer padding around the scrollable content.
    pub fn padding(mut self, insets: pebbles_foundation::EdgeInsets) -> Self {
        self.padding = Some(insets);
        self
    }
}

struct GridProps {
    count: usize,
    columns: usize,
    row_extent: f64,
    scrollbar: ScrollbarStyle,
    builder: Rc<dyn Fn(usize) -> AnyWidget>,
    spans: Option<Rc<dyn Fn(usize) -> (u32, u32)>>,
    spacing: f64,
    aspect_ratio: Option<f64>,
    max_extent: Option<f64>,
    controller: Option<ScrollController>,
    reverse: bool,
    padding: Option<pebbles_foundation::EdgeInsets>,
}

impl IntoWidget for GridView {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_grid,
            GridProps {
                count: self.count,
                columns: self.columns,
                row_extent: self.row_extent,
                scrollbar: self.scrollbar,
                builder: self.builder,
                spans: self.spans,
                spacing: self.spacing,
                aspect_ratio: self.aspect_ratio,
                max_extent: self.max_extent,
                controller: self.controller,
                reverse: self.reverse,
                padding: self.padding,
            },
        )
        .into_widget()
    }
}

/// One packed grid item: (row, col, col-span, row-span).
type Placement = (usize, usize, u32, u32);

/// Pack `count` items with their spans into a `cols`-column grid (the
/// CSS-grid packing: fill left-to-right, top-to-bottom, wrapping when a span
/// doesn't fit). Returns the placements and the number of rows used.
pub(crate) fn pack_grid(count: usize, cols: usize, spans: &dyn Fn(usize) -> (u32, u32)) -> (Vec<Placement>, usize) {
    let cols = cols.max(1);
    let mut occupied: Vec<u8> = Vec::new();
    let mut placements: Vec<Placement> = Vec::with_capacity(count);
    let mut rows_used = 0usize;
    for i in 0..count {
        let (raw_cs, raw_rs) = spans(i);
        let cs = raw_cs.clamp(1, cols as u32);
        let rs = raw_rs.max(1);
        // Find the first position (row-major) where the span fits.
        let mut placed = None;
        'scan: for row in 0usize.. {
            let needed = (row + rs as usize) * cols;
            if occupied.len() < needed {
                occupied.resize(needed, 0);
            }
            for c in 0..cols {
                if c as u32 + cs > cols as u32 {
                    break;
                }
                let mut fits = true;
                for rr in 0..rs as usize {
                    for cc in 0..cs as usize {
                        if occupied[(row + rr) * cols + c + cc] != 0 {
                            fits = false;
                            break;
                        }
                    }
                    if !fits {
                        break;
                    }
                }
                if fits {
                    for rr in 0..rs as usize {
                        for cc in 0..cs as usize {
                            occupied[(row + rr) * cols + c + cc] = 1;
                        }
                    }
                    placed = Some((row, c, cs, rs));
                    rows_used = rows_used.max(row + rs as usize);
                    break 'scan;
                }
            }
        }
        placements.push(placed.expect("a free cell always exists"));
    }
    (placements, rows_used)
}

fn render_grid(p: &GridProps) -> ListViewport {
    let (id, offset) = match &p.controller {
        Some(c) => (c.id, c.offset),
        None => {
            let offset = create_signal(0.0_f64);
            (owner_id().unwrap_or(0), offset)
        }
    };
    let gap = p.spacing.max(0.0);

    create_cleanup(move || {
        scroll::clear(id);
        scroll_metrics::clear(id);
    });

    // Column count and row height may derive from the live viewport
    // (max_extent → responsive columns; aspect_ratio → width-based rows).
    let metrics_pre = scroll_metrics::get(id);
    let cross_pre = metrics_pre.map(|x| x.cross).filter(|c| *c > 0.0).unwrap_or(800.0);
    let cols = match p.max_extent {
        Some(me) => ((cross_pre / me).floor() as usize).max(1),
        None => p.columns.max(1),
    };
    let cell_w = cross_pre / cols as f64;
    let row_h = match p.aspect_ratio {
        Some(r) => (cell_w / r).max(1.0),
        None => p.row_extent.max(1.0),
    };

    let (placements, rows_used) = match &p.spans {
        Some(spans) => pack_grid(p.count, cols, spans.as_ref()),
        None => {
            let rows = p.count.div_ceil(cols);
            let placements = (0..p.count)
                .map(|i| (i / cols, i % cols, 1u32, 1u32))
                .collect();
            (placements, rows)
        }
    };
    let stride = row_h + gap;
    let cell_stride = cell_w + gap;
    let content_extent = if rows_used == 0 {
        0.0
    } else {
        rows_used as f64 * stride - gap
    };
    let pad = p.padding.unwrap_or(pebbles_foundation::EdgeInsets::ZERO);
    let padded_extent = content_extent + pad.top + pad.bottom;

    {
        let ce = padded_extent;
        scroll::install(
            id,
            Rc::new(move |to| {
                let vp = scroll_metrics::get(id).map(|m| m.viewport).unwrap_or(0.0);
                let max = (ce - vp).max(0.0);
                let next = match to {
                    ScrollTo::By(d) => offset.peek() + d,
                    ScrollTo::ToFraction(f) => f * max,
                };
                offset.set(next.clamp(0.0, max));
            }),
        );
    }

    let o = offset.get();
    let m = scroll_metrics::get(id);
    let viewport = m.map(|x| x.viewport).unwrap_or(800.0);
    let cross = m.map(|x| x.cross).filter(|c| *c > 0.0).unwrap_or(800.0);
    let cell_w = cross / cols as f64;

    let first_row = (((o / stride).floor() as isize) - OVERSCAN).max(0) as usize;
    let last_row = ((((o + viewport) / stride).ceil() as isize) + OVERSCAN).max(0) as usize;
    let last_row = last_row.min(rows_used);

    let mut items: Vec<AnyWidget> = Vec::new();
    for (i, &(row, col, cs, rs)) in placements.iter().enumerate() {
        let row_end = row + rs as usize;
        if row_end <= first_row || row > last_row {
            continue;
        }
        {
            let item = (p.builder)(i);
            let height = rs as f64 * row_h + (rs as f64 - 1.0) * gap;
            let mut top = row as f64 * stride + pad.top;
            if p.reverse {
                top = padded_extent - top - height;
            }
            let placed = Positioned::new(item)
                .top(top)
                .left(col as f64 * cell_stride + pad.left)
                .width(cs as f64 * cell_w + (cs as f64 - 1.0) * gap)
                .height(height);
            items.push(placed.into_widget());
        }
    }
    let max = (padded_extent - viewport).max(0.0);
    ListViewport {
        axis: Axis::Vertical,
        offset: o.clamp(0.0, max),
        content_extent: padded_extent,
        id,
        scrollbar: p.scrollbar,
        child: Some(stack(items).into_widget()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_span_packing_follows_css_grid_rules() {
        // 4 columns; item 0 spans 2×2, item 4 spans 2×1, the rest 1×1.
        let spans = |i: usize| match i {
            0 => (2, 2),
            4 => (2, 1),
            _ => (1, 1),
        };
        let (placements, rows) = pack_grid(5, 4, &spans);
        assert_eq!(rows, 3, "3 rows packed");
        assert_eq!(
            placements,
            vec![(0, 0, 2, 2), (0, 2, 1, 1), (0, 3, 1, 1), (1, 2, 1, 1), (2, 0, 2, 1)],
            "spans pack around each other, wrapping when they don't fit"
        );

        // A span wider than the grid clamps to the grid.
        let wide = |_: usize| (9, 3);
        let (placements, rows) = pack_grid(2, 4, &wide);
        assert_eq!(placements, vec![(0, 0, 4, 3), (3, 0, 4, 3)]);
        assert_eq!(rows, 6, "oversized spans clamp to the column count and stack");
    }
}
