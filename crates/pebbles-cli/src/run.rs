//! `pebbles run` — the dev runner: build, launch with dev diagnostics on, stream
//! prettified logs, and hot-restart the app whenever a source file is saved.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::term;

struct Opts {
    release: bool,
    reload: bool,
    log: String,
    log_file: Option<String>,
    package: Option<String>,
    extra_watch: Vec<String>,
    app_args: Vec<String>,
    platform: Platform,
    port: u16,
}

/// The target Pebbles runs on — Flutter's `-d`. Default is the host desktop.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Platform {
    /// The host OS (Linux / macOS / Windows) — a native window. Default.
    Desktop,
    /// A WebGPU browser (Chrome / Edge / Safari / Firefox), served by Trunk.
    Web,
    Android,
    Ios,
}

impl Platform {
    /// Map a `-d` value to a platform. Accepts Flutter-ish synonyms (`chrome` = web,
    /// `linux`/`macos`/`windows` = desktop). Returns `None` for an unknown value.
    fn parse(s: &str) -> Option<Platform> {
        match s.to_ascii_lowercase().as_str() {
            "desktop" | "native" | "host" | "linux" | "macos" | "mac" | "osx" | "windows" | "win" => {
                Some(Platform::Desktop)
            }
            "web" | "chrome" | "safari" | "firefox" | "browser" | "wasm" => Some(Platform::Web),
            "android" => Some(Platform::Android),
            "ios" | "iphone" | "ipad" => Some(Platform::Ios),
            _ => None,
        }
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let mut o = Opts {
        release: false,
        reload: true,
        log: "debug".into(),
        log_file: None,
        package: None,
        extra_watch: vec![],
        app_args: vec![],
        platform: Platform::Desktop,
        port: 8080,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--release" => o.release = true,
            "--no-reload" => o.reload = false,
            // Flutter-style target selection: `pebbles run -d web` (default: desktop).
            "-d" | "--device" | "--platform" => {
                let Some(v) = it.next() else {
                    term::error("`-d` needs a target: desktop | web | android | ios");
                    return ExitCode::FAILURE;
                };
                match Platform::parse(v) {
                    Some(p) => o.platform = p,
                    None => {
                        term::error(&format!(
                            "unknown target `{v}` — use one of: desktop | web | android | ios"
                        ));
                        return ExitCode::FAILURE;
                    }
                }
            }
            // The port for the web dev server (`-d web`).
            "--port" => {
                if let Some(v) = it.next()
                    && let Ok(p) = v.parse::<u16>()
                {
                    o.port = p;
                }
            }
            "-q" | "--quiet" => o.log = "warn".into(),
            "--log" => {
                if let Some(l) = it.next() {
                    o.log = l.clone();
                }
            }
            // Write the full log to a file too. `--log-file` alone picks a default
            // path under the OS temp dir; `--log-file <path>` uses that path.
            "--log-file" => {
                let next = it.clone().next();
                if let Some(p) = next.filter(|p| !p.starts_with('-')) {
                    o.log_file = Some(p.clone());
                    it.next();
                } else {
                    o.log_file = Some(String::new()); // default path, filled below
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

    // Flutter-style target dispatch. Desktop falls through to the native
    // build+watch+run loop below; the others have their own runtimes.
    match o.platform {
        Platform::Desktop => {}
        Platform::Web => return run_web(&member_dir, &bin, &o),
        Platform::Android => return run_android(&member_dir, &bin, &o),
        Platform::Ios => return run_ios(),
    }

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

    // Resolve the log-file path (default: <tempdir>/pebbles-<bin>.log).
    if let Some(p) = &o.log_file
        && p.is_empty()
    {
        o.log_file = Some(std::env::temp_dir().join(format!("pebbles-{bin}.log")).display().to_string());
    }

    term::banner(&format!(
        "pebbles run — {bin} ({}, {})",
        if o.release { "release" } else { "dev" },
        if o.reload { "hot-restart on" } else { "hot-restart off" }
    ));
    if let Some(p) = &o.log_file {
        term::step(&format!("full log → {p}"));
    }

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

// --- web (`-d web`) ----------------------------------------------------------

/// Build the app to WebAssembly and serve it in a WebGPU browser, with live
/// rebuild-on-save. Delegates the wasm toolchain to **Trunk** (the standard
/// bundler for winit/wgpu web apps): it runs `cargo build --target
/// wasm32-unknown-unknown`, wasm-bindgen, serves over HTTP, and reloads the tab
/// on a source change — so Pebbles doesn't reimplement any of that.
///
/// Pebbles targets **WebGPU** browsers (Chrome/Edge, Safari, Firefox) — there is
/// no WebGL2 fallback, by design (Vello is a compute-shader renderer).
fn run_web(member_dir: &Path, bin: &str, o: &Opts) -> ExitCode {
    term::banner(&format!("pebbles run — {bin} (web, {})", if o.release { "release" } else { "dev" }));

    // 1. The wasm target must be installed (`cargo build --target wasm32…`).
    if !rustup_target_installed("wasm32-unknown-unknown") {
        term::step("installing the wasm32-unknown-unknown target…");
        let ok = Command::new("rustup")
            .args(["target", "add", "wasm32-unknown-unknown"])
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            term::error("could not add the wasm target — run: rustup target add wasm32-unknown-unknown");
            return ExitCode::FAILURE;
        }
    }

    // 2. Trunk drives the wasm build + dev server. It is a one-time install.
    if !tool_available("trunk") {
        term::error("Trunk is required to run on the web but was not found.");
        println!("  install it once with:  cargo install --locked trunk");
        println!("  (Trunk builds the wasm bundle, runs wasm-bindgen, and serves with live reload.)");
        return ExitCode::FAILURE;
    }

    // 3. An index.html for Trunk. Respect the project's own if present; otherwise
    //    generate a managed one under `.pebbles/web/` (kept out of the source tree).
    let index = match ensure_index_html(member_dir, bin) {
        Ok(p) => p,
        Err(e) => {
            term::error(&format!("could not prepare the web entry page: {e}"));
            return ExitCode::FAILURE;
        }
    };

    // 4. `trunk serve` — builds, serves, opens the browser, and live-reloads on save.
    let mut cmd = Command::new("trunk");
    cmd.arg("serve").arg(&index).arg("--open").arg("--port").arg(o.port.to_string());
    if o.release {
        cmd.arg("--release");
    }
    cmd.current_dir(member_dir);
    term::hot(&format!("serving on http://localhost:{} — Ctrl+C to stop", o.port));
    // A blank page almost always means WebGPU is off in the browser (Pebbles is
    // WebGPU-only — no WebGL2 fallback). Spell out the fix, since a silent white
    // canvas is the most common first-run surprise, especially on Linux.
    term::step("needs a WebGPU browser. If the page is BLANK, WebGPU is disabled — enable it:");
    println!(
        "    • Chrome/Edge:  chrome://flags → \"Unsafe WebGPU Support\" = Enabled (Linux: also \"Vulkan\"), relaunch"
    );
    println!("    •   check with: chrome://gpu  →  \"WebGPU: Hardware accelerated\"");
    println!("    • Firefox:      about:config → dom.webgpu.enabled = true (Nightly/128+)");
    println!("    • Safari 26+:   on by default");
    println!(
        "    • quick test:   chromium --enable-unsafe-webgpu --enable-features=Vulkan http://localhost:{}",
        o.port
    );
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            term::error(&format!("could not launch trunk: {e}"));
            ExitCode::FAILURE
        }
    }
}

fn rustup_target_installed(target: &str) -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).lines().any(|l| l.trim() == target))
        .unwrap_or(false)
}

fn tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Return the index.html Trunk should build. If the project ships its own
/// `index.html`, use it untouched. Otherwise write a managed full-viewport page
/// to `<member>/.pebbles/web/index.html` (regenerated each run) that mounts the
/// app's `<bin>` crate onto a canvas.
fn ensure_index_html(member_dir: &Path, bin: &str) -> std::io::Result<PathBuf> {
    let project_index = member_dir.join("index.html");
    if project_index.is_file() {
        return Ok(project_index);
    }
    let web_dir = member_dir.join(".pebbles").join("web");
    std::fs::create_dir_all(&web_dir)?;
    // href is relative to this index.html → back up to the crate's Cargo.toml.
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no" />
  <title>{bin} — Pebbles</title>
  <style>
    html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: #ffffff; }}
    /* Fill the viewport and beat any inline size winit sets on the canvas, so the
       surface tracks the window (Flutter-style: canvas = viewport, buffer = size*dpr). */
    canvas {{
      position: fixed; inset: 0;
      width: 100% !important; height: 100% !important;
      display: block; outline: none; touch-action: none;
    }}
  </style>
  <!-- Build this workspace crate to wasm. Trunk runs cargo + wasm-bindgen. -->
  <link data-trunk rel="rust" href="../../Cargo.toml" data-bin="{bin}" data-wasm-opt="0" />
</head>
<body></body>
</html>
"#
    );
    let index = web_dir.join("index.html");
    std::fs::write(&index, html)?;
    Ok(index)
}

// --- mobile (`-d android` / `-d ios`) ----------------------------------------

/// Build the app to an APK and run it on a device/emulator via **cargo-apk2** (the
/// NDK-driven, Gradle-free NativeActivity path — the one-command analog to Trunk).
/// The app crate must be `crate-type = ["cdylib"]` with `#[pebbles::main]` (which
/// `pebbles new` scaffolds), so `android_main` is exported into the `.so`.
fn run_android(member_dir: &Path, bin: &str, o: &Opts) -> ExitCode {
    term::banner(&format!("pebbles run — {bin} (android, {})", if o.release { "release" } else { "dev" }));

    // 1. The Android rustup target.
    if !rustup_target_installed("aarch64-linux-android") {
        term::step("adding the aarch64-linux-android target…");
        let ok = Command::new("rustup")
            .args(["target", "add", "aarch64-linux-android"])
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            term::error("could not add the android target — run: rustup target add aarch64-linux-android");
            return ExitCode::FAILURE;
        }
    }

    // 2. The NDK (cargo-apk2 compiles native code, so it's required — no `check` shortcut).
    if find_android_ndk().is_none() {
        term::error("Android NDK not found — a device build needs it.");
        println!(
            "  install it (Android Studio → SDK Manager → NDK), then set ANDROID_NDK_HOME (or ANDROID_HOME)."
        );
        println!("  run `pebbles doctor` to check your whole Android setup.");
        return ExitCode::FAILURE;
    }

    // 3. cargo-apk2 (its subcommand is `apk2`, or `apk` on the older cargo-apk).
    let Some(sub) = cargo_apk_subcommand() else {
        term::error("cargo-apk2 is required to build + run the APK but was not found.");
        println!("  install it once with:  cargo install cargo-apk2");
        println!("  (builds the APK via the NDK and installs/launches it — NativeActivity path.)");
        return ExitCode::FAILURE;
    };

    // 4. Build + install + run on the connected device/emulator. winit needs an
    //    Android base class; use NativeActivity (NDK-only, no Gradle/AAR).
    let mut cmd = Command::new("cargo");
    cmd.arg(sub)
        .arg("run")
        .arg("-p")
        .arg(bin)
        .arg("--features")
        .arg("pebbles/android-native-activity")
        .current_dir(member_dir);
    if o.release {
        cmd.arg("--release");
    }
    term::hot("building the APK and launching on your device/emulator (Ctrl+C to stop)…");
    term::step(
        "Android uses NativeActivity here (no soft keyboard yet) and needs Vulkan (Android 7+). \
         The crate must be crate-type=[\"cdylib\"] + #[pebbles::main] — see documentations/android-support.md.",
    );
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    match cmd.status() {
        Ok(s) if s.success() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            term::error(&format!("could not launch cargo {sub}: {e}"));
            ExitCode::FAILURE
        }
    }
}

/// iOS can only be built on macOS (Apple's constraint). Point at the exact steps.
fn run_ios() -> ExitCode {
    term::banner("pebbles run — ios");
    if cfg!(target_os = "macos") {
        term::step("iOS run is driven by cargo-mobile2 (see documentations/ios-support.md):");
        println!("    1. cargo install cargo-mobile2");
        println!("    2. cargo mobile init      (generates the Xcode project)");
        println!("    3. cargo apple run        (Simulator, or a signed device)");
    } else {
        term::warn("iOS can only be built on a Mac — no Linux/Windows path (Apple's constraint).");
        println!(
            "  the whole stack already compiles for aarch64-apple-ios in CI; the on-device build needs macOS + Xcode."
        );
    }
    ExitCode::SUCCESS
}

/// Locate the Android NDK: env first, then the newest `<sdk>/ndk/<version>`.
fn find_android_ndk() -> Option<PathBuf> {
    for var in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"] {
        if let Some(p) = std::env::var_os(var) {
            let p = PathBuf::from(p);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let sdk = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .into_iter()
        .find_map(|v| std::env::var_os(v).map(PathBuf::from).filter(|p| p.is_dir()))
        .or_else(|| {
            let home =
                std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)?;
            [
                home.join("Android/Sdk"),
                home.join("Library/Android/sdk"),
                home.join("AppData/Local/Android/Sdk"),
            ]
            .into_iter()
            .find(|p| p.is_dir())
        })?;
    let mut versions: Vec<PathBuf> =
        std::fs::read_dir(sdk.join("ndk")).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    versions.sort();
    versions.pop()
}

/// The available cargo APK subcommand: `apk2` (cargo-apk2) or `apk` (cargo-apk).
fn cargo_apk_subcommand() -> Option<&'static str> {
    for sub in ["apk2", "apk"] {
        let ok = Command::new("cargo")
            .args([sub, "--version"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if ok {
            return Some(sub);
        }
    }
    None
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
    if let Some(path) = &o.log_file {
        // The file captures the COMPLETE trace (all levels) regardless of the
        // console level, so a post-mortem always has full detail.
        cmd.env("PEBBLES_LOG_FILE", path).env("PEBBLES_LOG_FILE_LEVEL", "trace");
    }
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

fn first_change(old: &BTreeMap<PathBuf, SystemTime>, new: &BTreeMap<PathBuf, SystemTime>) -> Option<PathBuf> {
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
        if in_package
            && t.starts_with("name")
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
        if manifest.is_file() && std::fs::read_to_string(&manifest).is_ok_and(|t| has_workspace_table(&t)) {
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
