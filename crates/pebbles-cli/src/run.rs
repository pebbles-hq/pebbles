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
    package: Option<String>,
    extra_watch: Vec<String>,
    app_args: Vec<String>,
}

pub fn run(args: &[String]) -> ExitCode {
    let mut o = Opts {
        release: false,
        reload: true,
        log: "debug".into(),
        package: None,
        extra_watch: vec![],
        app_args: vec![],
    };
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
            // Select a workspace member (or example) by name — run it from anywhere.
            "-p" | "--package" | "--example" | "--bin" => {
                if let Some(name) = it.next() {
                    o.package = Some(name.clone());
                }
            }
            // Watch an extra directory for hot-restart (repeatable) — e.g. the
            // framework crates when iterating on Pebbles while running a sample.
            "--watch" => {
                if let Some(d) = it.next() {
                    o.extra_watch.push(d.clone());
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

    let cwd = std::env::current_dir().unwrap_or_default();
    // The workspace root (nearest ancestor Cargo.toml with `[workspace]`), if any —
    // its `target/` is where a member's binary actually lands.
    let workspace_root = find_workspace_root(&cwd);

    // Resolve which package to run and its source directory.
    let (member_dir, bin) = match &o.package {
        Some(name) => {
            let search_root = workspace_root.clone().unwrap_or_else(|| cwd.clone());
            match find_member(&search_root, name) {
                Some(dir) => (dir, name.clone()),
                None => {
                    term::error(&format!("no package `{name}` in this workspace"));
                    list_members(&search_root);
                    return ExitCode::FAILURE;
                }
            }
        }
        None => {
            let Some(project) = find_project(&cwd) else {
                term::error(
                    "no Cargo.toml found here — run this inside a Pebbles project (or `pebbles new <name>`)",
                );
                return ExitCode::FAILURE;
            };
            match package_name(&project) {
                Some(name) => (project, name),
                None => {
                    // A workspace root (no [package]) — the user must pick a member.
                    term::error("this is a Cargo workspace — choose a package to run:");
                    list_members(&project);
                    println!("  e.g. pebbles run -p gallery");
                    return ExitCode::FAILURE;
                }
            }
        }
    };

    // Where the binary lands: the workspace target if in a workspace, else the
    // package's own target (a standalone `pebbles new` project).
    let target_root = workspace_root.clone().unwrap_or_else(|| member_dir.clone());

    let mut watch_dirs = read_watch_dirs(&member_dir);
    for w in &o.extra_watch {
        let p = target_root.join(w);
        if p.exists() {
            watch_dirs.push(p);
        } else if Path::new(w).exists() {
            watch_dirs.push(PathBuf::from(w));
        }
    }

    term::banner(&format!(
        "pebbles run — {bin} ({}, {})",
        if o.release { "release" } else { "dev" },
        if o.reload { "hot-restart on" } else { "hot-restart off" }
    ));

    // Ctrl+C: flip a flag; the child shares our process group so it also gets the
    // signal, but we set the flag so our loop exits cleanly too.
    let running = Arc::new(AtomicBool::new(true));
    install_sigint(running.clone());

    let bin_path = target_bin(&target_root, &bin, o.release);

    loop {
        if !running.load(Ordering::Relaxed) {
            break;
        }
        // 1. Build.
        let t = Instant::now();
        term::step("building…");
        if !cargo_build(&target_root, &bin, o.release) {
            term::error("build failed — fix the errors above; waiting for a change…");
            if !o.reload || !wait_for_change(&member_dir, &watch_dirs, &running) {
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
            watch_until_change_or_exit(&member_dir, &watch_dirs, &mut child, &running)
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
                if !wait_for_change(&member_dir, &watch_dirs, &running) {
                    break;
                }
            }
            Outcome::Changed(file) => {
                let _ = child.kill();
                let _ = child.wait();
                term::hot(&format!("changed {} — restarting", short(&member_dir, &file)));
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

fn cargo_build(run_dir: &Path, package: &str, release: bool) -> bool {
    // `-p <package>` works whether `run_dir` is a workspace root or a standalone
    // package, so a sample builds the same way as a scaffolded app.
    let mut cmd = Command::new("cargo");
    cmd.arg("build").arg("-p").arg(package).current_dir(run_dir);
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

/// The `name = "..."` under `[package]` in a Cargo.toml file (naive, dep-free).
fn package_name(project: &Path) -> Option<String> {
    let text = std::fs::read_to_string(project.join("Cargo.toml")).ok()?;
    package_name_from(&text)
}

/// The `[package] name` from Cargo.toml text (`None` for a `[workspace]`-only root).
fn package_name_from(text: &str) -> Option<String> {
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

fn target_bin(target_root: &Path, bin: &str, release: bool) -> PathBuf {
    let profile = if release { "release" } else { "debug" };
    target_root.join("target").join(profile).join(bin)
}

/// The nearest ancestor whose Cargo.toml declares `[workspace]` — a member's
/// binary is built into THAT directory's `target/`, not the member's own.
fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start);
    while let Some(d) = dir {
        let manifest = d.join("Cargo.toml");
        if manifest.is_file()
            && std::fs::read_to_string(&manifest).is_ok_and(|t| has_workspace_table(&t))
        {
            return Some(d.to_path_buf());
        }
        dir = d.parent();
    }
    None
}

fn has_workspace_table(toml: &str) -> bool {
    toml.lines().any(|l| l.trim() == "[workspace]")
}

/// Find a workspace member by package name — walks the tree under `root` for a
/// Cargo.toml whose `[package] name` matches. Returns the member's directory.
fn find_member(root: &Path, name: &str) -> Option<PathBuf> {
    let mut found = None;
    walk_manifests(root, &mut |dir, pkg| {
        if pkg == name {
            found = Some(dir.to_path_buf());
        }
    });
    found
}

/// Print every runnable package under `root` (for error hints).
fn list_members(root: &Path) {
    let mut names = Vec::new();
    walk_manifests(root, &mut |_dir, pkg| names.push(pkg.to_string()));
    names.sort();
    names.dedup();
    if names.is_empty() {
        return;
    }
    println!("  packages: {}", names.join(", "));
}

/// Visit every `Cargo.toml` with a `[package] name` under `root` (skips target/.git).
fn walk_manifests(root: &Path, f: &mut dyn FnMut(&Path, &str)) {
    fn go(dir: &Path, f: &mut dyn FnMut(&Path, &str), depth: usize) {
        if depth > 4 {
            return; // members live shallow (crates/*, examples/*)
        }
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && let Ok(text) = std::fs::read_to_string(&manifest)
            && let Some(name) = package_name_from(&text)
        {
            f(dir, &name);
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir())
                && !path.file_name().is_some_and(|n| n == "target" || n == ".git")
            {
                go(&path, f, depth + 1);
            }
        }
    }
    go(root, f, 0);
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
