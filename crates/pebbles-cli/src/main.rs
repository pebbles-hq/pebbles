//! `pebbles` — the developer CLI for the Pebbles GUI framework.
//!
//! Scaffold, run, and check a Pebbles project:
//!
//! ```text
//! pebbles create <name>   scaffold a new Pebbles project (app or widget package)
//! pebbles run             build + run in dev mode (rich logs, hot-restart on save)
//! pebbles build           build an optimized release artifact
//! pebbles bundle          package for Linux desktops (AppDir/AppImage + .desktop)
//! pebbles doctor          check your environment
//! ```
//!
//! Deliberately std-only — no clap, no notify. Arg parsing is hand-rolled, file
//! watching is a light mtime poll, process control is `std::process`.

mod build;
mod bundle;
mod doctor;
mod new;
mod run;
mod template;
mod term;

use std::process::ExitCode;

/// The pebbles repo this CLI was built from. `pebbles create --path` points a
/// generated project's dependencies here, for working ON the framework; the
/// default is a git dependency, which is the only form that works on a machine
/// without a checkout. `env!` resolves at build time to
/// `<repo>/crates/pebbles-cli`, so the repo root is two levels up.
pub fn pebbles_repo_root() -> std::path::PathBuf {
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().and_then(|p| p.parent()).unwrap_or(manifest).to_path_buf()
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The subcommand an invocation selects, parsed from `argv[1]`. Pure — `main`
/// turns it into the actual action, so the arg parsing is testable on its own.
#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    New,
    Run,
    Build,
    Bundle,
    Doctor,
    Version,
    Help,
    Unknown(String),
}

/// Map the first CLI argument (the command word) to a [`Cmd`]. `None` (no args) is
/// `Help`, matching Cargo-style bare invocation.
fn classify(cmd: Option<&str>) -> Cmd {
    match cmd {
        Some("new") | Some("create") => Cmd::New,
        Some("run") | Some("dev") => Cmd::Run,
        Some("build") => Cmd::Build,
        Some("bundle") => Cmd::Bundle,
        Some("doctor") => Cmd::Doctor,
        Some("--version") | Some("-V") | Some("version") => Cmd::Version,
        Some("--help") | Some("-h") | Some("help") | None => Cmd::Help,
        Some(other) => Cmd::Unknown(other.to_string()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match classify(args.first().map(String::as_str)) {
        Cmd::New => new::run(&args[1..]),
        Cmd::Run => run::run(&args[1..]),
        Cmd::Build => build::run(&args[1..]),
        Cmd::Bundle => bundle::run(&args[1..]),
        Cmd::Doctor => doctor::run(&args[1..]),
        Cmd::Version => {
            println!("pebbles {VERSION}");
            ExitCode::SUCCESS
        }
        Cmd::Help => {
            print_help();
            ExitCode::SUCCESS
        }
        Cmd::Unknown(other) => {
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
    {CYAN}create{RESET} <name>     Scaffold a new Pebbles project in ./<name>
                      (alias: {CYAN}new{RESET})
    {CYAN}run{RESET}               Build + run the app in dev mode, with rich logs and
                      hot-restart on every file save
    {CYAN}build{RESET}             Build an optimized release artifact (native binary,
                      or a static wasm bundle with -d web)
    {CYAN}bundle{RESET}            Package a release build for Linux desktops: an AppDir
                      with a .desktop entry + hicolor icons, and an AppImage
                      when appimagetool is installed
    {CYAN}doctor{RESET}            Check your toolchain and environment
    {CYAN}help{RESET}              Show this help

{BOLD}pebbles create OPTIONS{RESET}
    -t, --template <k> app | widget      (default: app)
    --list            List the templates and exit
    --path            Depend on the local Pebbles checkout instead of git —
                      for developing the framework itself

{BOLD}pebbles run OPTIONS{RESET}
    -d, --device <t>  Target platform: desktop (default) | web | android | ios
                      (aliases: chrome/safari/firefox = web; linux/macos/windows
                      = desktop). Web serves a WebGPU/WebGL2 build with live reload.
    --port <n>        Dev-server port for -d web (default: 8080)
    -p, --package <n> Run a specific workspace member / sample by name
                      (aliases: --example, --bin)
    --watch <dir>     Also watch <dir> for hot-restart (repeatable) — e.g. the
                      framework crates while iterating on a sample
    --release         Build/run optimized (no dev diagnostics)
    --no-reload       Disable hot-restart (build + run once; desktop only)
    -q, --quiet       Only app logs at warn+ (default: debug)
    --log <level>     trace | debug | info | warn | error  (default: debug)
    --log-file [path] Also write the COMPLETE trace to a file (all levels);
                      defaults to <tempdir>/pebbles-<app>.log
    --                Everything after `--` is passed to the app

{BOLD}pebbles build OPTIONS{RESET}  {DIM}(always builds optimized/release){RESET}
    -d, --device <t>  Target: desktop (default) → target/release/<bin>; web →
                      a static wasm bundle in ./dist; android | ios → guidance.
    -p, --package <n> Build a specific workspace member by name
                      (aliases: --example, --bin)

{BOLD}pebbles bundle OPTIONS{RESET}  {DIM}(Linux; metadata from pebbles.toml [app]: name, id, icon){RESET}
    -p, --package <n> Bundle a specific workspace member by name
    --format <f>      appimage (default; falls back to the AppDir when
                      appimagetool is missing) | appdir
                      Output: target/bundle/linux/<bin>/. Needs a Vulkan or GL
                      driver on the target machine.

{BOLD}EXAMPLES{RESET}
    pebbles create hello && cd hello && pebbles run
    pebbles create --template widget my-widget   {DIM}# a reusable widget package{RESET}
    pebbles run -p desktop          {DIM}# run a sample from the workspace root{RESET}
    pebbles run -d web              {DIM}# build to wasm + open in the browser{RESET}
    pebbles run -p counter -d web   {DIM}# run a specific sample on the web{RESET}
    pebbles run -p desktop --watch crates   {DIM}# also hot-restart on framework edits{RESET}
    pebbles run --release           {DIM}# optimized release build{RESET}
    pebbles build                   {DIM}# release binary → target/release/<bin>{RESET}
    pebbles build -d web            {DIM}# static wasm bundle → ./dist (deployable){RESET}
    pebbles bundle -p counter       {DIM}# AppDir/AppImage + .desktop + icons{RESET}
    cd examples/counter && pebbles run
    pebbles run --log trace
    pebbles doctor
"
    );
}

#[cfg(test)]
mod tests {
    use super::{Cmd, classify, pebbles_repo_root};

    #[test]
    fn classify_maps_commands_and_aliases() {
        assert_eq!(classify(Some("new")), Cmd::New);
        assert_eq!(classify(Some("create")), Cmd::New); // alias
        assert_eq!(classify(Some("run")), Cmd::Run);
        assert_eq!(classify(Some("dev")), Cmd::Run); // alias
        assert_eq!(classify(Some("build")), Cmd::Build);
        assert_eq!(classify(Some("bundle")), Cmd::Bundle);
        assert_eq!(classify(Some("doctor")), Cmd::Doctor);
    }

    #[test]
    fn classify_version_and_help_flags() {
        for v in ["--version", "-V", "version"] {
            assert_eq!(classify(Some(v)), Cmd::Version, "{v} is the version command");
        }
        for h in ["--help", "-h", "help"] {
            assert_eq!(classify(Some(h)), Cmd::Help, "{h} is the help command");
        }
        // No argument at all defaults to help.
        assert_eq!(classify(None), Cmd::Help);
    }

    #[test]
    fn classify_unknown_command_carries_the_name() {
        assert_eq!(classify(Some("frobnicate")), Cmd::Unknown("frobnicate".to_string()));
    }

    #[test]
    fn repo_root_is_two_levels_above_the_cli_manifest() {
        // <repo>/crates/pebbles-cli is the manifest dir; the root is two parents up,
        // so joining back down reaches this crate again.
        let root = pebbles_repo_root();
        assert_eq!(root.join("crates").join("pebbles-cli"), std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    }
}
