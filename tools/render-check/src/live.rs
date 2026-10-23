//! Live-only smoke checks for example screens (V27, transitional): a scene the corpus cannot
//! hold (the desktop and landing examples fetch network images, so they can never be
//! self-goldens) is rendered by the candidate and the vello-hybrid smoke reference and judged
//! by the Smoke metric (§6.10) only. Deleted with the smoke reference at Phase 7 (V43).

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;

use pebbles_render::paint::Scene;

use crate::CheckError;
use crate::candidate::Candidate;
use crate::case::Category;
use crate::metric::{Rgba8, SmokeMetrics, smoke_compare};
use crate::reference::HybridReference;
use crate::scene::frame_scene;
use crate::status::Status;

/// The outcome of one live check.
#[derive(Clone, Debug)]
pub struct LiveOutcome {
    pub name: String,
    pub category: Category,
    pub status: Status,
    /// `None` when the check stopped before the smoke comparison.
    pub metrics: Option<SmokeMetrics>,
    /// Why a non-`Pass` status was given.
    pub detail: String,
    /// Where the PNGs were written.
    pub artifacts: PathBuf,
}

impl LiveOutcome {
    /// `Pass` only: no panic, no GPU error, stable, and the Smoke metric holds.
    pub fn passed(&self) -> bool {
        self.status == Status::Pass
    }

    /// One line for an assertion message.
    pub fn summary(&self) -> String {
        let m = self.metrics.map_or_else(
            || "no metrics".to_owned(),
            |m| format!("max_tile_delta={:.2} coverage_iou={:.4}", m.max_tile_delta, m.coverage_iou),
        );
        let detail = if self.detail.is_empty() { String::new() } else { format!(" — {}", self.detail) };
        format!("{} {:?}: {m}{detail} (artifacts: {})", self.name, self.status, self.artifacts.display())
    }
}

/// `$PEBBLES_RENDER_CHECK_OUT` (default: the workspace's `target/render-check`; tests run in their
/// package directory, so a relative default would scatter artifacts) `/live/<name, '/' → "__">/`.
fn artifact_dir(name: &str) -> PathBuf {
    let workspace_target = || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/render-check");
    let root = std::env::var_os("PEBBLES_RENDER_CHECK_OUT").map_or_else(workspace_target, PathBuf::from);
    root.join("live").join(name.replace('/', "__"))
}

fn panic_message(p: &(dyn std::any::Any + Send)) -> String {
    p.downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "non-string panic".to_owned())
}

/// Render `scene` (logical px, device scale 1.0) with the candidate twice (stability) and with
/// the smoke reference, judge with the Smoke metric, and write `candidate.png` /
/// `reference.png`. No adapter → `Err(CheckError::NoAdapter)`; every other failure is a status.
pub fn check_scene(
    name: &str,
    category: Category,
    scene: &Scene,
    width: u32,
    height: u32,
    background: peniko::Color,
) -> Result<LiveOutcome, CheckError> {
    let mut candidate = Candidate::new()?;
    let mut smoke = HybridReference::new(candidate.device().clone(), candidate.queue().clone());
    let artifacts = artifact_dir(name);
    let mut out = LiveOutcome {
        name: name.to_owned(),
        category,
        status: Status::Pass,
        metrics: None,
        detail: String::new(),
        artifacts: artifacts.clone(),
    };
    let built = frame_scene(scene, 1.0);
    let render = |candidate: &mut Candidate| -> Result<Rgba8, (Status, String)> {
        catch_unwind(AssertUnwindSafe(|| candidate.render_rgba(&built, (width, height), background, 1.0)))
            .map_err(|p| (Status::Panic, format!("candidate: {}", panic_message(&*p))))?
            .map_err(|e| (Status::GpuError, e.to_string()))
    };
    let judged = (|| {
        let first = render(&mut candidate)?;
        let second = render(&mut candidate)?;
        if first.data != second.data {
            return Err((Status::Unstable, "a second frame of the same scene differs".to_owned()));
        }
        let reference =
            catch_unwind(AssertUnwindSafe(|| smoke.render_rgba(&built, width, height, background)))
                .map_err(|p| (Status::Panic, format!("smoke reference: {}", panic_message(&*p))))?
                .map_err(|e| (Status::GpuError, e.to_string()))?;
        Ok((first, Rgba8 { width, height, data: reference }))
    })();
    match judged {
        Err((status, detail)) => {
            out.status = status;
            out.detail = detail;
        }
        Ok((cand, reference)) => {
            let bg = background.with_alpha(1.0).to_rgba8();
            match smoke_compare(&reference, &cand, [bg.r, bg.g, bg.b, bg.a]) {
                Ok(m) => {
                    out.metrics = Some(m);
                    if !m.passes() {
                        out.status = Status::Fail;
                        out.detail = "Smoke metric over its limits (tile delta ≤ 24, IoU ≥ 0.97)".to_owned();
                    }
                }
                Err(e) => {
                    out.status = Status::Size;
                    out.detail = format!("{e:?}");
                }
            }
            std::fs::create_dir_all(&artifacts).map_err(|e| CheckError::Io(e.to_string()))?;
            crate::png_io::encode(&artifacts.join("candidate.png"), &cand)?;
            crate::png_io::encode(&artifacts.join("reference.png"), &reference)?;
        }
    }
    Ok(out)
}
