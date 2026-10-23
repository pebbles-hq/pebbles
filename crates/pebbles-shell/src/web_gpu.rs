//! Web-only GPU helpers: URL test hooks, WebGL context-loss listeners, the GPU error message.
//! The query parser is target-independent so its tests run natively.

/// Value of `name` in the page URL's query string (`?a=1&b=2`), if present.
#[cfg(target_family = "wasm")]
pub(crate) fn query_param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    parse_query(&search, name)
}

/// 08 P04: forward the canvas's `webglcontextlost` / `webglcontextrestored` to the event loop.
/// Installed once per canvas (on the first GPU-ready); the closures live as long as the page.
#[cfg(target_family = "wasm")]
pub(crate) fn install_context_loss_listeners(
    window: &winit::window::Window,
    proxy: winit::event_loop::EventLoopProxy<crate::app::runner::PebblesUserEvent>,
) {
    use crate::app::runner::PebblesUserEvent;
    use wasm_bindgen::JsCast as _;
    use wasm_bindgen::closure::Closure;
    use winit::platform::web::WindowExtWebSys as _;
    let Some(canvas) = window.canvas() else { return };
    let listen = |name: &str, event: fn() -> PebblesUserEvent| {
        let proxy = proxy.clone();
        let callback = Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            // Without preventDefault the browser never offers to restore the context.
            e.prevent_default();
            let _ = proxy.send_event(event());
        });
        if let Err(e) = canvas.add_event_listener_with_callback(name, callback.as_ref().unchecked_ref()) {
            pebbles_foundation::log::logf(
                pebbles_foundation::log::Level::Debug,
                pebbles_foundation::log::Cat::Gpu,
                format_args!("could not listen for {name}: {e:?}"),
            );
        }
        callback.forget();
    };
    listen("webglcontextlost", || PebblesUserEvent::GpuContextLost);
    listen("webglcontextrestored", || PebblesUserEvent::GpuContextRestored);
}

/// Pure part, unit-testable on any target.
#[cfg_attr(not(target_family = "wasm"), allow(dead_code))] // native: tests only
pub(crate) fn parse_query(search: &str, name: &str) -> Option<String> {
    search.trim_start_matches('?').split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == name).then(|| v.to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::parse_query;

    #[test]
    fn parse_query_finds_value() {
        assert_eq!(parse_query("?a=1&pebbles-force-webgl=1", "pebbles-force-webgl").as_deref(), Some("1"));
    }

    #[test]
    fn parse_query_missing() {
        assert_eq!(parse_query("?a=1", "pebbles-force-webgl"), None);
        assert_eq!(parse_query("", "pebbles-force-webgl"), None);
    }

    #[test]
    fn parse_query_bare_key() {
        assert_eq!(parse_query("?pebbles-force-webgl", "pebbles-force-webgl").as_deref(), Some(""));
    }
}
