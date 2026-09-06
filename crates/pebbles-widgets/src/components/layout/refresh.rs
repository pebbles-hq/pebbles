//! [`RefreshIndicator`] — pull-to-refresh (A5): drag the top of the content past
//! the threshold and release to fire `on_refresh`; a spinner row holds until the
//! app finishes the refresh. Built on A4's drag-scroll + rubber-band overscroll.

use std::rc::Rc;

use pebbles_foundation::MainAxisSize;
use pebbles_render::RefreshState;

use crate::style::{style, styled};
use crate::theme::theme;
use crate::widgets::{Positioned, SingleChildScrollView, center, gap_w, row, spinner, stack, text};
use pebbles_core::children;
use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Signal, component_props, create_signal};
use pebbles_render::ScrollPhysics;

/// A call-once handle handed to `on_refresh`: call [`finish`](RefreshDone::finish)
/// when the async work completes and the indicator row collapses.
#[derive(Clone)]
pub struct RefreshDone {
    signal: Signal<bool>,
}

impl RefreshDone {
    /// Mark the refresh as complete — the spinner row collapses.
    pub fn finish(&self) {
        self.signal.set(false);
    }
}

/// A scrollable child with a pull-to-refresh gesture on its top edge.
#[derive(Clone)]
pub struct RefreshIndicator {
    child: Option<AnyWidget>,
    on_refresh: Option<Rc<dyn Fn(RefreshDone)>>,
    threshold: f64,
}

/// Wrap `child` (usually a scrollable column) in a pull-to-refresh indicator.
pub fn refresh_indicator(child: impl IntoWidget) -> RefreshIndicator {
    RefreshIndicator { child: Some(child.into_widget()), on_refresh: None, threshold: 64.0 }
}

impl RefreshIndicator {
    /// Fired once per armed pull-release. The app keeps the returned
    /// [`RefreshDone`] and calls `.finish()` when the refresh completes
    /// (async work rides `spawn`).
    pub fn on_refresh(mut self, cb: impl Fn(RefreshDone) + 'static) -> Self {
        self.on_refresh = Some(Rc::new(cb));
        self
    }
    /// Pull distance (in banded px) that arms the indicator. Default 64.
    pub fn threshold(mut self, px: f64) -> Self {
        self.threshold = px.max(1.0);
        self
    }
}

#[derive(Clone)]
struct Props {
    child: AnyWidget,
    on_refresh: Option<Rc<dyn Fn(RefreshDone)>>,
    threshold: f64,
}

impl IntoWidget for RefreshIndicator {
    fn into_widget(self) -> AnyWidget {
        component_props(
            render_refresh,
            Props {
                child: self.child.expect("refresh_indicator requires a child"),
                on_refresh: self.on_refresh,
                threshold: self.threshold,
            },
        )
        .into_widget()
    }
}

fn render_refresh(p: &Props) -> pebbles_core::Element {
    let refreshing = create_signal(false);
    let pulling = create_signal(false);

    // The scroll view's drag driver calls these as the pull crosses the
    // threshold and releases.
    let mut refresh = RefreshState::new(p.threshold);
    {
        refresh.on_arm = Some(Rc::new(move || pulling.set(true)));
    }
    {
        refresh.on_release = Some(Rc::new(move || pulling.set(false)));
    }
    {
        let on_refresh = p.on_refresh.clone();
        refresh.on_arm_release = Some(Rc::new(move || {
            pulling.set(false);
            // Fire once per armed release; a second pull while refreshing is
            // ignored until the app finishes (v1 contract).
            if refreshing.peek() {
                return;
            }
            if let Some(cb) = &on_refresh {
                refreshing.set(true);
                cb(RefreshDone { signal: refreshing });
            }
        }));
    }

    let scroll = SingleChildScrollView::vertical(p.child.clone())
        .drag_scroll(true)
        .physics(ScrollPhysics { overscroll: true, ..Default::default() })
        .refresh(refresh);

    let show = pulling.get() || refreshing.get();
    let c = theme().colors;
    let indicator = if show {
        let bar = row(children![
            spinner(16.0),
            gap_w(8.0),
            text(if refreshing.get() { "Refreshing…" } else { "Release to refresh" })
                .size(12.0)
                .color(c.muted_foreground),
        ])
        .main_axis_size(MainAxisSize::Min);
        let pill = styled(
            center(bar),
            style()
                .background(c.card)
                .border(pebbles_render::Border::new(c.border, 1.0))
                .radius_all(999.0)
                .padding_xy(14.0, 6.0)
                .shadow(pebbles_render::BoxShadow::new(
                    pebbles_foundation::Color::from_rgba8(0, 0, 0, 30),
                    pebbles_foundation::Offset::new(0.0, 2.0),
                    6.0,
                    0.0,
                )),
        );
        Positioned::new(pill).top(8.0).left(0.0).right(0.0).into_widget()
    } else {
        gap_w(0.0).into_widget()
    };
    stack(children![scroll.into_widget(), indicator]).into_widget()
}
