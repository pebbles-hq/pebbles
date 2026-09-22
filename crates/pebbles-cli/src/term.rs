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
    if let Some((time, level, cat, msg)) = parse_log_line(line) {
        let lvl_color = level_color(level);
        // Overflow / GPU lines get an extra emphasis so they don't scroll past.
        let msg_color = if cat == "layout" && msg.contains("overflow") {
            YELLOW
        } else if level == "ERROR" {
            RED
        } else {
            RESET
        };
        println!(
            "{GRAY}{time:>8}{RESET} {lvl_color}{BOLD}{level:<5}{RESET} {MAGENTA}{cat:<7}{RESET}{msg_color}{msg}{RESET}"
        );
        return;
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

/// Parse the framework log format `[<time> <LEVEL> <cat>] <msg>` into its four
/// parts. `None` when the line isn't in that shape (the caller then passes it
/// through verbatim). Pure, so the parsing is testable without capturing stdout.
fn parse_log_line(line: &str) -> Option<(&str, &str, &str, &str)> {
    let rest = line.strip_prefix('[')?;
    let close = rest.find(']')?;
    let head = &rest[..close];
    let msg = &rest[close + 1..];
    let parts: Vec<&str> = head.split_whitespace().collect();
    (parts.len() >= 3).then(|| (parts[0], parts[1], parts[2], msg))
}

/// The ANSI color for a log-level token; `RESET` (no tint) for anything unknown.
fn level_color(level: &str) -> &'static str {
    match level {
        "ERROR" => RED,
        "WARN" => YELLOW,
        "INFO" => GREEN,
        "DEBUG" => BLUE,
        "TRACE" => GRAY,
        _ => RESET,
    }
}

#[cfg(test)]
mod tests {
    use super::{BLUE, GREEN, RED, RESET, YELLOW, level_color, parse_log_line};

    #[test]
    fn parse_log_line_splits_the_framework_format() {
        let (time, level, cat, msg) = parse_log_line("[  1.234s INFO   layout] laid out 12 nodes").unwrap();
        assert_eq!(time, "1.234s");
        assert_eq!(level, "INFO");
        assert_eq!(cat, "layout");
        // The message keeps its leading space from after the `]`.
        assert_eq!(msg, " laid out 12 nodes");
    }

    #[test]
    fn parse_log_line_rejects_non_log_lines() {
        // Plain app output (no bracketed header) is passed through by the caller.
        assert!(parse_log_line("hello from the app").is_none());
        // A bracket but too few header fields is not the log format.
        assert!(parse_log_line("[oops] not enough parts").is_none());
        // An opening bracket with no close is rejected (no panic).
        assert!(parse_log_line("[unterminated").is_none());
    }

    #[test]
    fn level_color_maps_known_levels_and_defaults_to_reset() {
        assert_eq!(level_color("ERROR"), RED);
        assert_eq!(level_color("WARN"), YELLOW);
        assert_eq!(level_color("INFO"), GREEN);
        assert_eq!(level_color("DEBUG"), BLUE);
        assert_eq!(level_color("TRACE"), super::GRAY);
        // Unknown / lowercase → no tint.
        assert_eq!(level_color("VERBOSE"), RESET);
        assert_eq!(level_color("info"), RESET);
    }
}
