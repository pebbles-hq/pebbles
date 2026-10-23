//! Web-only GPU helpers: URL test hooks, WebGL context-loss listeners, the GPU error message.
//! The query parser is target-independent so its tests run natively.

/// Value of `name` in the page URL's query string (`?a=1&b=2`), if present.
#[cfg(target_family = "wasm")]
pub(crate) fn query_param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    parse_query(&search, name)
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
