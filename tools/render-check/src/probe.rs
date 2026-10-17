//! Analytic probes (§6.1 reference #2; V16).

use crate::case::Case;
use crate::metric::Rgba8;

/// Run every probe of `case` on `img`, appending `(name, None | Some(message))` to `out`.
/// Returns the number of failing probes. A panicking probe counts as failing.
pub fn run_probes(case: &Case, img: &Rgba8, out: &mut Vec<(&'static str, Option<String>)>) -> usize {
    let mut fails = 0;
    for (name, p) in &case.probes {
        let res = std::panic::catch_unwind(|| p(img, case.scale))
            .unwrap_or_else(|_| Err("probe panicked".to_string()));
        if res.is_err() {
            fails += 1;
        }
        out.push((name, res.err()));
    }
    fails
}
