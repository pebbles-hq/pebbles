//! The case model (§6.5): what to draw, how it is judged, and which gate it belongs to.

use pebbles_foundation::{Color, palette};
use pebbles_render::paint::Painter;
use pebbles_testing::Harness;

/// The threshold category (§8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Exact,
    Geometry,
    Gradient,
    Image,
    Shadow,
    Layer,
    Text,
    Mesh,
    /// Layer effects and backdrop filters (09 §10; 8-bit passes mirrored by the golden model).
    Effect,
    /// Perspective layers (09 §10.7; bilinear resampling).
    Projected,
    Screen,
    Stress,
    SelfConsistency,
    CrossBackend,
    Analytic,
}

impl Category {
    /// Every category.
    pub const ALL: [Category; 15] = [
        Self::Exact,
        Self::Geometry,
        Self::Gradient,
        Self::Image,
        Self::Shadow,
        Self::Layer,
        Self::Text,
        Self::Mesh,
        Self::Effect,
        Self::Projected,
        Self::Screen,
        Self::Stress,
        Self::SelfConsistency,
        Self::CrossBackend,
        Self::Analytic,
    ];
}

/// Which phase gate the case belongs to (§12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slice {
    P1,
    P2,
    P3,
    P4,
    TrackT,
    Screens,
    Robustness,
}

/// An analytic check on a rendered image (`scale` = the case's device scale).
pub type Probe = fn(img: &crate::metric::Rgba8, scale: f64) -> Result<(), String>;

/// How the case's logical Scene is recorded.
#[derive(Clone, Copy)]
pub enum Build {
    /// Record directly into a logical-px Scene.
    Ops(fn(&mut Painter<'_>)),
    /// Mount + interact through the GPU-free harness; the runner settles and draws.
    Harness(fn(&mut Harness)),
    /// Frame-indexed ops for stability cases (`frame` is 0-based).
    FrameOps(fn(&mut Painter<'_>, u32)),
}

/// What judges the case.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reference {
    /// Compare against the golden model (02 §5 spec, §6.7a). Default for op-level cases.
    Golden,
    /// Only probes judge (closed-form expectations).
    Analytic,
    /// Compare against the committed self-golden (widget/screen/stress; V12).
    SelfGolden,
    /// Compare the candidate with itself across frames / a fresh renderer (byte-equal).
    SelfConsistency,
}

/// The expected verdict.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    Pass,
    /// Known failure until card `until` lands (§2.6).
    Xfail {
        until: &'static str,
        reason: &'static str,
    },
}

/// One corpus case.
#[derive(Clone)]
pub struct Case {
    /// `"<family>/<case>[_s125]"`.
    pub name: String,
    pub category: Category,
    pub slice: Slice,
    /// Logical size.
    pub logical: (u32, u32),
    pub scale: f64,
    pub background: Color,
    pub build: Build,
    pub reference: Reference,
    pub probes: Vec<(&'static str, Probe)>,
    pub expect: Expect,
    /// ≥ 1; > 1 ⇒ stability case.
    pub frames: u32,
    /// Committed by V12?
    pub in_refs: bool,
    /// How a stability case (`frames > 1`, or `LastVsFresh`) is judged.
    pub stability: Stability,
    /// A documented per-case `max_pct_over` (09 §8.0 allows exactly one: the SDF clip
    /// conflation of `clip/rrect_overlap_edge`); `None` = the category's.
    pub pct_over_override: Option<f64>,
}

/// How a stability case is judged (§7.2 frames/).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    /// Every frame byte-equals frame 0.
    EveryFrame,
    /// Frames 0..n run on the shared (warm) renderer; the last one byte-equals the same frame
    /// rendered by a fresh renderer (atlas / image-cache / FrameTargets staleness).
    LastVsFresh,
}

fn base(name: &str, category: Category, slice: Slice, w: u32, h: u32, build: Build) -> Case {
    Case {
        name: name.to_string(),
        category,
        slice,
        logical: (w, h),
        scale: 1.0,
        background: palette::WHITE,
        build,
        // §6.1 / V47: op-level cases are judged by the golden model; widget/, screen/ and
        // stress/ (categories Screen and Stress) by committed self-goldens (V12).
        reference: if matches!(category, Category::Screen | Category::Stress) {
            Reference::SelfGolden
        } else {
            Reference::Golden
        },
        probes: Vec::new(),
        expect: Expect::Pass,
        frames: 1,
        in_refs: true,
        stability: Stability::EveryFrame,
        pct_over_override: None,
    }
}

/// An op-recorded case.
pub fn ops(name: &str, category: Category, slice: Slice, w: u32, h: u32, f: fn(&mut Painter<'_>)) -> Case {
    base(name, category, slice, w, h, Build::Ops(f))
}

/// A widget case built through `pebbles_testing::Harness`.
pub fn harness(name: &str, category: Category, slice: Slice, w: u32, h: u32, f: fn(&mut Harness)) -> Case {
    base(name, category, slice, w, h, Build::Harness(f))
}

/// A frame-indexed stability case (SelfConsistency).
pub fn frame_ops(
    name: &str,
    slice: Slice,
    w: u32,
    h: u32,
    frames: u32,
    f: fn(&mut Painter<'_>, u32),
) -> Case {
    base(name, Category::SelfConsistency, slice, w, h, Build::FrameOps(f)).frames(frames)
}

impl Case {
    /// Set the background.
    pub fn bg(mut self, c: Color) -> Self {
        self.background = c;
        self
    }

    /// Add a probe.
    pub fn probe(mut self, name: &'static str, p: Probe) -> Self {
        self.probes.push((name, p));
        self
    }

    /// Probes only judge; never committed.
    /// Override the category's `max_pct_over` (documented exceptions only).
    pub fn pct_over(mut self, max: f64) -> Self {
        self.pct_over_override = Some(max);
        self
    }

    pub fn analytic(mut self) -> Self {
        self.reference = Reference::Analytic;
        self.in_refs = false;
        self
    }

    /// Known failure until `until`.
    pub fn xfail(mut self, until: &'static str, reason: &'static str) -> Self {
        self.expect = Expect::Xfail { until, reason };
        self
    }

    /// Render `n` frames and require byte equality.
    pub fn frames(mut self, n: u32) -> Self {
        self.frames = n.max(1);
        self.reference = Reference::SelfConsistency;
        self.in_refs = false;
        self
    }

    /// `n` frames on the warm renderer; the last compared with a fresh renderer's render.
    pub fn last_vs_fresh(mut self, n: u32) -> Self {
        self = self.frames(n);
        self.stability = Stability::LastVsFresh;
        self
    }

    /// Not committed by V12.
    pub fn no_refs(mut self) -> Self {
        self.in_refs = false;
        self
    }

    /// Device px: `ceil(logical * scale)`.
    pub fn device_size(&self) -> (u32, u32) {
        let d = |v: u32| (f64::from(v) * self.scale).ceil() as u32;
        (d(self.logical.0), d(self.logical.1))
    }

    /// `name` with `/` → `__`.
    pub fn file_stem(&self) -> String {
        self.name.replace('/', "__")
    }
}

/// The scaled variants beyond 1.0 (DR: 1.25/1.5/1.75/2.0).
pub const SCALES: [(f64, &str); 4] = [(1.25, "_s125"), (1.5, "_s150"), (1.75, "_s175"), (2.0, "_s200")];

/// The base case plus `_s125`, `_s150`, `_s175`, `_s200` variants.
/// A scaled variant of an `Exact` case becomes `Geometry` (edges stop being pixel-aligned).
pub fn with_scale_variants(base: Case) -> Vec<Case> {
    let mut out = vec![base.clone()];
    for (scale, suffix) in SCALES {
        let mut c = base.clone();
        c.name = format!("{}{suffix}", base.name);
        c.scale = scale;
        if c.category == Category::Exact {
            c.category = Category::Geometry;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn nop(_: &mut Painter<'_>) {}

    #[test]
    fn scale_variants_generate_five_cases() {
        let v = with_scale_variants(ops("fam/x", Category::Exact, Slice::P1, 128, 100, nop));
        let names: Vec<_> = v.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["fam/x", "fam/x_s125", "fam/x_s150", "fam/x_s175", "fam/x_s200"]);
        assert_eq!(v[0].category, Category::Exact);
        assert!(v[1..].iter().all(|c| c.category == Category::Geometry));
        let sizes: Vec<_> = v.iter().map(Case::device_size).collect();
        assert_eq!(sizes, [(128, 100), (160, 125), (192, 150), (224, 175), (256, 200)]);
    }

    #[test]
    fn builders_have_the_documented_defaults() {
        let c = ops("fam/x", Category::Geometry, Slice::P1, 64, 64, nop);
        assert_eq!(c.scale, 1.0);
        assert_eq!(c.background, palette::WHITE);
        assert_eq!(c.reference, Reference::Golden);
        assert_eq!(c.expect, Expect::Pass);
        assert!(c.probes.is_empty() && c.in_refs && c.frames == 1);
        let a = c.clone().analytic();
        assert_eq!(a.reference, Reference::Analytic);
        assert!(!a.in_refs);
        assert_eq!(c.file_stem(), "fam__x");
    }
}
