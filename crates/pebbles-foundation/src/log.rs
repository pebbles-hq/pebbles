//! A lightweight, always-available diagnostic log for the whole GUI stack.
//!
//! The problem this solves: when the app freezes or "goes black", stderr prints
//! from scattered `eprintln!`s don't tell you *where in the frame* it died or
//! *what the UI was doing* at that moment. This gives every layer — reactive
//! core, widgets, shell render loop — one timestamped, leveled, categorized
//! stream plus an in-memory ring buffer you can dump on a panic or from an
//! on-screen overlay.
//!
//! ## Usage
//! ```ignore
//! use pebbles_core::log;
//! log::info(log::Cat::Nav, "route → markdown");
//! log::logf(log::Level::Warn, log::Cat::Gpu, format_args!("reset #{n}"));
//! ```
//!
//! ## Controls (env)
//! - `PEBBLES_LOG=1` — echo to stderr (default: off, the ring buffer always fills).
//! - `PEBBLES_LOG=trace|debug|info|warn|error` — echo at/above that level.
//! - `PEBBLES_LOG_FILE=<path>` — also append every record to a file (line-buffered,
//!   flushed each write, so a hard freeze still leaves the last line on disk).
//!
//! The ring buffer is ALWAYS on (cheap), so [`dump`]/[`snapshot`] work even when
//! stderr echo is off. The shell installs a panic hook that dumps it.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::fs::File;
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

/// Severity. Lower = noisier.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[repr(u8)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Info = 2,
    Warn = 3,
    Error = 4,
}

impl Level {
    fn tag(self) -> &'static str {
        match self {
            Level::Trace => "TRACE",
            Level::Debug => "DEBUG",
            Level::Info => "INFO ",
            Level::Warn => "WARN ",
            Level::Error => "ERROR",
        }
    }
    fn parse(s: &str) -> Option<Level> {
        match s.trim().to_ascii_lowercase().as_str() {
            "trace" => Some(Level::Trace),
            "debug" => Some(Level::Debug),
            "info" => Some(Level::Info),
            "warn" | "warning" => Some(Level::Warn),
            "error" => Some(Level::Error),
            _ => None,
        }
    }
}

/// What part of the UI a record is about — so a stream can be read by concern.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Cat {
    /// Frame lifecycle: heartbeat, timings, slow frames.
    Frame,
    /// Layout: constraint solving, overflow (Flutter-style), intrinsic sizing.
    Layout,
    /// GPU: surface/renderer creation, render errors, device resets, present.
    Gpu,
    /// CPU / performance: per-frame timings, task pumps, slow spans.
    Perf,
    /// Input: pointer/key dispatch (mostly Trace).
    Input,
    /// Navigation / route changes.
    Nav,
    /// Reactive engine: mount/unmount, dispose, dirty reconciliation.
    Reactive,
    /// Overlay / popover layer.
    Overlay,
    /// A widget-level note.
    Widget,
    /// Anything else.
    General,
}

impl Cat {
    fn tag(self) -> &'static str {
        match self {
            Cat::Frame => "frame",
            Cat::Layout => "layout",
            Cat::Gpu => "gpu",
            Cat::Perf => "perf",
            Cat::Input => "input",
            Cat::Nav => "nav",
            Cat::Reactive => "react",
            Cat::Overlay => "overlay",
            Cat::Widget => "widget",
            Cat::General => "gen",
        }
    }
}

/// One captured record (kept in the ring buffer).
#[derive(Clone)]
pub struct Record {
    pub at_ms: u128,
    pub level: Level,
    pub cat: Cat,
    pub msg: String,
}

impl fmt::Display for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (s, ms) = (self.at_ms / 1000, self.at_ms % 1000);
        write!(f, "[{s:>4}.{ms:03}s {} {:>7}] {}", self.level.tag(), self.cat.tag(), self.msg)
    }
}

/// Echo threshold: 255 = off (ring buffer only). Set once from the environment.
static ECHO: AtomicU8 = AtomicU8::new(255);
static START: OnceLock<Instant> = OnceLock::new();
static FILE: OnceLock<Option<std::sync::Mutex<File>>> = OnceLock::new();

const RING_CAP: usize = 4096;

thread_local! {
    /// The ring buffer — the UI runs single-threaded, so a thread-local is the
    /// whole store (background tasks log via their own buffers, which is fine —
    /// we only ever dump the UI thread's).
    static RING: RefCell<VecDeque<Record>> = RefCell::new(VecDeque::with_capacity(RING_CAP));
}

/// Initialize from the environment. Idempotent; the shell calls it at startup,
/// but any first log call also lazily initializes so library use never panics.
pub fn init() {
    START.get_or_init(Instant::now);
    FILE.get_or_init(|| {
        std::env::var("PEBBLES_LOG_FILE").ok().and_then(|p| {
            File::create(&p).ok().map(|f| {
                eprintln!("pebbles: logging to {p}");
                std::sync::Mutex::new(f)
            })
        })
    });
    // Dev mode (set by `pebbles run`, or manually) turns on Debug echo by default —
    // an explicit PEBBLES_LOG still wins.
    if let Ok(v) = std::env::var("PEBBLES_LOG") {
        let lvl = if v == "1" || v == "true" {
            Level::Debug
        } else {
            Level::parse(&v).unwrap_or(Level::Debug)
        };
        ECHO.store(lvl as u8, Ordering::Relaxed);
    } else if dev_mode() {
        ECHO.store(Level::Debug as u8, Ordering::Relaxed);
    }
}

/// Whether the app is running under `pebbles run` / dev mode (`PEBBLES_DEV=1`).
/// Dev mode enables Flutter-style diagnostics — overflow detection, richer render
/// logging — that are too chatty for a shipped build. Checked once.
pub fn dev_mode() -> bool {
    static DEV: OnceLock<bool> = OnceLock::new();
    *DEV.get_or_init(|| std::env::var("PEBBLES_DEV").is_ok_and(|v| v == "1" || v == "true"))
}

fn now_ms() -> u128 {
    START.get_or_init(Instant::now).elapsed().as_millis()
}

/// The core sink. Everything routes here.
pub fn logf(level: Level, cat: Cat, args: fmt::Arguments<'_>) {
    // Lazy init so a library log before the shell's init() still works.
    if START.get().is_none() {
        init();
    }
    let rec = Record { at_ms: now_ms(), level, cat, msg: fmt::format(args) };

    let echo = ECHO.load(Ordering::Relaxed);
    if echo != 255 && level as u8 >= echo {
        eprintln!("{rec}");
    }
    if let Some(Some(mx)) = FILE.get()
        && let Ok(mut f) = mx.lock()
    {
        let _ = writeln!(f, "{rec}");
        let _ = f.flush();
    }
    RING.with(|r| {
        let mut r = r.borrow_mut();
        if r.len() == RING_CAP {
            r.pop_front();
        }
        r.push_back(rec);
    });
}

/// Log a pre-formatted string.
pub fn log(level: Level, cat: Cat, msg: impl Into<String>) {
    logf(level, cat, format_args!("{}", msg.into()));
}

// Convenience shorthands ------------------------------------------------------
pub fn trace(cat: Cat, msg: impl Into<String>) {
    log(Level::Trace, cat, msg);
}
pub fn debug(cat: Cat, msg: impl Into<String>) {
    log(Level::Debug, cat, msg);
}
pub fn info(cat: Cat, msg: impl Into<String>) {
    log(Level::Info, cat, msg);
}
pub fn warn(cat: Cat, msg: impl Into<String>) {
    log(Level::Warn, cat, msg);
}
pub fn error(cat: Cat, msg: impl Into<String>) {
    log(Level::Error, cat, msg);
}

/// A copy of the ring buffer (oldest→newest) — for an on-screen log overlay or a
/// crash dump.
pub fn snapshot() -> Vec<Record> {
    RING.with(|r| r.borrow().iter().cloned().collect())
}

/// The most recent `n` records, newest last.
pub fn tail(n: usize) -> Vec<Record> {
    RING.with(|r| {
        let r = r.borrow();
        let start = r.len().saturating_sub(n);
        r.iter().skip(start).cloned().collect()
    })
}

/// Write the whole ring buffer to stderr, framed — used by the panic hook so a
/// crash always shows the last few thousand UI events regardless of echo level.
pub fn dump(reason: &str) {
    let snap = snapshot();
    eprintln!("\n──────── pebbles UI log dump ({reason}, {} records) ────────", snap.len());
    for rec in &snap {
        eprintln!("{rec}");
    }
    eprintln!("──────── end pebbles UI log dump ────────\n");
    if let Some(Some(mx)) = FILE.get()
        && let Ok(mut f) = mx.lock()
    {
        let _ = writeln!(f, "──── dump ({reason}) ────");
        let _ = f.flush();
    }
}

/// Whether stderr echo is enabled at all (so hot paths can skip building a string).
pub fn echo_enabled() -> bool {
    ECHO.load(Ordering::Relaxed) != 255
}
