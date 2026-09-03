//! Tiny ANSI terminal helpers + a prettifier for the framework's log lines.
//!
//! Colors auto-disable when stdout isn't a TTY or `NO_COLOR` is set, so piped
//! output stays clean.

use std::sync::OnceLock;

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const BLUE: &str = "\x1b[34m";
pub const MAGENTA: &str = "\x1b[35m";
pub const CYAN: &str = "\x1b[36m";
pub const GRAY: &str = "\x1b[90m";

fn colors_on() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        if std::env::var("NO_COLOR").is_ok() {
            return false;
        }
        // Best-effort TTY check without libc: honor FORCE_COLOR, else assume a
        // terminal (dev tool run interactively). Piped runs can set NO_COLOR.
        std::env::var("FORCE_COLOR").is_ok() || std::env::var("TERM").is_ok_and(|t| t != "dumb")
    })
}

/// Strip ANSI codes from a template if colors are off.
fn c(s: &str) -> &str {
    if colors_on() { s } else { "" }
}

pub fn banner(msg: &str) {
    println!("{}{}▶{} {BOLD}{msg}{}", c(BOLD), c(CYAN), c(RESET), c(RESET));
}

pub fn step(msg: &str) {
    println!("{}•{} {msg}", c(CYAN), c(RESET));
}

pub fn ok(msg: &str) {
    println!("{}✓{} {msg}", c(GREEN), c(RESET));
}

pub fn warn(msg: &str) {
    eprintln!("{}⚠ {msg}{}", c(YELLOW), c(RESET));
}

pub fn error(msg: &str) {
    eprintln!("{}✗ error:{} {msg}", c(RED), c(RESET));
}

pub fn hot(msg: &str) {
    println!("{}{}🔥 {msg}{}", c(BOLD), c(MAGENTA), c(RESET));
}

/// Prettify one line of app output. Recognizes the framework log format
/// `[  1.234s LEVEL   cat] message` and colorizes by level/category; passes
/// anything else through unchanged (so `println!`/`dbg!` from the app still show).
pub fn app_line(line: &str) {
    if !colors_on() {
        println!("{line}");
        return;
    }
    // Format: "[<time> <LEVEL> <cat>] <msg>"
    if let Some(rest) = line.strip_prefix('[')
        && let Some(close) = rest.find(']')
    {
        let head = &rest[..close];
        let msg = &rest[close + 1..];
        let parts: Vec<&str> = head.split_whitespace().collect();
        if parts.len() >= 3 {
            let (time, level, cat) = (parts[0], parts[1], parts[2]);
            let level_color = match level {
                "ERROR" => RED,
                "WARN" => YELLOW,
                "INFO" => GREEN,
                "DEBUG" => BLUE,
                "TRACE" => GRAY,
                _ => RESET,
            };
            // Overflow / GPU lines get an extra emphasis so they don't scroll past.
            let msg_color = if cat == "layout" && msg.contains("overflow") {
                YELLOW
            } else if level == "ERROR" {
                RED
            } else {
                RESET
            };
            println!(
                "{GRAY}{time:>8}{RESET} {level_color}{BOLD}{level:<5}{RESET} {MAGENTA}{cat:<7}{RESET}{msg_color}{msg}{RESET}"
            );
            return;
        }
    }
    // Cargo/rustc diagnostics: tint the common markers.
    if line.starts_with("error") || line.contains("error[") {
        println!("{RED}{line}{RESET}");
    } else if line.starts_with("warning") {
        println!("{YELLOW}{line}{RESET}");
    } else {
        println!("{line}");
    }
}
