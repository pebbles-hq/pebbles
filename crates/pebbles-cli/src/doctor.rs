//! `pebbles doctor` — a quick environment check, Flutter-style.

use std::process::{Command, ExitCode};

use crate::pebbles_repo_root;
use crate::term;

pub fn run(_args: &[String]) -> ExitCode {
    term::banner("pebbles doctor");
    let mut problems = 0;

    // Rust toolchain.
    match tool_version("cargo", &["--version"]) {
        Some(v) => term::ok(&format!("cargo — {v}")),
        None => {
            term::error("cargo not found — install Rust from https://rustup.rs");
            problems += 1;
        }
    }
    match tool_version("rustc", &["--version"]) {
        Some(v) => term::ok(&format!("rustc — {v}")),
        None => {
            term::error("rustc not found");
            problems += 1;
        }
    }

    // The pebbles checkout scaffolds point at.
    let root = pebbles_repo_root();
    if root.join("crates/pebbles/Cargo.toml").is_file() {
        term::ok(&format!("pebbles source — {}", root.display()));
    } else {
        term::warn(&format!(
            "pebbles source not found at {} — `pebbles new` will still work with --git",
            root.display()
        ));
    }

    // GPU / Vulkan (best-effort; a desktop GUI needs a working GPU stack).
    match tool_version("vulkaninfo", &["--summary"]) {
        Some(_) => term::ok("Vulkan — vulkaninfo present"),
        None => term::warn(
            "vulkaninfo not found — Pebbles renders via wgpu/Vulkan; install your GPU's Vulkan \
             driver if the window is black (mesa-vulkan-drivers / vulkan-tools)",
        ),
    }

    // Are we inside a project?
    if std::env::current_dir().is_ok_and(|d| d.join("Cargo.toml").is_file()) {
        term::ok("current directory is a Cargo project");
    } else {
        term::step("tip: run `pebbles new <name>` to scaffold an app");
    }

    println!();
    if problems == 0 {
        term::ok("no blocking problems found.");
        ExitCode::SUCCESS
    } else {
        term::error(&format!("{problems} problem(s) need attention."));
        ExitCode::FAILURE
    }
}

fn tool_version(bin: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(bin).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    Some(text.lines().next().unwrap_or("").trim().to_string())
}
