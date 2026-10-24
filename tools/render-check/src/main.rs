//! `render-check` binary: argument handling, `--list`, the case loop, the watchdog,
//! reports and exit codes (§6.4, §6.11, §6.12).

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use render_check::args::{self, Args};
use render_check::case::{Case, Expect, Reference, Slice};
use render_check::report::{CaseResult, RunInfo, write_report_json, write_report_md};
use render_check::run::{RunOptions, Runner, needs_recreate};
use render_check::{Status, corpus, exit_code, png_io};

/// No case in flight.
const IDLE: u64 = u64::MAX;

fn expect_label(c: &Case) -> String {
    match c.expect {
        Expect::Pass => "pass".to_string(),
        Expect::Xfail { until, .. } => format!("xfail({until})"),
    }
}

/// Adapter name lowercased, non-alphanumerics → `-`, runs collapsed, trimmed, ≤ 32 chars.
fn slug(name: &str) -> String {
    let mut s = String::new();
    for ch in name.to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            s.push(ch);
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    let s: String = s.trim_matches('-').chars().take(32).collect();
    s.trim_end_matches('-').to_string()
}

fn git_rev() -> String {
    if let Ok(sha) = std::env::var("GITHUB_SHA") {
        return sha.chars().take(7).collect();
    }
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn case_timeout(slice: Slice) -> u64 {
    let secs = std::env::var("PEBBLES_RENDER_CHECK_CASE_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok());
    let secs: u64 = secs.unwrap_or(30);
    if slice == Slice::Robustness { secs * 4 } else { secs }
}

fn write_reports(dir: &Path, info: &RunInfo, results: &[CaseResult]) -> Result<(), String> {
    write_report_md(dir, info, results).map_err(|e| e.to_string())?;
    write_report_json(dir, info, results).map_err(|e| e.to_string())
}

/// State the watchdog reads: case start (ms since program start, `IDLE` between cases).
struct Watch {
    epoch: Instant,
    start_ms: AtomicU64,
    current: Mutex<Option<(CaseResult, u64)>>,
    results: Mutex<Vec<CaseResult>>,
}

fn spawn_watchdog(watch: Arc<Watch>, dir: PathBuf, info: RunInfo) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let start = watch.start_ms.load(Ordering::SeqCst);
            if start == IDLE {
                continue;
            }
            let now = watch.epoch.elapsed().as_millis() as u64;
            let cur = watch.current.lock().unwrap_or_else(PoisonError::into_inner).clone();
            let Some((mut hung, timeout_s)) = cur else { continue };
            if now.saturating_sub(start) <= timeout_s * 1000 {
                continue;
            }
            hung.status = Status::Hang;
            hung.note = format!("exceeded {timeout_s} s (PEBBLES_RENDER_CHECK_CASE_TIMEOUT_SECS)");
            let mut results = watch.results.lock().unwrap_or_else(PoisonError::into_inner).clone();
            results.push(hung);
            let _ = write_reports(&dir, &info, &results);
            eprintln!("render-check: HANG; partial report in {}", dir.display());
            // A stuck GPU call cannot be cancelled.
            std::process::exit(1);
        }
    });
}

fn run(args: Args) -> ExitCode {
    // Deterministic widget scenes: animations jump to their end state (as capture does).
    pebbles_core::set_prefers_reduced_motion(true);
    let opts = RunOptions {
        reference: args.reference,
        reference_dir: args.reference_dir.clone(),
        tolerance: args.tolerance,
        determinism: args.determinism,
        keep_images: args.keep_images,
        tier_a: args.tier_a,
        glyph_hint: args.glyph_hint,
        cases_dir: None,
        cross_backend: false,
    };
    let mut runner = match Runner::new(opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("render-check: {e}");
            return ExitCode::from(3);
        }
    };
    let ai = runner.candidate.adapter_info().clone();
    if args.write_refs.is_some() && !ai.name.to_lowercase().contains("llvmpipe") {
        eprintln!("render-check: --write-refs needs an llvmpipe adapter, got `{}`", ai.name);
        return ExitCode::from(2);
    }
    runner.opts.cross_backend = !ai.name.to_lowercase().contains("llvmpipe");
    let tag = args
        .platform_tag
        .clone()
        .or_else(|| std::env::var("PEBBLES_RENDER_CHECK_PLATFORM").ok())
        .unwrap_or_else(|| format!("{}-{}", std::env::consts::OS, slug(&ai.name)));
    let dir = args.out.join(&tag);
    runner.opts.cases_dir = Some(dir.join("cases"));
    let info = RunInfo {
        git_rev: git_rev(),
        platform_tag: tag,
        reference_mode: args.reference.label().to_string(),
        tolerance: args.tolerance.label().to_string(),
        adapter_name: ai.name.clone(),
        backend: format!("{:?}", ai.backend),
        device_type: format!("{:?}", ai.device_type),
        driver: ai.driver.clone(),
        driver_info: ai.driver_info.clone(),
        msaa: None,
    };
    let watch = Arc::new(Watch {
        epoch: Instant::now(),
        start_ms: AtomicU64::new(IDLE),
        current: Mutex::new(None),
        results: Mutex::new(Vec::new()),
    });
    spawn_watchdog(Arc::clone(&watch), dir.clone(), info.clone());

    let cases: Vec<Case> = corpus::all().into_iter().filter(|c| args.selects(&c.name)).collect();
    for case in &cases {
        let placeholder = runner_placeholder(case);
        *watch.current.lock().unwrap_or_else(PoisonError::into_inner) =
            Some((placeholder, case_timeout(case.slice)));
        watch.start_ms.store(watch.epoch.elapsed().as_millis() as u64, Ordering::SeqCst);
        let outcome = runner.run_case(case);
        watch.start_ms.store(IDLE, Ordering::SeqCst);
        let r = outcome.result;
        println!(
            "{:<11} {}{}",
            r.status.label(),
            r.name,
            if r.note.is_empty() { String::new() } else { format!("  ({})", r.note) }
        );
        // A self-golden case (widget, screen, stress) has no model to pass: with its probes green it
        // is UNJUDGED, and its candidate is exactly what the reviewed reference records (V12).
        let writable = r.status == Status::Pass
            || (r.status == Status::Unjudged && case.reference == Reference::SelfGolden);
        if let (Some(refs), Some(img), true) =
            (&args.write_refs, &outcome.candidate, case.in_refs && writable)
            && let Err(e) = png_io::encode(&refs.join(format!("{}.png", case.file_stem())), img)
        {
            eprintln!("render-check: {e}");
            return ExitCode::from(2);
        }
        let recreate = needs_recreate(r.status);
        watch.results.lock().unwrap_or_else(PoisonError::into_inner).push(r);
        if recreate && let Err(e) = runner.recreate() {
            eprintln!("render-check: could not recreate the device: {e}");
            return ExitCode::from(3);
        }
    }
    let results = watch.results.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Err(e) = write_reports(&dir, &info, &results) {
        eprintln!("render-check: {e}");
        return ExitCode::from(2);
    }
    let statuses: Vec<Status> = results.iter().map(|r| r.status).collect();
    let counts: Vec<String> = Status::ALL
        .iter()
        .map(|s| (s, statuses.iter().filter(|x| *x == s).count()))
        .filter(|(_, n)| *n > 0)
        .map(|(s, n)| format!("{}={n}", s.label()))
        .collect();
    println!(
        "render-check: {} cases: {}; report: {}",
        results.len(),
        counts.join(" "),
        dir.join("report.md").display()
    );
    ExitCode::from(exit_code(&statuses) as u8)
}

/// The row the watchdog reports if `case` hangs.
fn runner_placeholder(case: &Case) -> CaseResult {
    let until = match case.expect {
        Expect::Xfail { until, .. } => Some(until),
        Expect::Pass => None,
    };
    CaseResult {
        name: case.name.clone(),
        slice: case.slice,
        category: case.category,
        status: Status::Hang,
        expect: expect_label(case),
        xfail_until: until,
        device_size: case.device_size(),
        scale: case.scale,
        metrics: None,
        limits: None,
        probes: Vec::new(),
        fallbacks: Vec::new(),
        note: String::new(),
        artifacts: None,
        millis_reference: 0,
        millis_candidate: 0,
    }
}

fn main() -> ExitCode {
    // Test fonts join every TextEnv as user fonts, before any Harness exists (V22).
    render_check::register_test_fonts();
    let mut argv = std::env::args().skip(1).peekable();
    if argv.peek().is_some_and(|a| a == "bench") {
        argv.next();
        return match render_check::bench::parse(argv) {
            Ok(a) => ExitCode::from(render_check::bench::run(&a)),
            Err(e) => {
                eprintln!("render-check bench: {e}\n{}", render_check::bench::USAGE);
                ExitCode::from(2)
            }
        };
    }
    let args = match args::parse(argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("render-check: {e}\n{}", args::USAGE);
            return ExitCode::from(2);
        }
    };
    if args.help {
        println!("{}", args::USAGE);
        return ExitCode::SUCCESS;
    }
    if args.list {
        for c in corpus::all().iter().filter(|c| args.selects(&c.name)) {
            let (w, h) = c.logical;
            println!(
                "{}\t{:?}\t{:?}\t{w}x{h}@{}\t{}\t{}",
                c.name,
                c.category,
                c.slice,
                c.scale,
                expect_label(c),
                c.in_refs
            );
        }
        return ExitCode::SUCCESS;
    }
    run(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_matches_the_platform_tag_rule() {
        assert_eq!(slug("llvmpipe (LLVM 19.1.7, 256 bits)"), "llvmpipe-llvm-19-1-7-256-bits");
        assert!(slug("A very long adapter name with many many words in it").len() <= 32);
        assert_eq!(slug("--x--"), "x");
    }
}
