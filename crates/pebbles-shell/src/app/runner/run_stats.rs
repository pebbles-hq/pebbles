//! `PEBBLES_FRAME_STATS=1` (web: `?pebbles-frame-stats=1`): one `[pebbles run]` summary line per
//! second of activity, for the scripted desktop runs (08 §15 C2, M-RESIZE, M-MULTI, M-THREAD;
//! `scripts/desktop-run.py`). Counts outcomes in BOTH frame modes (inline frames settle on the
//! UI thread, threaded ones through `FrameDone`), so it is the place where
//! `resize_sync_timeouts` (08 §17.4) and the present cadence become visible. Never per frame.

#[allow(clippy::wildcard_imports)]
use super::*;

/// Seconds between summary lines.
const PERIOD: Duration = Duration::from_secs(1);

/// The per-period counters.
#[derive(Default)]
pub(super) struct RunStats {
    started: Option<Instant>,
    last_line: Option<Instant>,
    presented_main: u64,
    presented_secondary: u64,
    unpresented: u64,
    period_main: u64,
    period_secondary: u64,
    period_unpresented: u64,
    last_main_present: Option<Instant>,
    /// Main-window present-to-present intervals this period (ms).
    gaps_ms: Vec<f64>,
    /// Main-window `render()` wall time this period (ms; inline includes the present).
    ui_ms: Vec<f64>,
    /// The oldest synthetic input not yet followed by a main-window present.
    input_at: Option<Instant>,
    /// Input-to-present latency this period (ms).
    input_ms: Vec<f64>,
    /// Outcomes of the frames that were not presented this period, by kind.
    period_outcomes: Vec<(FrameOutcome, u64)>,
}

/// `(max, p95)` of `v`, `(0, 0)` when empty.
fn max_p95(v: &mut [f64]) -> (f64, f64) {
    if v.is_empty() {
        return (0.0, 0.0);
    }
    v.sort_by(f64::total_cmp);
    let p95 = v[((v.len() - 1) * 95) / 100];
    (v[v.len() - 1], p95)
}

impl RunStats {
    /// A settled frame of the main window or a secondary one.
    pub(super) fn frame(&mut self, main: bool, outcome: FrameOutcome) {
        let now = Instant::now();
        self.started.get_or_insert(now);
        if outcome != FrameOutcome::Presented {
            self.unpresented += 1;
            self.period_unpresented += 1;
            match self.period_outcomes.iter_mut().find(|(o, _)| *o == outcome) {
                Some((_, n)) => *n += 1,
                None => self.period_outcomes.push((outcome, 1)),
            }
            return;
        }
        if !main {
            self.presented_secondary += 1;
            self.period_secondary += 1;
            return;
        }
        self.presented_main += 1;
        self.period_main += 1;
        if let Some(prev) = self.last_main_present.replace(now) {
            self.gaps_ms.push((now - prev).as_secs_f64() * 1e3);
        }
        if let Some(at) = self.input_at.take() {
            self.input_ms.push((now - at).as_secs_f64() * 1e3);
        }
    }

    /// One main-window `render()` took `ms`.
    pub(super) fn ui_frame(&mut self, ms: f64) {
        self.ui_ms.push(ms);
    }

    /// Synthetic input reached the main window's UI (input storm).
    pub(super) fn input(&mut self) {
        self.input_at.get_or_insert_with(Instant::now);
    }

    /// Whether a line is due.
    pub(super) fn due(&self) -> bool {
        match (self.started, self.last_line) {
            (None, _) => false,
            (Some(s), None) => s.elapsed() >= PERIOD,
            (Some(_), Some(l)) => l.elapsed() >= PERIOD,
        }
    }
}

impl Runner {
    /// Emit the `[pebbles run]` line when one is due.
    pub(super) fn run_stats_tick(&mut self) {
        if !render::frame_stats_enabled() || !self.run_stats.due() {
            return;
        }
        let threaded = self.sink.as_ref().is_some_and(|s| s.state().is_none());
        let configures = self
            .render_state()
            .map(|s| {
                let mut v: Vec<String> = s
                    .surfaces
                    .values()
                    .map(|surface| surface.configure_count().map_or_else(|| "n/a".into(), |c| c.to_string()))
                    .collect();
                v.sort();
                v.join(",")
            })
            .unwrap_or_else(|| "n/a".into());
        let (size, pos, scale, occluded) = match self.active.as_ref() {
            Some(a) => {
                let s = a.window.inner_size();
                let p = a
                    .window
                    .outer_position()
                    .map(|p| format!("{},{}", p.x, p.y))
                    .unwrap_or_else(|_| "?".into());
                (format!("{}x{}", s.width, s.height), p, a.window.scale_factor(), a.occlusion.is_occluded())
            }
            None => ("0x0".into(), "?".into(), 0.0, false),
        };
        let occluded_secondary = self.windows.values().filter(|w| w.occlusion.is_occluded()).count();
        let r = &mut self.run_stats;
        let now = Instant::now();
        let t = r.started.map_or(0.0, |s| (now - s).as_secs_f64());
        let (gap_max, gap_p95) = max_p95(&mut r.gaps_ms);
        let (ui_max, ui_p95) = max_p95(&mut r.ui_ms);
        let (in_max, _) = max_p95(&mut r.input_ms);
        let outcomes = if r.period_outcomes.is_empty() {
            "-".to_owned()
        } else {
            r.period_outcomes.iter().map(|(o, n)| format!("{o:?}:{n}")).collect::<Vec<_>>().join(",")
        };
        let line = format!(
            "[pebbles run] t={t:.1}s mode={} windows={} size={size} pos={pos} scale={scale:.2} \
             presented=+{}/{} secondary=+{}/{} unpresented=+{}/{} occluded={}/{occluded_secondary} \
             resize_sync_timeouts={} gen={} gap_ms={gap_max:.1}/{gap_p95:.1} ui_ms={ui_max:.1}/{ui_p95:.1} \
             input_ms={in_max:.1} configures={configures} not_presented={outcomes}",
            if threaded { "threaded" } else { "inline" },
            1 + self.windows.len(),
            r.period_main,
            r.presented_main,
            r.period_secondary,
            r.presented_secondary,
            r.period_unpresented,
            r.unpresented,
            u8::from(occluded),
            self.resize_sync_timeouts,
            self.gpu_generation,
        );
        r.period_main = 0;
        r.period_secondary = 0;
        r.period_unpresented = 0;
        r.gaps_ms.clear();
        r.ui_ms.clear();
        r.input_ms.clear();
        r.period_outcomes.clear();
        r.last_line = Some(now);
        #[cfg(target_family = "wasm")]
        web_sys::console::log_1(&line.into());
        #[cfg(not(target_family = "wasm"))]
        eprintln!("{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameOutcome, RunStats, max_p95};

    #[test]
    fn percentiles() {
        assert_eq!(max_p95(&mut []), (0.0, 0.0));
        let mut v: Vec<f64> = (1..=100).map(f64::from).collect();
        v.reverse();
        assert_eq!(max_p95(&mut v), (100.0, 95.0));
    }

    #[test]
    fn counts_by_window_and_outcome() {
        let mut r = RunStats::default();
        assert!(!r.due());
        r.frame(true, FrameOutcome::Presented);
        r.frame(false, FrameOutcome::Presented);
        r.frame(true, FrameOutcome::Skipped);
        assert_eq!((r.presented_main, r.presented_secondary, r.unpresented), (1, 1, 1));
        assert_eq!(r.period_outcomes, vec![(FrameOutcome::Skipped, 1)]);
        r.input();
        r.frame(true, FrameOutcome::Presented);
        assert_eq!(r.input_ms.len(), 1);
        assert_eq!(r.gaps_ms.len(), 1);
    }
}
