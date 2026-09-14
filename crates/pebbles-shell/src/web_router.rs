//! (web) The browser-history bridge for [`pebbles_core::router`].
//!
//! Installs a [`UrlSync`](pebbles_core::router::UrlSync) that mirrors the router's
//! `navigate` / `replace` / `back` / `forward` to `window.history` (deep-linkable
//! URLs), seeds the initial route from the current URL, and feeds `popstate`
//! (Back/Forward, or a manual URL edit) back into the router — waking the winit
//! loop via [`PebblesUserEvent::RouteChanged`] so route-reading components rebuild.
//! The history `index` rides in `history.state` so it stays authoritative. This
//! module is compiled only for wasm; desktop/mobile keep the router in-memory.

use std::rc::Rc;

use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use wasm_bindgen::closure::Closure;
use winit::event_loop::EventLoopProxy;

use crate::app::runner::PebblesUserEvent;

/// The current `pathname + search` (e.g. `"/components/button?tab=api"`).
fn current_url(window: &web_sys::Window) -> String {
    let loc = window.location();
    let path = loc.pathname().unwrap_or_else(|_| "/".into());
    let search = loc.search().unwrap_or_default();
    format!("{path}{search}")
}

/// `window.history` as a [`UrlSync`] target.
struct WebHistory {
    history: web_sys::History,
}

impl pebbles_core::router::UrlSync for WebHistory {
    fn push(&self, url: &str, index: usize) {
        let state = JsValue::from_f64(index as f64);
        let _ = self.history.push_state_with_url(&state, "", Some(url));
    }
    fn replace(&self, url: &str, index: usize) {
        let state = JsValue::from_f64(index as f64);
        let _ = self.history.replace_state_with_url(&state, "", Some(url));
    }
    fn go(&self, delta: i32) {
        let _ = self.history.go_with_delta(delta);
    }
}

/// Wire the router to the browser. Called once when the web loop goes live.
pub(crate) fn enable(proxy: EventLoopProxy<PebblesUserEvent>) {
    let Some(window) = web_sys::window() else { return };
    let Ok(history) = window.history() else { return };

    // Seed the router from the current URL + install the history bridge (this also
    // stamps `history.state = 0` on the current entry).
    let initial = current_url(&window);
    let adapter: Rc<dyn pebbles_core::router::UrlSync> = Rc::new(WebHistory { history });
    pebbles_core::router::install_url_sync(adapter, &initial);

    // Back/Forward (or a manual URL edit) → restore the router, then wake the loop.
    let cb = Closure::<dyn FnMut(web_sys::PopStateEvent)>::new(move |e: web_sys::PopStateEvent| {
        let index = e.state().as_f64().map(|f| f as usize).unwrap_or(0);
        if let Some(w) = web_sys::window() {
            pebbles_core::router::restore(index, &current_url(&w));
        }
        let _ = proxy.send_event(PebblesUserEvent::RouteChanged);
    });
    let _ = window.add_event_listener_with_callback("popstate", cb.as_ref().unchecked_ref());
    cb.forget(); // lives for the app's lifetime
}
