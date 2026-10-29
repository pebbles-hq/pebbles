//! `pebbles soak` — long-running soak tooling (11 §2.9, §6.3 SOAK-TOUR / SOAK-DAILY; G28, G43).
//!
//! ```text
//! pebbles soak run --bin <app> [--minutes N] [--tour] [--dir DIR] [-- app args]
//! pebbles soak summary [DIR]
//! pebbles soak systemd --bin <app> [--dir DIR]
//! ```
//!
//! `run` starts the app with `PEBBLES_SOAK_CSV` (one sample per 600 frames: RSS, GPU allocator
//! bytes, UI and renderer census) and `PEBBLES_FRAME_STATS` (a `[pebbles run]` line per active
//! second with frame times), writing into `DIR/<YYYY-MM-DD>/` (UTC): the directory rotates
//! daily, and a run that crosses midnight continues its log in the next day's directory.
//! `summary` judges every run of ≥ 30 min by the §2.9 rules and prints the day-by-day RSS / GPU
//! memory / frame-time trend, flagging growth; it exits non-zero when a run fails §2.9.
//! `systemd` prints a user unit + timer template (not installed).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::term;

pub fn run(args: &[String]) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("run") => run_app(&args[1..]),
        Some("summary") => {
            let dir = args.get(1).map_or_else(default_dir, PathBuf::from);
            let report = summarize_dir(&dir);
            print!("{}", report.text);
            if report.failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
        }
        Some("systemd") => match parse_run_args(&args[1..]) {
            Ok(opts) => {
                print!("{}", systemd_templates(&opts));
                ExitCode::SUCCESS
            }
            Err(e) => usage_error(&e),
        },
        _ => usage_error("expected `run`, `summary` or `systemd`"),
    }
}

fn usage_error(msg: &str) -> ExitCode {
    term::error(msg);
    eprintln!(
        "usage:\n  pebbles soak run --bin <app> [--minutes N] [--tour] [--strict] [--dir DIR] [-- app args]\n  \
         pebbles soak summary [DIR]\n  pebbles soak systemd --bin <app> [--dir DIR]"
    );
    ExitCode::FAILURE
}

/// `$XDG_STATE_HOME/pebbles-soak`, else `~/.local/state/pebbles-soak`.
fn default_dir() -> PathBuf {
    if let Some(s) = std::env::var_os("XDG_STATE_HOME").filter(|s| !s.is_empty()) {
        return PathBuf::from(s).join("pebbles-soak");
    }
    let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("."), PathBuf::from);
    home.join(".local/state/pebbles-soak")
}

#[derive(Debug, PartialEq)]
struct RunOpts {
    bin: PathBuf,
    minutes: Option<u64>,
    tour: bool,
    strict: bool,
    dir: PathBuf,
    app_args: Vec<String>,
}

fn parse_run_args(args: &[String]) -> Result<RunOpts, String> {
    let mut bin = None;
    let mut minutes = None;
    let mut tour = false;
    let mut strict = false;
    let mut dir = None;
    let mut app_args = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--bin" => bin = it.next().map(PathBuf::from),
            "--minutes" => {
                let v = it.next().ok_or("--minutes needs a value")?;
                minutes = Some(v.parse::<u64>().map_err(|_| format!("bad --minutes `{v}`"))?);
            }
            "--tour" => tour = true,
            "--strict" => strict = true,
            "--dir" => dir = it.next().map(PathBuf::from),
            "--" => {
                app_args.extend(it.by_ref().cloned());
            }
            other => return Err(format!("unknown option `{other}`")),
        }
    }
    let bin = bin.ok_or("--bin <app> is required")?;
    // SOAK-TOUR (§6.3) is a 60-minute run unless told otherwise.
    if tour && minutes.is_none() {
        minutes = Some(60);
    }
    Ok(RunOpts { bin, minutes, tour, strict, dir: dir.unwrap_or_else(default_dir), app_args })
}

/// The app's environment for a soak run (11 §6.3).
fn soak_env(opts: &RunOpts, csv: &Path) -> Vec<(String, String)> {
    let mut env = vec![
        ("PEBBLES_SOAK_CSV".to_string(), csv.display().to_string()),
        ("PEBBLES_FRAME_STATS".into(), "1".into()),
        ("PEBBLES_GPU_INFO".into(), "1".into()),
        ("PEBBLES_LOG".into(), "warn".into()),
        ("RUST_BACKTRACE".into(), "1".into()),
    ];
    if opts.strict || opts.tour {
        env.push(("PEBBLES_GPU_STRICT".into(), "1".into()));
    }
    if opts.tour {
        env.push(("GALLERY_TOUR".into(), "1500".into()));
        env.push(("PEBBLES_INPUT_STORM".into(), "1".into()));
    }
    env
}

/// UTC civil date `YYYY-MM-DD` for a Unix time (Howard Hinnant's days-to-civil).
fn utc_date(unix_secs: u64) -> String {
    let z = (unix_secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn day_dir(root: &Path, unix_secs: u64) -> PathBuf {
    root.join(utc_date(unix_secs))
}

fn run_app(args: &[String]) -> ExitCode {
    let opts = match parse_run_args(args) {
        Ok(o) => o,
        Err(e) => return usage_error(&e),
    };
    let start_day = day_dir(&opts.dir, unix_now());
    if let Err(e) = std::fs::create_dir_all(&start_day) {
        term::error(&format!("cannot create {}: {e}", start_day.display()));
        return ExitCode::FAILURE;
    }
    let csv = start_day.join("soak.csv");
    term::banner("pebbles soak");
    term::step(&format!("{} → {}", opts.bin.display(), start_day.display()));
    let mut child = match Command::new(&opts.bin)
        .args(&opts.app_args)
        .envs(soak_env(&opts, &csv))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            term::error(&format!("cannot start {}: {e}", opts.bin.display()));
            return ExitCode::FAILURE;
        }
    };
    // stdout and stderr into one daily-rotated log.
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for stream in [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    let started = Instant::now();
    let deadline = opts.minutes.map(|m| started + Duration::from_secs(m * 60));
    let mut log = DailyLog::new(&opts.dir);
    let status = loop {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => log.write(unix_now(), &line),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break child.wait().ok(),
        }
        if let Ok(Some(st)) = child.try_wait() {
            // Drain what is left.
            while let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
                log.write(unix_now(), &line);
            }
            break Some(st);
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            term::step("time is up — stopping the app");
            terminate(&mut child);
            while let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) {
                log.write(unix_now(), &line);
            }
            break None;
        }
    };
    let timed_out = status.is_none();
    if let Some(st) = status
        && !st.success()
    {
        term::warn(&format!("the app exited with {st}"));
    }
    let report = summarize_dir(&opts.dir);
    print!("{}", report.text);
    if report.failed || !(timed_out || status.is_some_and(|s| s.success())) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// SIGTERM, then SIGKILL after 5 s.
fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill").args(["-TERM", &child.id().to_string()]).status();
        for _ in 0..50 {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// `app.log` in the day directory of each line's time: rotates at UTC midnight.
struct DailyLog {
    root: PathBuf,
    day: String,
    file: Option<std::fs::File>,
}

impl DailyLog {
    fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf(), day: String::new(), file: None }
    }

    fn write(&mut self, unix_secs: u64, line: &str) {
        let day = utc_date(unix_secs);
        if day != self.day || self.file.is_none() {
            let dir = self.root.join(&day);
            let _ = std::fs::create_dir_all(&dir);
            self.file = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("app.log")).ok();
            self.day = day;
        }
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{line}");
        }
    }
}

// --- summary ------------------------------------------------------------------------------------

/// One `PEBBLES_SOAK_CSV` row.
#[derive(Clone, Debug, PartialEq)]
struct Row {
    uptime_s: f64,
    rss_kib: Option<u64>,
    gpu_alloc: Option<u64>,
    /// Count columns (census, not bytes / sizes), by column name.
    counts: BTreeMap<String, u64>,
}

/// Columns judged by the §2.9 window rule: every census count (not bytes, not texture sides,
/// not capacities), plus the framework census.
fn is_count_column(name: &str) -> bool {
    const FIXED: [&str; 6] =
        ["unix_ms", "frame", "uptime_s", "rss_kib", "gpu_alloc_bytes", "gpu_reserved_bytes"];
    !FIXED.contains(&name)
        && !name.ends_with("_bytes")
        && !name.ends_with("_side")
        && !name.ends_with("_capacity")
}

/// Parse a soak CSV into runs (a run ends where `uptime_s` goes backwards: the CSV is appended
/// by every launch on the same day).
fn parse_csv(text: &str) -> Vec<Vec<Row>> {
    let mut lines = text.lines();
    let mut runs: Vec<Vec<Row>> = Vec::new();
    let mut header: Vec<String> = Vec::new();
    for line in lines.by_ref() {
        if line.starts_with("unix_ms,") {
            header = line.split(',').map(str::to_string).collect();
            continue;
        }
        if header.is_empty() || line.trim().is_empty() {
            continue;
        }
        let cells: Vec<&str> = line.split(',').collect();
        let get = |name: &str| header.iter().position(|h| h == name).and_then(|i| cells.get(i)).copied();
        let num = |name: &str| get(name).and_then(|v| v.parse::<u64>().ok());
        let Some(uptime_s) = get("uptime_s").and_then(|v| v.parse::<f64>().ok()) else { continue };
        let counts = header
            .iter()
            .enumerate()
            .filter(|(_, h)| is_count_column(h))
            .filter_map(|(i, h)| Some((h.clone(), cells.get(i)?.parse::<u64>().ok()?)))
            .collect();
        let row = Row { uptime_s, rss_kib: num("rss_kib"), gpu_alloc: num("gpu_alloc_bytes"), counts };
        match runs.last_mut() {
            Some(run) if run.last().is_some_and(|r| r.uptime_s <= uptime_s) => run.push(row),
            _ => runs.push(vec![row]),
        }
    }
    runs
}

/// The §2.9 verdict of one run.
#[derive(Debug, Default, PartialEq)]
struct RunVerdict {
    minutes: f64,
    /// Not judged (shorter than 30 min).
    too_short: bool,
    /// Count columns whose late-window max exceeds their early-window max.
    grown_counts: Vec<(String, u64, u64)>,
    /// (at 10 min, at end) and whether it breaks the ×1.02 rule.
    gpu: Option<(u64, u64, bool)>,
    /// (at 10 min, at end) and whether it breaks the ×1.05 rule (secondary).
    rss: Option<(u64, u64, bool)>,
}

impl RunVerdict {
    fn failed(&self) -> bool {
        !self.too_short && (!self.grown_counts.is_empty() || self.gpu.is_some_and(|g| g.2))
    }
}

/// The sample nearest to `t` seconds.
fn at(run: &[Row], t: f64) -> Option<&Row> {
    run.iter().min_by(|a, b| (a.uptime_s - t).abs().total_cmp(&(b.uptime_s - t).abs()))
}

/// §2.9 for one run: counts — max over minutes [10, 20) ≥ max over the last 10 minutes (for the
/// 60-minute soak, minutes 50–60); GPU allocator bytes at the end ≤ at 10 min × 1.02; RSS at
/// the end ≤ at 10 min × 1.05 (secondary: reported, judged with the census).
fn judge_run(run: &[Row]) -> RunVerdict {
    let end = run.last().map_or(0.0, |r| r.uptime_s);
    let mut v = RunVerdict { minutes: end / 60.0, ..RunVerdict::default() };
    if end < 30.0 * 60.0 {
        v.too_short = true;
        return v;
    }
    let window_max = |lo: f64, hi: f64, col: &str| {
        run.iter()
            .filter(|r| r.uptime_s >= lo && r.uptime_s < hi)
            .filter_map(|r| r.counts.get(col))
            .max()
            .copied()
    };
    let cols: Vec<String> = run
        .iter()
        .flat_map(|r| r.counts.keys().cloned())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    for col in cols {
        if let (Some(early), Some(late)) =
            (window_max(600.0, 1200.0, &col), window_max(end - 600.0, end + 1.0, &col))
            && late > early
        {
            v.grown_counts.push((col, early, late));
        }
    }
    let ten = at(run, 600.0);
    let last = run.last();
    if let (Some(a), Some(b)) = (ten.and_then(|r| r.gpu_alloc), last.and_then(|r| r.gpu_alloc)) {
        v.gpu = Some((a, b, (b as f64) > (a as f64) * 1.02));
    }
    if let (Some(a), Some(b)) = (ten.and_then(|r| r.rss_kib), last.and_then(|r| r.rss_kib)) {
        v.rss = Some((a, b, (b as f64) > (a as f64) * 1.05));
    }
    v
}

/// Frame times of one day from its `[pebbles run]` lines: the median of the per-second p95 UI
/// frame times and the worst present gap.
#[derive(Debug, Default, PartialEq)]
struct FrameTimes {
    seconds: usize,
    ui_p95_median: f64,
    gap_max: f64,
}

fn parse_frame_times(log: &str) -> FrameTimes {
    let field = |line: &str, key: &str| -> Option<(f64, f64)> {
        let rest = &line[line.find(key)? + key.len()..];
        let tok = rest.split_whitespace().next()?;
        let (a, b) = tok.split_once('/')?;
        Some((a.parse().ok()?, b.parse().ok()?))
    };
    let mut p95s = Vec::new();
    let mut gap_max = 0.0f64;
    for line in log.lines().filter(|l| l.contains("[pebbles run]")) {
        if let Some((_, p95)) = field(line, " ui_ms=") {
            p95s.push(p95);
        }
        if let Some((max, _)) = field(line, " gap_ms=") {
            gap_max = gap_max.max(max);
        }
    }
    p95s.sort_by(f64::total_cmp);
    let ui_p95_median = p95s.get(p95s.len() / 2).copied().unwrap_or(0.0);
    FrameTimes { seconds: p95s.len(), ui_p95_median, gap_max }
}

/// One day of the soak.
#[derive(Debug, Default)]
struct Day {
    date: String,
    runs: Vec<RunVerdict>,
    rss_max_kib: Option<u64>,
    gpu_max: Option<u64>,
    frames: FrameTimes,
    panics: usize,
    gpu_errors: usize,
}

struct Report {
    text: String,
    failed: bool,
}

fn load_day(dir: &Path, date: &str) -> Day {
    let csv = std::fs::read_to_string(dir.join("soak.csv")).unwrap_or_default();
    let log = std::fs::read_to_string(dir.join("app.log")).unwrap_or_default();
    day_from(date, &csv, &log)
}

fn day_from(date: &str, csv: &str, log: &str) -> Day {
    let runs = parse_csv(csv);
    let rss_max_kib = runs.iter().flatten().filter_map(|r| r.rss_kib).max();
    let gpu_max = runs.iter().flatten().filter_map(|r| r.gpu_alloc).max();
    Day {
        date: date.to_string(),
        runs: runs.iter().map(|r| judge_run(r)).collect(),
        rss_max_kib,
        gpu_max,
        frames: parse_frame_times(log),
        panics: log.lines().filter(|l| l.contains("panicked")).count(),
        gpu_errors: log
            .lines()
            .filter(|l| l.contains("wgpu validation error") || l.contains("PEBBLES_GPU_STRICT"))
            .count(),
    }
}

fn summarize_dir(root: &Path) -> Report {
    let mut dates: Vec<String> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.len() == 10 && n.as_bytes()[4] == b'-')
        .collect();
    dates.sort();
    let days: Vec<Day> = dates.iter().map(|d| load_day(&root.join(d), d)).collect();
    let mut r = summarize(&days);
    r.text = format!("soak directory: {}\n{}", root.display(), r.text);
    r
}

/// The report over all days. Fails on a §2.9 failure, a panic or a strict-mode GPU error;
/// flags (warns about) day-over-day growth: peak RSS or GPU bytes up > 10 % from the first
/// to the last day, or the median p95 UI frame time up > 50 % (and by ≥ 2 ms).
fn summarize(days: &[Day]) -> Report {
    use std::fmt::Write as _;
    let mut t = String::new();
    let mut failed = false;
    let mb = |k: Option<u64>| k.map_or("-".to_string(), |k| format!("{:.1}", k as f64 / 1024.0));
    let mib = |b: Option<u64>| b.map_or("-".to_string(), |b| format!("{:.1}", b as f64 / 1048576.0));
    let _ = writeln!(t, "{} day(s)", days.len());
    let _ = writeln!(
        t,
        "date        runs  judged  rss_max_MB  gpu_max_MiB  ui_p95_med_ms  gap_max_ms  panics  verdict"
    );
    for d in days {
        let judged = d.runs.iter().filter(|r| !r.too_short).count();
        let bad = d.runs.iter().any(RunVerdict::failed) || d.panics > 0 || d.gpu_errors > 0;
        failed |= bad;
        let _ = writeln!(
            t,
            "{}  {:>4}  {:>6}  {:>10}  {:>11}  {:>13.2}  {:>10.1}  {:>6}  {}",
            d.date,
            d.runs.len(),
            judged,
            mb(d.rss_max_kib),
            mib(d.gpu_max),
            d.frames.ui_p95_median,
            d.frames.gap_max,
            d.panics,
            if bad { "FAIL" } else { "ok" }
        );
        for (i, run) in d.runs.iter().enumerate().filter(|(_, r)| !r.too_short) {
            for (col, early, late) in &run.grown_counts {
                let _ = writeln!(
                    t,
                    "  FAIL run {i} ({:.0} min): {col} grew {early} → {late} (§2.9 window rule)",
                    run.minutes
                );
            }
            if let Some((a, b, true)) = run.gpu {
                let _ = writeln!(t, "  FAIL run {i}: GPU allocator bytes {a} → {b} (> ×1.02 from minute 10)");
            }
            if let Some((a, b, true)) = run.rss {
                let _ = writeln!(
                    t,
                    "  WARN run {i}: RSS {a} → {b} KiB (> ×1.05; census decides — profile before calling it a leak)"
                );
            }
        }
        if d.gpu_errors > 0 {
            let _ = writeln!(t, "  FAIL: {} GPU validation/strict error line(s) in app.log", d.gpu_errors);
        }
    }
    let mut flags = Vec::new();
    if let (Some(first), Some(last)) = (days.first(), days.last())
        && days.len() >= 2
    {
        let grew = |a: Option<u64>, b: Option<u64>| matches!((a, b), (Some(a), Some(b)) if b as f64 > a as f64 * 1.10);
        if grew(first.rss_max_kib, last.rss_max_kib) {
            flags.push(format!(
                "peak RSS grew {} → {} MB from {} to {}",
                mb(first.rss_max_kib),
                mb(last.rss_max_kib),
                first.date,
                last.date
            ));
        }
        if grew(first.gpu_max, last.gpu_max) {
            flags.push(format!("peak GPU bytes grew {} → {} MiB", mib(first.gpu_max), mib(last.gpu_max)));
        }
        let (a, b) = (first.frames.ui_p95_median, last.frames.ui_p95_median);
        if a > 0.0 && b > a * 1.5 && b - a >= 2.0 {
            flags.push(format!("median p95 UI frame time grew {a:.2} → {b:.2} ms"));
        }
    }
    for f in &flags {
        let _ = writeln!(t, "GROWTH: {f}");
    }
    let span = days.len();
    let _ = writeln!(
        t,
        "verdict: {}{}",
        if failed { "FAIL" } else { "PASS" },
        if span >= 14 { format!(" ({span} days ≥ 14)") } else { format!(" ({span} of 14 days so far)") }
    );
    Report { text: t, failed }
}

/// A systemd `--user` service + timer pair (printed, never installed).
fn systemd_templates(opts: &RunOpts) -> String {
    let exe = std::env::current_exe().map_or_else(|_| "pebbles".to_string(), |p| p.display().to_string());
    let bin = opts.bin.display();
    let dir = opts.dir.display();
    format!(
        "\
# ~/.config/systemd/user/pebbles-soak.service
# The daily-use soak app: starts with the graphical session; run the app as usual all day.
[Unit]
Description=Pebbles daily-use soak ({bin})
PartOf=graphical-session.target
After=graphical-session.target

[Service]
Type=simple
ExecStart={exe} soak run --bin {bin} --dir {dir}
Restart=no

[Install]
WantedBy=graphical-session.target

# ~/.config/systemd/user/pebbles-soak-summary.service
[Unit]
Description=Pebbles soak summary

[Service]
Type=oneshot
ExecStart=/bin/sh -c '{exe} soak summary {dir} > {dir}/summary.txt 2>&1 || true'

# ~/.config/systemd/user/pebbles-soak-summary.timer
[Unit]
Description=Daily Pebbles soak summary

[Timer]
OnCalendar=daily
Persistent=true

[Install]
WantedBy=timers.target

# Enable (after saving the three files above):
#   systemctl --user daemon-reload
#   systemctl --user enable --now pebbles-soak.service pebbles-soak-summary.timer
# Read the trend any time:  {exe} soak summary {dir}
"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    const HEADER: &str = "unix_ms,frame,uptime_s,rss_kib,gpu_alloc_bytes,gpu_reserved_bytes,elements,render_nodes,signals,subscriptions,glyph_entries,glyph_mask_side,image_bytes";

    /// A 60-minute run, one row per minute; `f(min)` gives (rss, gpu, elements, glyph_entries).
    fn csv(minutes: u32, f: impl Fn(u32) -> (u64, u64, u64, u64)) -> String {
        let mut t = format!("{HEADER}\n");
        for m in 0..=minutes {
            let (rss, gpu, el, gl) = f(m);
            t += &format!("0,{},{}.0,{rss},{gpu},0,{el},10,5,5,{gl},1024,999999\n", m * 600, m * 60);
        }
        t
    }

    #[test]
    fn parse_args() {
        let o = parse_run_args(&s(&["--bin", "app", "--tour", "--dir", "/tmp/x", "--", "-v"])).unwrap();
        assert_eq!(o.minutes, Some(60), "the tour is the 60-minute soak");
        assert!(o.tour && !o.strict);
        assert_eq!(o.app_args, s(&["-v"]));
        assert_eq!(o.dir, PathBuf::from("/tmp/x"));
        assert!(parse_run_args(&s(&["--minutes", "5"])).is_err(), "--bin is required");
        assert!(parse_run_args(&s(&["--bin", "a", "--minutes", "x"])).is_err());
        let env = soak_env(&o, Path::new("/tmp/x/soak.csv"));
        for k in [
            "PEBBLES_SOAK_CSV",
            "PEBBLES_FRAME_STATS",
            "GALLERY_TOUR",
            "PEBBLES_INPUT_STORM",
            "PEBBLES_GPU_STRICT",
        ] {
            assert!(env.iter().any(|(e, _)| e == k), "{k}");
        }
    }

    #[test]
    fn utc_dates() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400), "2000-02-29");
        assert_eq!(utc_date(1_793_318_399), "2026-10-29");
        assert_eq!(utc_date(1_793_318_400), "2026-10-30", "rotates at UTC midnight");
    }

    #[test]
    fn daily_log_rotates_at_midnight() {
        let root = std::env::temp_dir().join(format!("pebbles-soak-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut log = DailyLog::new(&root);
        log.write(1_793_318_399, "before midnight");
        log.write(1_793_318_400, "after midnight");
        let a = std::fs::read_to_string(root.join("2026-10-29/app.log")).unwrap();
        let b = std::fs::read_to_string(root.join("2026-10-30/app.log")).unwrap();
        assert_eq!((a.trim(), b.trim()), ("before midnight", "after midnight"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn flat_run_passes() {
        let runs = parse_csv(&csv(60, |m| {
            (200_000 + u64::from(m % 3) * 100, 50_000_000, 400 + u64::from(m % 5), 300)
        }));
        assert_eq!(runs.len(), 1);
        let v = judge_run(&runs[0]);
        assert!(!v.too_short && !v.failed(), "{v:?}");
        assert_eq!(v.gpu, Some((50_000_000, 50_000_000, false)));
    }

    #[test]
    fn growing_census_fails() {
        // Elements leak 2 per minute: the late window max exceeds the early one.
        let runs = parse_csv(&csv(60, |m| (200_000, 50_000_000, 400 + u64::from(m) * 2, 300)));
        let v = judge_run(&runs[0]);
        assert!(v.failed());
        assert_eq!(v.grown_counts[0].0, "elements");
        // Sides and bytes are not count columns.
        assert!(v.grown_counts.iter().all(|(c, ..)| c != "glyph_mask_side" && c != "image_bytes"));
    }

    #[test]
    fn gpu_growth_fails_and_rss_only_warns() {
        let runs = parse_csv(&csv(60, |m| {
            (200_000 + u64::from(m) * 1000, 50_000_000 + u64::from(m) * 100_000, 400, 300)
        }));
        let v = judge_run(&runs[0]);
        assert!(v.gpu.unwrap().2, "GPU bytes +10 % > ×1.02");
        assert!(v.rss.unwrap().2, "RSS +25 % > ×1.05");
        let runs = parse_csv(&csv(60, |m| (200_000 + u64::from(m) * 1000, 50_000_000, 400, 300)));
        let v = judge_run(&runs[0]);
        assert!(v.rss.unwrap().2 && !v.failed(), "RSS alone never fails (§2.9: census decides)");
    }

    #[test]
    fn short_runs_and_relaunches() {
        let mut text = csv(20, |_| (1, 1, 1, 1));
        // A relaunch appends to the same CSV (no second header): uptime restarts.
        text += &csv(45, |_| (1, 1, 1, 1)).lines().skip(1).map(|l| format!("{l}\n")).collect::<String>();
        let runs = parse_csv(&text);
        assert_eq!(runs.len(), 2);
        assert!(judge_run(&runs[0]).too_short);
        assert!(!judge_run(&runs[1]).too_short);
    }

    #[test]
    fn frame_times_from_run_lines() {
        let log = "\
[pebbles run] t=1.0s mode=inline windows=1 size=800x600 pos=0,0 scale=1.00 presented=+60/60 secondary=+0/0 unpresented=+0/0 occluded=0/0 resize_sync_timeouts=0 gen=0 gap_ms=20.0/17.0 ui_ms=4.0/2.0 input_ms=0.0 configures=1 not_presented=-
noise
[pebbles run] t=2.0s mode=inline windows=1 size=800x600 pos=0,0 scale=1.00 presented=+60/120 secondary=+0/0 unpresented=+0/0 occluded=0/0 resize_sync_timeouts=0 gen=0 gap_ms=40.0/17.0 ui_ms=9.0/3.0 input_ms=0.0 configures=1 not_presented=-
[pebbles run] t=3.0s mode=inline windows=1 size=800x600 pos=0,0 scale=1.00 presented=+60/180 secondary=+0/0 unpresented=+0/0 occluded=0/0 resize_sync_timeouts=0 gen=0 gap_ms=18.0/17.0 ui_ms=5.0/4.0 input_ms=0.0 configures=1 not_presented=-
";
        assert_eq!(parse_frame_times(log), FrameTimes { seconds: 3, ui_p95_median: 3.0, gap_max: 40.0 });
    }

    #[test]
    fn report_flags_growth_and_failures() {
        let flat = csv(60, |_| (200_000, 50_000_000, 400, 300));
        let fast = "[pebbles run] gap_ms=17.0/17.0 ui_ms=3.0/2.0\n".repeat(10);
        let slow = "[pebbles run] gap_ms=17.0/17.0 ui_ms=9.0/6.0\n".repeat(10);
        let d1 = day_from("2026-10-01", &flat, &fast);
        let d14 = day_from("2026-10-14", &csv(60, |_| (260_000, 50_000_000, 400, 300)), &slow);
        let r = summarize(&[d1, d14]);
        assert!(!r.failed, "{}", r.text);
        assert!(r.text.contains("GROWTH: peak RSS grew"), "{}", r.text);
        assert!(r.text.contains("GROWTH: median p95 UI frame time grew 2.00 → 6.00 ms"), "{}", r.text);
        assert!(r.text.contains("2 of 14 days so far"));

        let panicked = day_from("2026-10-02", &flat, "thread 'main' panicked at x\n");
        let r = summarize(&[panicked]);
        assert!(r.failed && r.text.contains("FAIL"));
        let leak = day_from("2026-10-03", &csv(60, |m| (1, 1, 400 + u64::from(m), 1)), "");
        let r = summarize(&[leak]);
        assert!(r.failed && r.text.contains("elements grew"), "{}", r.text);
    }

    #[test]
    fn systemd_template_mentions_both_units() {
        let o = parse_run_args(&s(&["--bin", "/opt/app", "--dir", "/tmp/soak"])).unwrap();
        let t = systemd_templates(&o);
        assert!(t.contains("soak run --bin /opt/app --dir /tmp/soak"));
        assert!(t.contains("OnCalendar=daily") && t.contains("graphical-session.target"));
    }
}
