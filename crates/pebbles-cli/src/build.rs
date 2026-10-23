//! `pebbles build` — produce a release artifact. The dev-loop's `run` sibling
//! builds-and-launches; `build` just builds (always optimized) and reports the
//! output. Desktop emits a native release binary; `-d web` emits a static wasm
//! bundle via Trunk; mobile prints next-steps guidance (packaging is v0.2.0).
//!
//! Deliberately thin — it reuses `run`'s platform parsing, member
//! resolution, cargo/trunk plumbing, and the managed `index.html`, so the two
//! commands stay in lockstep.

use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::time::Instant;

use crate::run::{
    Platform, RENDERER_DEPRECATED, cargo_build, ensure_index_html, find_workspace_root, resolve_member,
    rustup_target_installed, target_bin, tool_available, warn_legacy_backend_features,
};
use crate::term;

/// Parsed `pebbles build` options. A build is *always* release, so there is no
/// `--release` flag (unlike `run`); everything else mirrors `run`'s selectors.
#[derive(Debug, PartialEq, Eq)]
struct BuildOpts {
    package: Option<String>,
    platform: Platform,
    /// `--renderer` was passed (deprecated and ignored: there is one renderer).
    deprecated_renderer: bool,
}

/// Parse `pebbles build` argv into [`BuildOpts`]. Pure (no I/O), so it's unit-
/// testable — mirrors `run`'s `-d` / `-r` / `-p` handling. Returns the error
/// message to print on a bad flag; `build takes no positional args`, so a bare
/// token is rejected rather than silently swallowed.
fn parse_args(args: &[String]) -> Result<BuildOpts, String> {
    let mut o = BuildOpts { package: None, platform: Platform::Desktop, deprecated_renderer: false };
    let mut it = args.iter().peekable();
    while let Some(a) = it.next() {
        match a.as_str() {
            // Deprecated: accept and ignore one value, but never swallow the next flag.
            "-r" | "--renderer" | "--backend" => {
                if it.peek().is_some_and(|v| !v.starts_with('-')) {
                    it.next();
                }
                o.deprecated_renderer = true;
            }
            s if s.starts_with("--renderer=") || s.starts_with("--backend=") => o.deprecated_renderer = true,
            // Flutter-style target selection: `pebbles build -d web` (default: desktop).
            "-d" | "--device" | "--platform" => {
                let v = it.next().ok_or("`-d` needs a target: desktop | web | android | ios")?;
                o.platform = Platform::parse(v).ok_or_else(|| {
                    format!("unknown target `{v}` — use one of: desktop | web | android | ios")
                })?;
            }
            s if s.starts_with("--device=") => {
                let v = &s["--device=".len()..];
                o.platform = Platform::parse(v).ok_or_else(|| {
                    format!("unknown target `{v}` — use one of: desktop | web | android | ios")
                })?;
            }
            // Select a workspace member (or example) by name.
            "-p" | "--package" | "--example" | "--bin" => {
                let name = it.next().ok_or("`-p` needs a package name")?;
                o.package = Some(name.clone());
            }
            s if s.starts_with('-') => {
                return Err(format!("unknown option `{s}` for `pebbles build`"));
            }
            s => {
                return Err(format!(
                    "unexpected argument `{s}` — pebbles build takes no positional args (did you mean `-p {s}`?)"
                ));
            }
        }
    }
    Ok(o)
}

pub fn run(args: &[String]) -> ExitCode {
    let o = match parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            term::error(&e);
            return ExitCode::FAILURE;
        }
    };

    let cwd = std::env::current_dir().unwrap_or_default();
    let workspace_root = find_workspace_root(&cwd);

    // Resolve which package to build and its source directory (shared with `run`).
    let Some((member_dir, bin)) =
        resolve_member(&cwd, workspace_root.as_deref(), o.package.as_deref(), "build")
    else {
        return ExitCode::FAILURE;
    };
    if o.deprecated_renderer {
        term::warn(RENDERER_DEPRECATED);
    }
    warn_legacy_backend_features(&member_dir);

    // Flutter-style target dispatch. Desktop is the default native release build;
    // web emits a static bundle; mobile prints guidance only.
    match o.platform {
        Platform::Web => return build_web(&member_dir, &bin),
        Platform::Android => return build_mobile_guidance(Platform::Android),
        Platform::Ios => return build_mobile_guidance(Platform::Ios),
        Platform::Desktop => {}
    }

    // Where the binary lands: the workspace target if in a workspace, else the
    // package's own target (a standalone `pebbles new` project).
    let target_root = workspace_root.unwrap_or_else(|| member_dir.clone());

    term::banner(&format!("pebbles build — {bin} (release)"));
    term::step("building an optimized release binary…");
    let t = Instant::now();
    if !cargo_build(&target_root, &bin, true) {
        term::error("build failed — fix the errors above.");
        return ExitCode::FAILURE;
    }
    term::ok(&format!("built in {:.1}s", t.elapsed().as_secs_f64()));

    let bin_path = target_bin(&target_root, &bin, true);
    println!("  binary: {}", bin_path.display());
    if let Ok(meta) = std::fs::metadata(&bin_path) {
        println!("  size:   {}", human_size(meta.len()));
    }
    term::ok(&format!("done — release build of {bin} ready to ship"));
    ExitCode::SUCCESS
}

// --- web (`-d web`) ----------------------------------------------------------

/// Build the app to a **static** WebAssembly bundle with Trunk (`trunk build`,
/// not `serve`) and report the `dist/` output. Reuses `run`'s web setup: the
/// wasm-target check, the Trunk check (same install hint), and the managed
/// `index.html`. The result is a deployable static site.
fn build_web(member_dir: &Path, bin: &str) -> ExitCode {
    term::banner(&format!("pebbles build — {bin} (web, release)"));

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

    // 2. Trunk drives the wasm build. It is a one-time install (same hint as `run`).
    if !tool_available("trunk") {
        term::error("Trunk is required to build for the web but was not found.");
        println!("  install it once with:  cargo install --locked trunk");
        println!("  (Trunk builds the wasm bundle, runs wasm-bindgen, and emits a static bundle.)");
        return ExitCode::FAILURE;
    }

    // 3. The same managed (or project-supplied) index.html `run` uses.
    let index = match ensure_index_html(member_dir, bin) {
        Ok(p) => p,
        Err(e) => {
            term::error(&format!("could not prepare the web entry page: {e}"));
            return ExitCode::FAILURE;
        }
    };

    // 4. `trunk build` — the static-bundle analog of `trunk serve`.
    let mut cmd = Command::new("trunk");
    cmd.arg("build").arg(&index).arg("--release").arg("--dist").arg("./dist").arg("--public-url").arg("/");
    cmd.current_dir(member_dir);
    term::step("building a static wasm bundle with Trunk…");
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    match cmd.status() {
        Ok(s) if s.success() => {
            let dist = member_dir.join("dist");
            term::ok("done — static web bundle ready");
            println!("  output: {}", dist.display());
            if let Some(sz) = wasm_bundle_size(&dist) {
                println!("  wasm:   {}", human_size(sz));
            }
            println!("  deploy the contents of dist/ to any static host (public-url = /).");
            ExitCode::SUCCESS
        }
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            term::error(&format!("could not launch trunk: {e}"));
            ExitCode::FAILURE
        }
    }
}

/// The size of the wasm-bindgen output (`*_bg.wasm`) in a Trunk `dist/`, if one
/// is there — the number engineers care about for a web bundle.
fn wasm_bundle_size(dist: &Path) -> Option<u64> {
    let entries = std::fs::read_dir(dist).ok()?;
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.ends_with("_bg.wasm")))
        .filter_map(|p| std::fs::metadata(&p).ok().map(|m| m.len()))
        .max()
}

// --- mobile (`-d android` / `-d ios`) ----------------------------------------

/// Print next-steps guidance for a mobile target — packaging an APK/`.ipa` is
/// v0.2.0 territory, so no build is attempted. Informational (returns success).
fn build_mobile_guidance(platform: Platform) -> ExitCode {
    match platform {
        Platform::Android => {
            term::banner("pebbles build — android");
            term::step("packaging an APK is v0.2.0 territory — no build attempted yet.");
            println!("  the device path uses cargo-apk2 (NDK NativeActivity, Gradle-free):");
            println!("    1. cargo install cargo-apk2");
            println!("    2. cargo apk2 build -p <app> --release");
            println!("  run `pebbles doctor` to check your Android toolchain.");
        }
        Platform::Ios => {
            term::banner("pebbles build — ios");
            term::step("packaging an .app/.ipa is v0.2.0 territory — no build attempted yet.");
            if cfg!(target_os = "macos") {
                println!("  the device path uses cargo-mobile2:");
                println!("    1. cargo install cargo-mobile2");
                println!("    2. cargo mobile init      (generates the Xcode project)");
                println!("    3. cargo apple build --release");
            } else {
                term::warn("iOS can only be built on a Mac — no Linux/Windows path (Apple's constraint).");
                println!(
                    "  the whole stack already compiles for aarch64-apple-ios in CI; the packaged build needs macOS + Xcode."
                );
            }
        }
        // Desktop / Web never reach here — dispatched before this call.
        Platform::Desktop | Platform::Web => {}
    }
    ExitCode::SUCCESS
}

// --- helpers -----------------------------------------------------------------

/// Human-readable byte size for the artifact summary (binary/base-1024). Pure.
fn human_size(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    let b = bytes as f64;
    if b >= MIB {
        format!("{:.1} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.1} KiB", b / KIB)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_defaults_to_desktop_no_renderer_no_package() {
        let o = parse_args(&[]).unwrap();
        assert_eq!(o, BuildOpts { package: None, platform: Platform::Desktop, deprecated_renderer: false });
    }

    #[test]
    fn parse_selects_device() {
        assert_eq!(parse_args(&argv(&["-d", "web"])).unwrap().platform, Platform::Web);
        assert_eq!(parse_args(&argv(&["--device", "android"])).unwrap().platform, Platform::Android);
        // Flutter-ish synonyms flow through `Platform::parse`.
        assert_eq!(parse_args(&argv(&["-d", "chrome"])).unwrap().platform, Platform::Web);
        assert_eq!(parse_args(&argv(&["--device=ios"])).unwrap().platform, Platform::Ios);
    }

    #[test]
    fn parse_accepts_and_ignores_the_deprecated_renderer_flag() {
        let o = parse_args(&argv(&["--renderer", "gl"])).unwrap();
        assert!(o.deprecated_renderer);
        let o = parse_args(&argv(&["-r", "-d", "web"])).unwrap();
        assert!(o.deprecated_renderer);
        assert_eq!(o.platform, Platform::Web, "the value-less -r must not swallow -d");
        assert!(parse_args(&argv(&["--renderer=wgpu"])).unwrap().deprecated_renderer);
    }

    #[test]
    fn parse_selects_package() {
        assert_eq!(parse_args(&argv(&["-p", "counter"])).unwrap().package.as_deref(), Some("counter"));
        assert_eq!(parse_args(&argv(&["--bin", "gallery"])).unwrap().package.as_deref(), Some("gallery"));
    }

    #[test]
    fn parse_combines_flags() {
        let o = parse_args(&argv(&["-p", "gallery", "-d", "web", "--renderer", "wgpu"])).unwrap();
        assert_eq!(
            o,
            BuildOpts { package: Some("gallery".into()), platform: Platform::Web, deprecated_renderer: true }
        );
    }

    #[test]
    fn parse_rejects_bad_input() {
        // Unknown flag, unknown device, a bare positional, and a flag missing its value are
        // all errors (not silent).
        assert!(parse_args(&argv(&["--nope"])).is_err());
        assert!(parse_args(&argv(&["-d", "toaster"])).is_err());
        assert!(parse_args(&argv(&["counter"])).is_err());
        assert!(parse_args(&argv(&["-d"])).is_err());
        assert!(parse_args(&argv(&["-p"])).is_err());
    }

    #[test]
    fn human_size_scales_units() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(1024), "1.0 KiB");
        assert_eq!(human_size(1536), "1.5 KiB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MiB");
    }
}
