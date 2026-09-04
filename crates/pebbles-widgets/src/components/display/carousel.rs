//! [`Carousel`] — a paged, swipeable slideshow (A6): horizontal snap-paged
//! scrolling, dot indicators, prev/next arrows, optional autoplay (paused while
//! hovered) and a `CarouselController` for programmatic paging. Infinite wrap is
//! v2 (p2 §J).

use std::rc::Rc;

use pebbles_foundation::{Axis, MainAxisSize};
use pebbles_render::ScrollbarStyle;

use crate::components::input::icon_button;
use crate::style::{style, styled};
use crate::theme::theme;
use crate::widgets::{
    GestureDetector, ListView, Positioned, ScrollController, SizedBox, center, column, extent_probe, gap_w,
    row, stack, text,
};
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, children, component_props, create_effect, create_loop_while, create_signal};

/// Programmatic control of a carousel's current page. Create one with
/// [`use_carousel_controller`] and pass it to [`Carousel::controller`].
#[derive(Clone, Copy)]
pub struct CarouselController {
    offset: Signal<f64>,
    width: Signal<f64>,
}

/// Create a [`CarouselController`] (call at the top level of a component, like a
/// signal — it persists across renders).
pub fn use_carousel_controller() -> CarouselController {
    let offset = create_signal(0.0_f64);
    let width = create_signal(0.0_f64);
    CarouselController { offset, width }
}

impl CarouselController {
    fn page_width(&self) -> f64 {
        self.width.peek().max(1.0)
    }
    /// The raw scroll offset (px) — for tooling/tests.
    #[doc(hidden)]
    pub fn offset(&self) -> f64 {
        self.offset.peek()
    }
    /// The measured page width (px) — for tooling/tests.
    #[doc(hidden)]
    pub fn width(&self) -> f64 {
        self.width.peek()
    }
    /// The current page index (`round(offset / page_width)`).
    pub fn page(&self) -> usize {
        (self.offset.peek() / self.page_width()).round().max(0.0) as usize
    }
    /// Jump to page `i` (instant).
    pub fn jump(&self, i: usize) {
        self.offset.set(i as f64 * self.page_width());
    }
    /// Advance one page.
    pub fn next(&self) {
        self.jump(self.page() + 1);
    }
    /// Go back one page (clamps at page 0).
    pub fn prev(&self) {
        self.jump(self.page().saturating_sub(1));
    }
}

/// A paged slideshow of `pages` — Flutter's `PageView`-ish, Rust-shaped.
#[derive(Clone)]
pub struct Carousel {
    pages: Vec<AnyWidget>,
    height: f64,
    indicator: bool,
    arrows: bool,
    autoplay: Option<f64>,
    on_page_changed: Option<Rc<dyn Fn(usize)>>,
    controller: Option<CarouselController>,
}

/// Create a [`Carousel`] from its slides (in display order).
pub fn carousel(pages: impl pebbles_core::IntoChildren) -> Carousel {
    Carousel {
        pages: pages.into_children(),
        height: 320.0,
        indicator: true,
        arrows: true,
        autoplay: None,
        on_page_changed: None,
        controller: None,
    }
}

impl Carousel {
    /// The carousel's height (pages fill it).
    pub fn height(mut self, height: f64) -> Self {
        self.height = height.max(1.0);
        self
    }
    /// Show the dot indicators (default on).
    pub fn indicator(mut self, show: bool) -> Self {
        self.indicator = show;
        self
    }
    /// Show the prev/next arrows (default on; the prev arrow is hidden on the
    /// first page and the next on the last).
    pub fn arrows(mut self, show: bool) -> Self {
        self.arrows = show;
        self
    }
    /// Advance a page every `secs` seconds; pauses while hovered.
    pub fn autoplay(mut self, secs: f64) -> Self {
        self.autoplay = Some(secs.max(0.5));
        self
    }
    /// Fired whenever the settled page changes.
    pub fn on_page_changed(mut self, cb: impl Fn(usize) + 'static) -> Self {
        self.on_page_changed = Some(Rc::new(cb));
        self
    }
    /// Drive the carousel programmatically (prev/next/jump).
    pub fn controller(mut self, controller: CarouselController) -> Self {
        self.controller = Some(controller);
        self
    }
}

#[derive(Clone)]
struct Props {
    pages: Vec<AnyWidget>,
    height: f64,
    indicator: bool,
    arrows: bool,
    autoplay: Option<f64>,
    on_page_changed: Option<Rc<dyn Fn(usize)>>,
    controller: Option<CarouselController>,
}

impl IntoWidget for Carousel {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_carousel,
            Props {
                pages: self.pages,
                height: self.height,
                indicator: self.indicator,
                arrows: self.arrows,
                autoplay: self.autoplay,
                on_page_changed: self.on_page_changed,
                controller: self.controller,
            },
        )
        .into_widget()
    }
}

fn render_carousel(p: &Props) -> pebbles_core::Element {
    let hovered = create_signal(false);
    let count = p.pages.len();
    // The live page width — measured from the laid-out viewport (reports on
    // change, so the first measured frame re-renders with the true width).
    let width = create_signal(0.0_f64);
    let offset = match &p.controller {
        Some(c) => c.offset,
        None => {
            let offset = create_signal(0.0_f64);
            offset
        }
    };
    let id = match &p.controller {
        Some(_) => offset.raw_id(),
        None => pebbles_core::owner_id().unwrap_or(offset.raw_id()),
    };
    // Keep the controller's width mirror in sync with the measurement.
    if let Some(c) = &p.controller {
        let w = c.width;
        let width = width;
        create_effect(move || w.set(width.get()));
    }

    let page_w = width.get().max(1.0);
    let page_idx = ((offset.get() / page_w).round().max(0.0) as usize).min(count.saturating_sub(1));

    // on_page_changed: fire when the settled page changes (not on scroll noise).
    let last_page = create_signal(usize::MAX);
    if last_page.peek() == usize::MAX {
        last_page.set(page_idx);
    }
    if last_page.peek() != page_idx {
        last_page.set(page_idx);
        if let Some(cb) = &p.on_page_changed {
            cb(page_idx);
        }
    }

    // Autoplay: advance when the loop wraps; paused while hovered.
    if let Some(secs) = p.autoplay {
        let looped = create_loop_while(!hovered.get(), secs);
        let prev_phase = create_signal(looped.get());
        let offset = offset;
        let width = width;
        let count = count;
        create_effect(move || {
            let v = looped.get();
            if v < prev_phase.peek() {
                let w = width.peek().max(1.0);
                let next = ((offset.peek() / w).round() as usize + 1) % count;
                offset.set(next as f64 * w);
            }
            prev_phase.set(v);
        });
    }

    let c = theme().colors;

    // The paged list: one page per slide, snapped to the measured page width.
    let pages = Rc::new(p.pages.clone());
    let list = ListView::builder(count, page_w, move |i| pages[i].clone())
        .horizontal()
        .snap(page_w)
        .scrollbar(ScrollbarStyle { policy: pebbles_render::ScrollbarPolicy::Hidden, ..Default::default() })
        .controller(ScrollController::from_parts(id, offset));
    let list: AnyWidget = list.into_widget();

    // Measure the viewport width through a probe; the report keeps `width` live.
    let probed = extent_probe(
        Axis::Horizontal,
        {
            let width = width;
            move |w: f64| width.set(w)
        },
        list,
    );

    // Dots + arrows overlaid on the pages.
    let mut overlay: Vec<AnyWidget> = Vec::new();
    if p.indicator && count > 1 {
        let mut dots: Vec<AnyWidget> = Vec::new();
        for i in 0..count {
            let active = i == page_idx;
            let dot = styled(
                center(text("").size(0.0)),
                style().size(if active { 18.0 } else { 6.0 }, 6.0).radius_all(999.0).background(if active {
                    c.primary
                } else {
                    c.muted_foreground
                }),
            );
            dots.push(dot.into_widget());
            if i + 1 < count {
                dots.push(gap_w(6.0).into_widget());
            }
        }
        overlay.push(
            Positioned::new(row(dots).main_axis_size(MainAxisSize::Min))
                .bottom(10.0)
                .left(0.0)
                .right(0.0)
                .into_widget(),
        );
    }
    if p.arrows && count > 1 {
        let at_first = page_idx == 0;
        let at_last = page_idx + 1 >= count;
        if !at_first {
            let prev = icon_button(pebbles_render::IconKind::ChevronLeft).size(20.0).on_pressed({
                let offset = offset;
                let width = width;
                move || {
                    let w = width.peek().max(1.0);
                    let page = ((offset.peek() / w).round().max(0.0) as usize).saturating_sub(1);
                    offset.set(page as f64 * w);
                }
            });
            overlay.push(Positioned::new(prev).left(8.0).top(0.0).bottom(0.0).into_widget());
        }
        if !at_last {
            let next = icon_button(pebbles_render::IconKind::ChevronRight).size(20.0).on_pressed({
                let offset = offset;
                let width = width;
                move || {
                    let w = width.peek().max(1.0);
                    let page = (offset.peek() / w).round().max(0.0) as usize + 1;
                    offset.set(page as f64 * w);
                }
            });
            overlay.push(Positioned::new(next).right(8.0).top(0.0).bottom(0.0).into_widget());
        }
    }

    let body: AnyWidget = column(children![SizedBox::new(
        None,
        Some(p.height),
        Some(stack(std::iter::once(probed.into_widget()).chain(overlay).collect::<Vec<_>>()).into_widget()),
    )])
    .cross_axis_alignment(pebbles_foundation::CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min)
    .into_widget();
    let body = GestureDetector::new(body)
        .on_hover_enter(move || hovered.set(true))
        .on_hover_exit(move || hovered.set(false));
    body.into_widget()
}
