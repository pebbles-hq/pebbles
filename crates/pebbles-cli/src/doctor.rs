//! `pebbles doctor` — environment + per-platform toolchain check, Flutter-style.
//!
//! Grouped by target (Rust core, Desktop, Web, Android, iOS). Each line is
//! ✓ (ready) · ! (optional/target-specific gap, with the fix) · ✗ (blocking) ·
//! – (info). Only ✗ makes the command exit non-zero, so `pebbles doctor` is green
//! as long as you can build for at least the host, and tells you exactly what to
//! install for the targets you haven't set up yet.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::sync::OnceLock;

use crate::pebbles_repo_root;
use crate::term::{self, BOLD, CYAN, DIM, GRAY, GREEN, RED, RESET, YELLOW};

pub fn run(_args: &[String]) -> ExitCode {
    term::banner("pebbles doctor");
    println!("{DIM}checks your setup for each target; only ✗ blocks.{RESET}\n");

    let mut blocking = 0;

    // ── Rust toolchain (the one hard requirement) ────────────────────────────
    group("Rust toolchain");
    match tool_version("cargo", &["--version"]) {
        Some(v) => item(St::Ok, "cargo", &v),
        None => {
            item(St::Fail, "cargo", "not found — install Rust from https://rustup.rs");
            blocking += 1;
        }
    }
    match tool_version("rustc", &["--version"]) {
        Some(v) => item(St::Ok, "rustc", &v),
        None => {
            item(St::Fail, "rustc", "not found — install Rust from https://rustup.rs");
            blocking += 1;
        }
    }
    match tool_version("rustfmt", &["--version"]) {
        Some(v) => item(St::Ok, "rustfmt", &v),
        None => item(St::Warn, "rustfmt", "not found — `rustup component add rustfmt`"),
    }
    if have("cargo-clippy") || tool_version("cargo", &["clippy", "--version"]).is_some() {
        item(St::Ok, "clippy", "installed");
    } else {
        item(St::Warn, "clippy", "not found — `rustup component add clippy`");
    }

    // ── Pebbles checkout (what `pebbles new` scaffolds against) ───────────────
    group("Pebbles");
    let root = pebbles_repo_root();
    if root.join("crates/pebbles/Cargo.toml").is_file() {
        item(St::Ok, "source", &root.display().to_string());
    } else {
        item(St::Warn, "source", "not found here — `pebbles new` still works with --git");
    }

    // ── Desktop (the host) ───────────────────────────────────────────────────
    group(&format!("Desktop ({})", host_os()));
    #[cfg(target_os = "linux")]
    match tool_version("vulkaninfo", &["--summary"]) {
        Some(_) => item(St::Ok, "Vulkan", "vulkaninfo present"),
        None => item(
            St::Warn,
            "Vulkan",
            "vulkaninfo not found — Pebbles renders via wgpu/Vulkan; install your GPU driver \
             (mesa-vulkan-drivers / vulkan-tools) if the window is black",
        ),
    }
    #[cfg(target_os = "macos")]
    item(St::Ok, "Metal", "built in on macOS");
    #[cfg(target_os = "windows")]
    item(St::Ok, "graphics", "Vulkan/DX12 via wgpu");
    item(St::Ok, "run", "pebbles run   (the default target)");

    // ── Web ──────────────────────────────────────────────────────────────────
    group("Web  (pebbles run -d web)");
    if rustup_target_installed("wasm32-unknown-unknown") {
        item(St::Ok, "wasm target", "wasm32-unknown-unknown installed");
    } else {
        item(St::Warn, "wasm target", "missing — `rustup target add wasm32-unknown-unknown`");
    }
    match tool_version("trunk", &["--version"]) {
        Some(v) => item(St::Ok, "trunk", &v),
        None => item(St::Warn, "trunk", "not found — `cargo install --locked trunk`"),
    }
    item(St::Info, "browser", "needs WebGPU: Chrome/Edge, Safari 26+, or Firefox (see PLATFORMS.md)");

    // ── Android ──────────────────────────────────────────────────────────────
    group("Android  (pebbles run -d android)");
    let mut android_missing = 0;
    let sdk = android_sdk();
    match &sdk {
        Some(p) => item(St::Ok, "Android SDK", &p.display().to_string()),
        None => {
            item(St::Warn, "Android SDK", "not found — install Android Studio, or set ANDROID_HOME");
            android_missing += 1;
        }
    }
    match android_ndk(sdk.as_deref()) {
        Some(p) => item(St::Ok, "NDK", &p.display().to_string()),
        None => {
            item(
                St::Warn,
                "NDK",
                "not found — install via Android Studio SDK Manager, or set ANDROID_NDK_HOME",
            );
            android_missing += 1;
        }
    }
    if rustup_target_installed("aarch64-linux-android") {
        item(St::Ok, "android target", "aarch64-linux-android installed");
    } else {
        item(St::Warn, "android target", "missing — `rustup target add aarch64-linux-android`");
        android_missing += 1;
    }
    // `pebbles run -d android` builds + installs via cargo-apk2 (the NDK-driven,
    // Gradle-free NativeActivity path) — so that, not cargo-ndk/Gradle/JDK, is
    // what doctor checks. Accept the older cargo-apk too, as the run path does.
    if tool_version("cargo", &["apk2", "--version"]).is_some() || have("cargo-apk2") {
        item(St::Ok, "cargo-apk2", "installed");
    } else if tool_version("cargo", &["apk", "--version"]).is_some() || have("cargo-apk") {
        item(St::Ok, "cargo-apk", "installed (older cargo-apk; cargo-apk2 recommended)");
    } else {
        item(
            St::Warn,
            "cargo-apk2",
            "not found — `cargo install cargo-apk2` (builds + runs the APK via the NDK, Gradle-free)",
        );
        android_missing += 1;
    }
    if have("adb") || sdk.as_deref().is_some_and(|s| s.join("platform-tools/adb").exists()) {
        item(St::Ok, "adb", "present (device/emulator install)");
    } else {
        item(St::Warn, "adb", "not found — comes with the SDK platform-tools");
    }
    if android_missing == 0 {
        item(St::Info, "status", "toolchain looks ready");
    }

    // ── iOS ──────────────────────────────────────────────────────────────────
    group("iOS  (pebbles run -d ios)");
    if cfg!(target_os = "macos") {
        match tool_version("xcodebuild", &["-version"]) {
            Some(v) => item(St::Ok, "Xcode", &v.replace('\n', " ")),
            None => item(
                St::Warn,
                "Xcode",
                "not found — install from the App Store, then `xcode-select --install`",
            ),
        }
        if rustup_target_installed("aarch64-apple-ios") {
            item(St::Ok, "ios target", "aarch64-apple-ios installed");
        } else {
            item(St::Warn, "ios target", "missing — `rustup target add aarch64-apple-ios`");
        }
        if have("cargo-mobile2") || have("cargo-apple") {
            item(St::Ok, "cargo-mobile2", "installed");
        } else {
            item(St::Warn, "cargo-mobile2", "not found — `cargo install cargo-mobile2`");
        }
    } else {
        item(St::Info, "macOS required", "iOS can only be built on a Mac (Apple's constraint)");
    }

    // ── Summary ──────────────────────────────────────────────────────────────
    println!();
    if blocking == 0 {
        term::ok("no blocking problems — you can build Pebbles apps.");
        println!(
            "{DIM}  ! items are optional per-target setup; install them when you target that platform.{RESET}"
        );
        ExitCode::SUCCESS
    } else {
        term::error(&format!("{blocking} blocking problem(s) — Rust itself needs attention."));
        ExitCode::FAILURE
    }
}

// ── output helpers ───────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum St {
    Ok,
    Warn,
    Fail,
    Info,
}

fn group(title: &str) {
    println!("\n{BOLD}{CYAN}{title}{RESET}");
}

fn item(st: St, label: &str, detail: &str) {
    let (glyph, color) = match st {
        St::Ok => ("✓", GREEN),
        St::Warn => ("!", YELLOW),
        St::Fail => ("✗", RED),
        St::Info => ("–", GRAY),
    };
    println!("  {color}{glyph}{RESET} {label} {DIM}—{RESET} {detail}");
}

// ── probes ───────────────────────────────────────────────────────────────────

fn tool_version(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(version_line(&out.stdout, &out.stderr))
}

/// Pick the version string from a tool's captured output: prefer stdout, fall back
/// to stderr (some tools — `java -version` — print the version there), then take the
/// first line, trimmed. Pure, so the parsing is testable without spawning a process.
fn version_line(stdout: &[u8], stderr: &[u8]) -> String {
    let text =
        if stdout.is_empty() { String::from_utf8_lossy(stderr) } else { String::from_utf8_lossy(stdout) };
    text.lines().next().unwrap_or("").trim().to_string()
}

fn have(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `rustup target list --installed`, fetched once.
fn installed_targets() -> &'static str {
    static TARGETS: OnceLock<String> = OnceLock::new();
    TARGETS.get_or_init(|| {
        Command::new("rustup")
            .args(["target", "list", "--installed"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
            .unwrap_or_default()
    })
}

fn rustup_target_installed(target: &str) -> bool {
    target_in_list(installed_targets(), target)
}

/// Whether `target` appears (as a whole, trimmed line) in a `rustup target list
/// --installed` dump. Pure decision helper — exact match, so a prefix like
/// `aarch64-linux-android` doesn't spuriously satisfy a longer query.
fn target_in_list(list: &str, target: &str) -> bool {
    list.lines().any(|l| l.trim() == target)
}

fn host_os() -> &'static str {
    if cfg!(target_os = "linux") {
        "Linux"
    } else if cfg!(target_os = "macos") {
        "macOS"
    } else if cfg!(target_os = "windows") {
        "Windows"
    } else {
        "host"
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(PathBuf::from)
}

/// Locate the Android SDK: env first, then the conventional per-OS location.
fn android_sdk() -> Option<PathBuf> {
    for var in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(p) = std::env::var_os(var) {
            let p = PathBuf::from(p);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let h = home()?;
    let candidates = [
        h.join("Android/Sdk"),               // Linux
        h.join("Library/Android/sdk"),       // macOS
        h.join("AppData/Local/Android/Sdk"), // Windows
    ];
    candidates.into_iter().find(|p| p.is_dir())
}

/// Locate the NDK: env first, then the newest `<sdk>/ndk/<version>`.
fn android_ndk(sdk: Option<&Path>) -> Option<PathBuf> {
    for var in ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT", "NDK_HOME"] {
        if let Some(p) = std::env::var_os(var) {
            let p = PathBuf::from(p);
            if p.is_dir() {
                return Some(p);
            }
        }
    }
    let ndk_root = sdk?.join("ndk");
    let mut versions: Vec<PathBuf> =
        std::fs::read_dir(&ndk_root).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    versions.sort();
    versions.pop() // newest by lexical version
}

#[cfg(test)]
mod tests {
    use super::{host_os, target_in_list, version_line};

    #[test]
    fn version_line_prefers_stdout_and_trims() {
        // Normal case: first stdout line, trimmed.
        assert_eq!(version_line(b"cargo 1.89.0 (abc)\n", b""), "cargo 1.89.0 (abc)");
        assert_eq!(version_line(b"  rustc 1.89.0  \n2nd line", b""), "rustc 1.89.0");
    }

    #[test]
    fn version_line_falls_back_to_stderr_when_stdout_empty() {
        // e.g. `java -version` prints to stderr.
        assert_eq!(version_line(b"", b"openjdk version \"21\"\n"), "openjdk version \"21\"");
        // Both empty → empty string, not a panic.
        assert_eq!(version_line(b"", b""), "");
    }

    #[test]
    fn target_in_list_is_an_exact_whole_line_match() {
        let list = "aarch64-apple-ios\n  wasm32-unknown-unknown  \nx86_64-unknown-linux-gnu\n";
        assert!(target_in_list(list, "wasm32-unknown-unknown"), "trimmed whole-line match hits");
        assert!(target_in_list(list, "aarch64-apple-ios"));
        // A prefix / substring must NOT satisfy a different query.
        assert!(!target_in_list(list, "aarch64-apple-ios-sim"));
        assert!(!target_in_list(list, "wasm32"));
        assert!(!target_in_list("", "any-target"));
    }

    #[test]
    fn host_os_reports_the_current_platform() {
        let os = host_os();
        // Non-empty, and consistent with the compile target.
        assert!(!os.is_empty());
        #[cfg(target_os = "linux")]
        assert_eq!(os, "Linux");
        #[cfg(target_os = "macos")]
        assert_eq!(os, "macOS");
        #[cfg(target_os = "windows")]
        assert_eq!(os, "Windows");
    }
}
