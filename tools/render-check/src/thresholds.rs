//! Per-category limits (§8.1, initial values; V19 calibrates them).

use crate::case::Category;
use crate::metric::Metrics;

pub use crate::args::Tolerance;

/// The metric limits of one category.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limits {
    /// `T`: a pixel is "over" when `delta > T`.
    pub over_threshold: u8,
    /// Max percentage of pixels over `T`.
    pub max_pct_over: f64,
    /// Max delta off the edge mask.
    pub max_interior: u8,
    /// Max mean absolute error.
    pub max_mae: f64,
}

const fn l(over_threshold: u8, max_pct_over: f64, max_interior: u8, max_mae: f64) -> Limits {
    Limits { over_threshold, max_pct_over, max_interior, max_mae }
}

/// The §8.1 table. `None` for `Analytic` (probes only). `Smoke` is structural (§6.10) and
/// has no pixel limits of its own; it gets the all-zero row only so every non-Analytic
/// category resolves. Canonical = Linux + lavapipe. Until §8.2 records a calibrated relaxed
/// set (V19), `Relaxed` equals `Canonical` (it may only ever be looser, never stricter).
pub fn limits(cat: Category, tol: Tolerance) -> Option<Limits> {
    let canonical = match cat {
        Category::Exact => l(0, 0.0, 0, 0.0),
        Category::Geometry => l(2, 0.0, 1, 0.10),
        Category::Text => l(3, 0.50, 1, 0.30),
        Category::Gradient => l(2, 0.0, 2, 0.30),
        Category::Image => l(2, 0.0, 2, 0.30),
        Category::Shadow => l(3, 1.00, 3, 0.50),
        Category::Layer => l(2, 0.0, 2, 0.20),
        Category::Mesh => l(64, 2.00, 1, 1.00),
        Category::Screen | Category::Stress => l(0, 0.0, 0, 0.0),
        Category::SelfConsistency => l(0, 0.0, 0, 0.0),
        Category::CrossBackend => l(1, 0.10, 1, 0.05),
        Category::Smoke => l(0, 0.0, 0, 0.0),
        Category::Analytic => return None,
    };
    Some(match tol {
        Tolerance::Canonical | Tolerance::Relaxed => canonical,
    })
}

/// The §6.10 PASS rule.
pub fn passes(m: &Metrics, l: &Limits) -> bool {
    m.max_interior_delta <= l.max_interior && m.pct_over <= l.max_pct_over && m.mae <= l.max_mae
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_has_limits() {
        for c in Category::ALL {
            assert_eq!(limits(c, Tolerance::Canonical).is_some(), c != Category::Analytic, "{c:?}");
        }
        assert_eq!(limits(Category::SelfConsistency, Tolerance::Canonical), Some(l(0, 0.0, 0, 0.0)));
    }

    #[test]
    fn relaxed_is_never_stricter_than_canonical() {
        for c in Category::ALL {
            let (Some(a), Some(b)) = (limits(c, Tolerance::Canonical), limits(c, Tolerance::Relaxed)) else {
                continue;
            };
            assert!(b.over_threshold >= a.over_threshold, "{c:?}");
            assert!(b.max_pct_over >= a.max_pct_over, "{c:?}");
            assert!(b.max_interior >= a.max_interior, "{c:?}");
            assert!(b.max_mae >= a.max_mae, "{c:?}");
        }
    }

    #[test]
    fn passes_applies_all_three_limits() {
        let lim = l(2, 0.0, 1, 0.10);
        let ok = Metrics {
            max_delta: 50,
            max_interior_delta: 1,
            over_count: 0,
            pct_over: 0.0,
            mae: 0.1,
            pixels: 1,
        };
        assert!(passes(&ok, &lim));
        assert!(!passes(&Metrics { max_interior_delta: 2, ..ok }, &lim));
        assert!(!passes(&Metrics { pct_over: 0.01, ..ok }, &lim));
        assert!(!passes(&Metrics { mae: 0.11, ..ok }, &lim));
    }
}
