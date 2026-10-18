//! The per-case runner (§6.11): build → frame scene → candidate → stability → smoke →
//! verdict → artifacts. `main.rs` adds the watchdog and the report files.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::time::Instant;

use crate::CheckError;
use crate::args::{RefMode, Tolerance};
use crate::candidate::Candidate;
use crate::case::{Case, Category, Expect, Reference};
use crate::golden::Golden;
use crate::metric::{Rgba8, compare, smoke_compare};
use crate::reference::HybridReference;
use crate::report::{CaseResult, diff_image, side_by_side};
use crate::scene::{frame_scene, logical_scene};
use crate::status::Status;
use crate::thresholds::{Limits, limits, passes};
use crate::{png_io, probe};

/// Runner options (from `Args`).
#[derive(Clone, Debug)]
pub struct RunOptions {
    pub reference: RefMode,
    pub smoke: bool,
    pub reference_dir: PathBuf,
    pub tolerance: Tolerance,
    pub determinism: bool,
    pub keep_images: bool,
    pub tier_a: bool,
    /// `<out>/<tag>/cases`; `None` = write no artifacts.
    pub cases_dir: Option<PathBuf>,
}

impl Default for RunOptions {
    fn default() -> Self {
        let a = crate::args::Args::default();
        Self {
            reference: a.reference,
            smoke: a.smoke,
            reference_dir: a.reference_dir,
            tolerance: a.tolerance,
            determinism: false,
            keep_images: false,
            tier_a: true,
            cases_dir: None,
        }
    }
}

/// A finished case plus its images (the candidate is kept for `--write-refs`).
pub struct Outcome {
    pub result: CaseResult,
    pub candidate: Option<Rgba8>,
}

/// Candidate + smoke reference on one device.
pub struct Runner {
    pub candidate: Candidate,
    smoke: HybridReference,
    pub opts: RunOptions,
}

fn panic_message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic payload".to_string())
}

fn expect_label(e: Expect) -> (String, Option<&'static str>) {
    match e {
        Expect::Pass => ("pass".to_string(), None),
        Expect::Xfail { until, .. } => (format!("xfail({until})"), Some(until)),
    }
}

/// Map a spec verdict through the case's expectation.
fn verdict(pass: bool, e: Expect) -> Status {
    match (pass, e) {
        (true, Expect::Pass) => Status::Pass,
        (false, Expect::Pass) => Status::Fail,
        (true, Expect::Xfail { .. }) => Status::Xpass,
        (false, Expect::Xfail { .. }) => Status::Xfail,
    }
}

/// The premultiplied RGBA8 of an opaque-forced background.
fn bg_rgba8(c: peniko::Color) -> [u8; 4] {
    let p = c.with_alpha(1.0).to_rgba8();
    [p.r, p.g, p.b, p.a]
}

impl Runner {
    /// A runner on a new device; `Err(NoAdapter)` means exit 3.
    pub fn new(opts: RunOptions) -> Result<Self, CheckError> {
        let mut candidate = Candidate::new()?;
        candidate.set_tier_a(opts.tier_a);
        let smoke = HybridReference::new(candidate.device().clone(), candidate.queue().clone());
        Ok(Self { candidate, smoke, opts })
    }

    /// New device for both sides (after a PANIC or GPU_ERROR).
    pub fn recreate(&mut self) -> Result<(), CheckError> {
        self.candidate.recreate()?;
        self.candidate.set_tier_a(self.opts.tier_a);
        self.smoke = HybridReference::new(self.candidate.device().clone(), self.candidate.queue().clone());
        Ok(())
    }

    fn render_candidate(&mut self, case: &Case, frame: u32) -> Result<Rgba8, (Status, String)> {
        let size = case.device_size();
        let (bg, scale) = (case.background, case.scale);
        let built = catch_unwind(|| frame_scene(&logical_scene(case, frame), scale))
            .map_err(|p| (Status::Panic, format!("build: {}", panic_message(&*p))))?;
        let cand = &mut self.candidate;
        catch_unwind(AssertUnwindSafe(|| cand.render_rgba(&built, size, bg, 1.0)))
            .map_err(|p| (Status::Panic, format!("candidate: {}", panic_message(&*p))))?
            .map_err(|e| (Status::GpuError, e.to_string()))
    }

    /// Run one case. Never panics on a case failure; a hard status leaves the device to be
    /// recreated by the caller (`needs_recreate`).
    pub fn run_case(&mut self, case: &Case) -> Outcome {
        let (expect, xfail_until) = expect_label(case.expect);
        let mut r = CaseResult {
            name: case.name.clone(),
            slice: case.slice,
            category: case.category,
            status: Status::Pass,
            expect,
            xfail_until,
            device_size: case.device_size(),
            scale: case.scale,
            metrics: None,
            limits: limits(case.category, self.opts.tolerance),
            probes: Vec::new(),
            fallbacks: Vec::new(),
            smoke: None,
            note: String::new(),
            artifacts: None,
            millis_reference: 0,
            millis_candidate: 0,
        };
        let t = Instant::now();
        let cand = match self.render_candidate(case, 0) {
            Ok(img) => img,
            Err((status, note)) => {
                r.status = status;
                r.note = note;
                r.millis_candidate = t.elapsed().as_millis();
                return Outcome { result: r, candidate: None };
            }
        };
        r.fallbacks = self.candidate.last_fallbacks();
        // Stability: frames 1..n of a FrameOps case, then the --determinism re-render.
        let reruns = (1..case.frames).chain(self.opts.determinism.then_some(0));
        for frame in reruns {
            match self.render_candidate(case, frame) {
                Ok(again) if again == cand => {}
                Ok(_) => {
                    r.status = Status::Unstable;
                    r.note = format!("frame {frame} differs from frame 0");
                }
                Err((status, note)) => {
                    r.status = status;
                    r.note = note;
                    return Outcome { result: r, candidate: Some(cand) };
                }
            }
        }
        r.millis_candidate = t.elapsed().as_millis();

        let t = Instant::now();
        let smoke_img = if self.opts.smoke {
            match self.render_smoke(case) {
                Ok(img) => Some(img),
                Err((status, note)) => {
                    r.status = status;
                    r.note = note;
                    return Outcome { result: r, candidate: Some(cand) };
                }
            }
        } else {
            None
        };
        r.smoke = smoke_img.as_ref().and_then(|s| smoke_compare(s, &cand, bg_rgba8(case.background)).ok());
        let reference = self.judge(case, &cand, &mut r);
        r.millis_reference = t.elapsed().as_millis();
        self.write_artifacts(case, reference.as_ref().or(smoke_img.as_ref()), &cand, &mut r);
        Outcome { result: r, candidate: Some(cand) }
    }

    fn render_smoke(&mut self, case: &Case) -> Result<Rgba8, (Status, String)> {
        let (w, h) = case.device_size();
        let built = catch_unwind(|| frame_scene(&logical_scene(case, 0), case.scale))
            .map_err(|p| (Status::Panic, format!("build: {}", panic_message(&*p))))?;
        let smoke = &mut self.smoke;
        let data = catch_unwind(AssertUnwindSafe(|| smoke.render_rgba(&built, w, h, case.background)))
            .map_err(|p| (Status::Panic, format!("smoke reference: {}", panic_message(&*p))))?
            .map_err(|e| (Status::GpuError, e.to_string()))?;
        Ok(Rgba8 { width: w, height: h, data })
    }

    /// Render the case with the golden model (§6.7a). An op the model does not support yet
    /// (V45/V46) is a `PROBE_FAIL`, never a pass.
    fn render_golden(&self, case: &Case) -> Result<Rgba8, (Status, String)> {
        let (w, h) = case.device_size();
        let built = catch_unwind(|| frame_scene(&logical_scene(case, 0), case.scale))
            .map_err(|p| (Status::Panic, format!("build: {}", panic_message(&*p))))?;
        catch_unwind(AssertUnwindSafe(|| Golden::render(&built, w, h, case.background)))
            .map_err(|p| (Status::Panic, format!("golden: {}", panic_message(&*p))))?
            .map_err(|e| (Status::ProbeFail, e.to_string()))
    }

    /// Apply the spec reference; returns the reference image when one exists.
    fn judge(&self, case: &Case, cand: &Rgba8, r: &mut CaseResult) -> Option<Rgba8> {
        let probe_fails = probe::run_probes(case, cand, &mut r.probes);
        if r.status == Status::Unstable {
            return None;
        }
        let golden_mode = self.opts.reference == RefMode::Golden;
        let dir_mode = !golden_mode && case.in_refs;
        let reference_img = match (case.reference, dir_mode) {
            (Reference::SelfConsistency, _) => {
                r.status = verdict(true, case.expect);
                return None;
            }
            (Reference::Analytic, false) => {
                r.status = verdict(probe_fails == 0, case.expect);
                return None;
            }
            (Reference::SelfGolden, _) | (_, true) => {
                let path = self.opts.reference_dir.join(format!("{}.png", case.file_stem()));
                match png_io::decode(&path) {
                    Ok(img) => Some(img),
                    Err(_) if dir_mode => {
                        r.status = Status::MissingRef;
                        r.note = format!("missing {}", path.display());
                        return None;
                    }
                    // Self-goldens land with V12; until then smoke + no hard status judge.
                    Err(_) => None,
                }
            }
            (Reference::Golden, false) => match self.render_golden(case) {
                Ok(img) => Some(img),
                Err((status, note)) => {
                    r.status = status;
                    r.note = note;
                    return None;
                }
            },
        };
        let Some(reference_img) = reference_img else {
            r.status = if probe_fails > 0 { verdict(false, case.expect) } else { Status::Unjudged };
            if r.status == Status::Fail {
                r.status = Status::ProbeFail;
            }
            if r.status == Status::Unjudged {
                r.note = match case.reference {
                    Reference::SelfGolden => "no self-golden yet (V12); smoke only".into(),
                    _ => "no reference image; smoke only".into(),
                };
            }
            return None;
        };
        let lim = if dir_mode { limits(Category::CrossBackend, self.opts.tolerance) } else { r.limits };
        r.limits = lim;
        match lim.map(|l| (compare(&reference_img, cand, l.over_threshold), l)) {
            Some((Ok(m), l)) => {
                r.metrics = Some(m);
                r.status = verdict(passes(&m, &l) && probe_fails == 0, case.expect);
            }
            Some((Err(e), _)) => {
                r.status = Status::Size;
                r.note = e.to_string();
            }
            None => r.status = verdict(probe_fails == 0, case.expect),
        }
        Some(reference_img)
    }

    fn write_artifacts(&self, case: &Case, reference: Option<&Rgba8>, cand: &Rgba8, r: &mut CaseResult) {
        let Some(root) = &self.opts.cases_dir else { return };
        if r.status == Status::Pass && !self.opts.keep_images {
            return;
        }
        let dir = root.join(case.file_stem());
        let lim = r.limits.unwrap_or(Limits {
            over_threshold: 0,
            max_pct_over: 0.0,
            max_interior: 0,
            max_mae: 0.0,
        });
        let mut files = vec![("candidate", cand.clone())];
        if let Some(refimg) = reference.filter(|i| (i.width, i.height) == (cand.width, cand.height)) {
            let diff = diff_image(refimg, cand, &lim);
            files.push(("side", side_by_side(refimg, cand, &diff)));
            files.push(("diff", diff));
            files.push(("reference", refimg.clone()));
        }
        for (name, img) in &files {
            if let Err(e) = png_io::encode(&dir.join(format!("{name}.png")), img) {
                r.note = format!("{} (artifact write failed: {e})", r.note);
                return;
            }
        }
        r.artifacts = Some(format!("cases/{}/", case.file_stem()));
    }
}

/// Whether `status` leaves the device suspect (§6.11: recreate both sides).
pub fn needs_recreate(status: Status) -> bool {
    matches!(status, Status::Panic | Status::GpuError)
}
