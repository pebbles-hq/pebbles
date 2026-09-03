//! `pebbles run` — the dev runner: build, launch with dev diagnostics on, stream
//! prettified logs, and hot-restart the app whenever a source file is saved.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::term;

struct Opts {
    release: bool,
    reload: bool,
    log: String,
    app_args: Vec<String>,
}

pub fn run(args: &[String]) -> ExitCode {
    let mut o = Opts { release: false, reload: true, log: "debug".into(), app_args: vec![] };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--release" => o.release = true,
            "--no-reload" => o.reload = false,
            "-q" | "--quiet" => o.log = "warn".into(),
            "--log" => {
                if let Some(l) = it.next() {
                    o.log = l.clone();
                }
            }
            "--" => {
                o.app_args = it.cloned().collect();
                break;
            }
            s if s.starts_with('-') => {
                term::error(&format!("unknown option `{s}` for `pebbles run`"));
                return ExitCode::FAILURE;
            }
            s => o.app_args.push(s.to_string()),
        }
    }

    // Locate the project (walk up for Cargo.toml).
    let Some(project) = find_project(&std::env::current_dir().unwrap_or_default()) else {
        term::error("no Cargo.toml found here — run this inside a Pebbles project (or `pebbles new <name>`)");
        return ExitCode::FAILURE;
    };
    let Some(bin) = package_name(&project) else {
        term::error("could not read the package name from Cargo.toml");
        return ExitCode::FAILURE;
    };
    let watch_dirs = read_watch_dirs(&project);

    term::banner(&format!(
        "pebbles run — {bin} ({}, {})",
        if o.release { "release" } else { "dev" },
        if o.reload { "hot-restart on" } else { "hot-restart off" }
    ));

    // Ctrl+C: flip a flag; the child shares our process group so it also gets the
    // signal, but we set the flag so our loop exits cleanly too.
    let running = Arc::new(AtomicBool::new(true));
    install_sigint(running.clone());

    let bin_path = target_bin(&project, &bin, o.release);

    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }
        // 1. Build.
        let t = Instant::now();
        term::step("building…");
        if !cargo_build(&project, o.release) {
            term::error("build failed — fix the errors above; waiting for a change…");
            if !o.reload || !wait_for_change(&project, &watch_dirs, &running) {
                return ExitCode::FAILURE;
            }
            continue;
        }
        term::ok(&format!("built in {:.1}s", t.elapsed().as_secs_f64()));

        // 2. Launch with dev diagnostics on.
        let mut child = match spawn_app(&bin_path, &o) {
            Ok(c) => c,
            Err(e) => {
                term::error(&format!("could not launch {}: {e}", bin_path.display()));
                return ExitCode::FAILURE;
            }
        };
        term::hot(&format!("running {bin} (pid {})", child.id()));
        let readers = pipe_output(&mut child);

        // 3. Watch for a source change or the app exiting.
        let outcome = if o.reload {
            watch_until_change_or_exit(&project, &watch_dirs, &mut child, &running)
        } else {
            let status = child.wait();
            drop(status);
            Outcome::Exited
        };
        for r in readers {
            let _ = r.join();
        }

        match outcome {
            Outcome::Interrupted => break,
            Outcome::Exited if !o.reload => break,
            Outcome::Exited => {
                term::warn("app exited — waiting for a change to restart…");
                if !wait_for_change(&project, &watch_dirs, &running) {
                    break;
                }
            }
            Outcome::Changed(file) => {
                let _ = child.kill();
                let _ = child.wait();
                term::hot(&format!("changed {} — restarting", short(&project, &file)));
            }
        }
    }

    term::ok("stopped.");
    ExitCode::SUCCESS
}

enum Outcome {
    Changed(PathBuf),
    Exited,
    Interrupted,
}

fn cargo_build(project: &Path, release: bool) -> bool {
    let mut cmd = Command::new("cargo");
    cmd.arg("build").current_dir(project);
    if release {
        cmd.arg("--release");
    }
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    matches!(cmd.status(), Ok(s) if s.success())
}

fn spawn_app(bin: &Path, o: &Opts) -> std::io::Result<Child> {
    let mut cmd = Command::new(bin);
    cmd.args(&o.app_args)
        .env("PEBBLES_DEV", "1")
        .env("PEBBLES_LOG", &o.log)
        .env("RUST_BACKTRACE", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn()
}

/// Spawn reader threads that prettify the app's stdout+stderr.
fn pipe_output(child: &mut Child) -> Vec<thread::JoinHandle<()>> {
    let mut handles = Vec::new();
    if let Some(out) = child.stdout.take() {
        handles.push(thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                term::app_line(&line);
            }
        }));
    }
    if let Some(err) = child.stderr.take() {
        handles.push(thread::spawn(move || {
            for line in BufReader::new(err).lines().map_while(Result::ok) {
                term::app_line(&line);
            }
        }));
    }
    handles
}

/// Poll for a source change while the app runs; also notice the app exiting.
fn watch_until_change_or_exit(
    project: &Path,
    dirs: &[PathBuf],
    child: &mut Child,
    running: &AtomicBool,
) -> Outcome {
    let mut snapshot = scan(project, dirs);
    loop {
        if !running.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Outcome::Interrupted;
        }
        // Did the app exit on its own?
        match child.try_wait() {
            Ok(Some(_)) => return Outcome::Exited,
            Ok(None) => {}
            Err(_) => return Outcome::Exited,
        }
        thread::sleep(Duration::from_millis(200));
        let next = scan(project, dirs);
        if let Some(changed) = first_change(&snapshot, &next) {
            // Debounce: editors write in bursts — let it settle.
            thread::sleep(Duration::from_millis(120));
            return Outcome::Changed(changed);
        }
        snapshot = next;
    }
}

/// Block until a source file changes (used after a failed build / app exit).
/// Returns false if interrupted.
fn wait_for_change(project: &Path, dirs: &[PathBuf], running: &AtomicBool) -> bool {
    let mut snapshot = scan(project, dirs);
    loop {
        if !running.load(Ordering::Relaxed) {
            return false;
        }
        thread::sleep(Duration::from_millis(200));
        let next = scan(project, dirs);
        if first_change(&snapshot, &next).is_some() {
            thread::sleep(Duration::from_millis(120));
            return true;
        }
        snapshot = next;
    }
}

/// mtime snapshot of every `.rs`/`.toml` under the watched dirs (+ the manifest).
fn scan(project: &Path, dirs: &[PathBuf]) -> BTreeMap<PathBuf, SystemTime> {
    let mut map = BTreeMap::new();
    for f in ["Cargo.toml", "pebbles.toml"] {
        let p = project.join(f);
        if let Ok(m) = p.metadata().and_then(|m| m.modified()) {
            map.insert(p, m);
        }
    }
    for dir in dirs {
        walk(dir, &mut map);
    }
    map
}

fn walk(dir: &Path, map: &mut BTreeMap<PathBuf, SystemTime>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            // Skip build output.
            if path.file_name().is_some_and(|n| n == "target" || n == ".git") {
                continue;
            }
            walk(&path, map);
        } else if path.extension().is_some_and(|e| e == "rs" || e == "toml")
            && let Ok(m) = entry.metadata().and_then(|m| m.modified())
        {
            map.insert(path, m);
        }
    }
}

fn first_change(
    old: &BTreeMap<PathBuf, SystemTime>,
    new: &BTreeMap<PathBuf, SystemTime>,
) -> Option<PathBuf> {
    for (p, m) in new {
        match old.get(p) {
            Some(prev) if prev == m => {}
            _ => return Some(p.clone()),
        }
    }
    // A deletion also counts.
    old.keys().find(|p| !new.contains_key(*p)).cloned()
}

// --- project introspection ---------------------------------------------------

fn find_project(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        if d.join("Cargo.toml").is_file() {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

/// The `name = "..."` under `[package]` in Cargo.toml (naive but dependency-free).
fn package_name(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join("Cargo.toml")).ok()?;
    let mut in_package = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_package = t == "[package]";
            continue;
        }
        if in_package && t.starts_with("name")
            && let Some(v) = t.split('=').nth(1)
        {
            return Some(v.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn read_watch_dirs(project: &Path) -> Vec<PathBuf> {
    // pebbles.toml `[dev] watch = [...]`, else default to `src`.
    if let Ok(text) = std::fs::read_to_string(project.join("pebbles.toml"))
        && let Some(line) = text.lines().find(|l| l.trim_start().starts_with("watch"))
        && let Some(rhs) = line.split('=').nth(1)
    {
        let dirs: Vec<PathBuf> = rhs
            .trim()
            .trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| project.join(s.trim().trim_matches('"')))
            .filter(|p| !p.as_os_str().is_empty() && p.exists())
            .collect();
        if !dirs.is_empty() {
            return dirs;
        }
    }
    vec![project.join("src")]
}

fn target_bin(project: &Path, bin: &str, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    project.join("target").join(profile).join(bin)
}

fn short(project: &Path, file: &Path) -> String {
    file.strip_prefix(project).unwrap_or(file).display().to_string()
}

// --- Ctrl+C ------------------------------------------------------------------

/// Ctrl+C handling is delegated to the OS: the CLI and the app it spawns share a
/// process group, so a terminal SIGINT reaches both — the app terminates and the
/// CLI does too. Staying dependency-free means no custom SIGINT handler; the
/// `running` flag is kept for structure and future signal wiring. Closing the app
/// window is detected by the watch loop as a clean `Exited`.
fn install_sigint(flag: Arc<AtomicBool>) {
    let _ = flag;
}
