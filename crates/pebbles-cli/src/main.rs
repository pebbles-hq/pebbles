//! `pebbles` — the developer CLI for the Pebbles GUI framework.
//!
//! Flutter-style tooling for a Rust desktop UI:
//!
//! ```text
//! pebbles new <name>      scaffold a new Pebbles app
//! pebbles run             build + run in dev mode (rich logs, hot-restart on save)
//! pebbles doctor          check your environment
//! ```
//!
//! Deliberately std-only — no clap, no notify. Arg parsing is hand-rolled, file
//! watching is a light mtime poll, process control is `std::process`.

mod doctor;
mod new;
mod run;
mod term;

use std::process::ExitCode;

/// The pebbles repo this CLI was built from — scaffolded projects point their
/// `pebbles = { path = ... }` dependency here so a fresh app builds immediately
/// against the local checkout. `env!` resolves at build time to
/// `<repo>/crates/pebbles-cli`; the repo root is two levels up.
pub fn pebbles_repo_root() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(|p| p.parent()).unwrap_or(manifest).to_path_buf()
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str);

    match cmd {
        Some("new") | Some("create") => new::run(&args[1..]),
        Some("run") | Some("dev") => run::run(&args[1..]),
        Some("doctor") => doctor::run(&args[1..]),
        Some("--version") | Some("-V") | Some("version") => {
            println!("pebbles {VERSION}");
            ExitCode::SUCCESS
        }
        Some("--help") | Some("-h") | Some("help") | None => {
            print_help();
            ExitCode::SUCCESS
        }
        Some(other) => {
            term::error(&format!("unknown command `{other}`"));
            print_help();
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    use term::{BOLD, CYAN, DIM, RESET};
    println!(
        "\
{BOLD}pebbles{RESET} {DIM}{VERSION}{RESET} — the Pebbles developer CLI

{BOLD}USAGE{RESET}
    pebbles <command> [options]

{BOLD}COMMANDS{RESET}
    {CYAN}new{RESET} <name>        Scaffold a new Pebbles app in ./<name>
    {CYAN}run{RESET}               Build + run the app in dev mode, with rich logs and
                      hot-restart on every file save
    {CYAN}doctor{RESET}            Check your toolchain and environment
    {CYAN}help{RESET}              Show this help

{BOLD}pebbles run OPTIONS{RESET}
    --release         Build/run optimized (no dev diagnostics)
    --no-reload       Disable hot-restart (build + run once)
    -q, --quiet       Only app logs at warn+ (default: debug)
    --log <level>     trace | debug | info | warn | error  (default: debug)
    --                Everything after `--` is passed to the app

{BOLD}EXAMPLES{RESET}
    pebbles new hello && cd hello && pebbles run
    pebbles run --log trace
    pebbles doctor
"
    );
}
