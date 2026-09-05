//! [`stream_builder`] — rebuild when a [`Channel`] emits (Flutter's `StreamBuilder`).
//!
//! A thin reactive builder over Pebbles' existing [`Channel`]: it reads the channel's
//! latest value (subscribing the component), and calls your `builder` with it — `None`
//! before the first message, `Some(v)` after each `send`. Drop it in anywhere a widget
//! goes; no component boilerplate.
//!
//! ```ignore
//! let ticks = channel::<u32>();
//! stream_builder(ticks.clone(), |v| text(format!("latest: {:?}", v)))
//! ```

use std::rc::Rc;

use pebbles_core::widget::{AnyWidget, IntoWidget};
use pebbles_core::{Channel, Element, component_props};

/// A widget that rebuilds on each [`Channel`] message. Built by [`stream_builder`].
#[derive(Clone)]
pub struct StreamBuilder<T: Clone + 'static> {
    channel: Channel<T>,
    builder: Rc<dyn Fn(Option<T>) -> AnyWidget>,
}

/// See [`StreamBuilder`]. `builder(latest)` is `None` before the first `send`.
pub fn stream_builder<T, W>(
    channel: Channel<T>,
    builder: impl Fn(Option<T>) -> W + 'static,
) -> StreamBuilder<T>
where
    T: Clone + 'static,
    W: IntoWidget,
{
    StreamBuilder { channel, builder: Rc::new(move |v| builder(v).into_widget()) }
}

impl<T: Clone + 'static> IntoWidget for StreamBuilder<T> {
    fn into_widget(self) -> AnyWidget {
        component_props(render_stream_builder::<T>, self).into_widget()
    }
}

fn render_stream_builder<T: Clone + 'static>(b: &StreamBuilder<T>) -> Element {
    // `latest()` subscribes this component, so it re-renders on every send.
    (b.builder)(b.channel.latest())
}
