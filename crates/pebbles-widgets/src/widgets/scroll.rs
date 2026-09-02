//! Scrolling: the [`SingleChildScrollView`] widget, the [`list_view`] helper, and a
//! [`ScrollExt`] so any widget can be made scrollable with `.scrollable()`.
//!
//! Backed by [`pebbles_render::RenderScroll`], which paints a customizable
//! scrollbar. The shell routes wheel events to
//! [`Ui::dispatch_scroll`](pebbles_core::Ui::dispatch_scroll) and scrollbar drags to
//! [`Ui::begin_scrollbar_drag`](pebbles_core::Ui::begin_scrollbar_drag).

use pebbles_foundation::{Axis, Color, MainAxisSize};
use pebbles_render::{
    RefreshState, RenderObject, RenderScroll, ScrollPhysics, ScrollbarPolicy, ScrollbarStyle,
};

use crate::widgets::column;
use pebbles_core::widget::{AnyWidget, RenderWidget};

/// A viewport that scrolls a single (usually tall or wide) child along one axis,
/// with a customizable scrollbar.
#[derive(Clone)]
pub struct SingleChildScrollView {
    axis: Axis,
    scrollbar: ScrollbarStyle,
    snap: f64,
    drag_scroll: bool,
    physics: ScrollPhysics,
    refresh: Option<RefreshState>,
    child: Option<AnyWidget>,
}

impl SingleChildScrollView {
    /// A vertically-scrolling viewport.
    pub fn vertical(child: impl pebbles_core::IntoWidget) -> Self {
        SingleChildScrollView {
            axis: Axis::Vertical,
            scrollbar: ScrollbarStyle::default(),
            snap: 0.0,
            drag_scroll: false,
            physics: ScrollPhysics::default(),
            refresh: None,
            child: Some(child.into_widget()),
        }
    }
    /// A horizontally-scrolling viewport.
    pub fn horizontal(child: impl pebbles_core::IntoWidget) -> Self {
        SingleChildScrollView {
            axis: Axis::Horizontal,
            scrollbar: ScrollbarStyle::default(),
            snap: 0.0,
            drag_scroll: false,
            physics: ScrollPhysics::default(),
            refresh: None,
            child: Some(child.into_widget()),
        }
    }

    /// Snap the scroll offset to multiples of `extent` (e.g. a carousel/paging
    /// scroll). `0` disables snapping (the default).
    pub fn snap(mut self, extent: f64) -> Self {
        self.snap = extent;
        self
    }

    /// Opt into pan-to-scroll: dragging anywhere over the content scrolls it 1:1
    /// (touch, pen, or trackpad-drag). A draggable child under the pointer still
    /// wins — the viewport only claims drags nothing else wants.
    pub fn drag_scroll(mut self, enabled: bool) -> Self {
        self.drag_scroll = enabled;
        self
    }

    /// Replace the scroll physics: spring stiffness, fling friction and whether
    /// drags may rubber-band past the edges.
    pub fn physics(mut self, physics: ScrollPhysics) -> Self {
        self.physics = physics;
        self
    }

    /// Install a pull-to-refresh trigger: while a content drag pulls past the top
    /// (with overscroll on), the arm/release callbacks fire. The
    /// `refresh_indicator` component builds this for you.
    pub fn refresh(mut self, refresh: RefreshState) -> Self {
        self.refresh = Some(refresh);
        self
    }

    /// Replace the whole scrollbar style.
    pub fn scrollbar(mut self, style: ScrollbarStyle) -> Self {
        self.scrollbar = style;
        self
    }
    /// Painted scrollbar thickness.
    pub fn scrollbar_thickness(mut self, thickness: f64) -> Self {
        self.scrollbar.thickness = thickness;
        self
    }
    /// Scrollbar thumb + track colors.
    pub fn scrollbar_colors(mut self, thumb: Color, track: Color) -> Self {
        self.scrollbar.thumb_color = thumb;
        self.scrollbar.track_color = track;
        self
    }
    /// Always show the track (not only on overflow).
    pub fn always_scrollbar(mut self) -> Self {
        self.scrollbar.policy = ScrollbarPolicy::Always;
        self
    }
    /// Scroll, but never paint a scrollbar.
    pub fn hide_scrollbar(mut self) -> Self {
        self.scrollbar.policy = ScrollbarPolicy::Hidden;
        self
    }

    fn make(&self) -> RenderScroll {
        let mut r = RenderScroll::new(self.axis);
        r.scrollbar = self.scrollbar;
        r.snap = self.snap;
        r.drag_scroll = self.drag_scroll;
        r.physics = self.physics;
        r.refresh = self.refresh.clone();
        r
    }
}

pebbles_core::render_widget!(SingleChildScrollView);

impl RenderWidget for SingleChildScrollView {
    fn create_render_object(&self) -> Box<dyn RenderObject> {
        Box::new(self.make())
    }
    fn update_render_object(&self, object: &mut dyn RenderObject) {
        if let Some(s) = object.downcast_mut::<RenderScroll>() {
            s.axis = self.axis;
            s.scrollbar = self.scrollbar;
            s.snap = self.snap;
            s.drag_scroll = self.drag_scroll;
            s.physics = self.physics;
            s.refresh = self.refresh.clone();
        }
    }
    fn take_children(&mut self) -> Vec<AnyWidget> {
        self.child.take().into_iter().collect()
    }
}

/// A vertically-scrolling list: a `Column` of `children` inside a scroll view.
pub fn list_view(children: Vec<AnyWidget>) -> SingleChildScrollView {
    SingleChildScrollView::vertical(column(children).main_axis_size(MainAxisSize::Min))
}

/// Make any widget scrollable — the "scrollable property" for e.g. a `Container`
/// or `Column`: `column(children![...]).scrollable()`.
pub trait ScrollExt: pebbles_core::IntoWidget + Sized {
    /// Wrap in a vertically-scrolling viewport.
    fn scrollable(self) -> SingleChildScrollView {
        SingleChildScrollView::vertical(self)
    }
    /// Wrap in a horizontally-scrolling viewport.
    fn scrollable_horizontal(self) -> SingleChildScrollView {
        SingleChildScrollView::horizontal(self)
    }
}

impl<T: pebbles_core::IntoWidget + Sized> ScrollExt for T {}
