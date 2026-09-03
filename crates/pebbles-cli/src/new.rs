//! `pebbles new <name>` — scaffold a runnable Pebbles app.

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::pebbles_repo_root;
use crate::term;

pub fn run(args: &[String]) -> ExitCode {
    let mut name: Option<&str> = None;
    let mut path_dep = true; // default: point at the local pebbles checkout
    for a in args {
        match a.as_str() {
            "--git" => path_dep = false,
            s if s.starts_with('-') => {
                term::error(&format!("unknown option `{s}` for `pebbles new`"));
                return ExitCode::FAILURE;
            }
            s => name = Some(s),
        }
    }
    let Some(name) = name else {
        term::error("usage: pebbles new <name>");
        return ExitCode::FAILURE;
    };
    if !is_valid_crate_name(name) {
        term::error(&format!(
            "`{name}` isn't a valid crate name (use letters, digits, `_`, `-`; start with a letter)"
        ));
        return ExitCode::FAILURE;
    }

    let dir = Path::new(name);
    if dir.exists() {
        term::error(&format!("`{name}` already exists"));
        return ExitCode::FAILURE;
    }

    term::banner(&format!("Creating Pebbles app `{name}`"));

    // The dependency line: a path to the local checkout (default — builds
    // immediately) or a git dep (with `--git`, portable off this machine).
    let dep_line = if path_dep {
        // The umbrella `pebbles` crate lives at <repo>/crates/pebbles.
        let pkg = pebbles_repo_root().join("crates").join("pebbles");
        format!("pebbles = {{ path = {:?} }}", pkg.display().to_string())
    } else {
        "pebbles = { git = \"https://github.com/pebbles-hq/pebbles\" }".to_string()
    };

    let files: &[(&str, String)] = &[
        ("Cargo.toml", cargo_toml(name, &dep_line)),
        ("src/main.rs", MAIN_RS.to_string()),
        (".gitignore", GITIGNORE.to_string()),
        ("pebbles.toml", pebbles_toml(name)),
        ("README.md", readme(name)),
    ];

    for (rel, contents) in files {
        let path = dir.join(rel);
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            term::error(&format!("could not create {}: {e}", parent.display()));
            return ExitCode::FAILURE;
        }
        if let Err(e) = fs::write(&path, contents) {
            term::error(&format!("could not write {}: {e}", path.display()));
            return ExitCode::FAILURE;
        }
        term::step(&format!("created {name}/{rel}"));
    }

    term::ok(&format!("`{name}` is ready."));
    println!();
    println!("  Next:");
    println!("    cd {name}");
    println!("    pebbles run");
    println!();
    ExitCode::SUCCESS
}

fn is_valid_crate_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn cargo_toml(name: &str, dep_line: &str) -> String {
    format!(
        "\
[package]
name = \"{name}\"
version = \"0.1.0\"
edition = \"2024\"
publish = false

[dependencies]
{dep_line}
"
    )
}

fn pebbles_toml(name: &str) -> String {
    format!(
        "\
# Pebbles project config, read by `pebbles run`.
[app]
name = \"{name}\"

[dev]
# Log level the dev runner starts the app at (trace|debug|info|warn|error).
log = \"debug\"
# Directories watched for hot-restart (relative to this file).
watch = [\"src\"]
"
    )
}

fn readme(name: &str) -> String {
    format!(
        "\
# {name}

A desktop app built with [Pebbles](https://github.com/pebbles-hq/pebbles).

## Develop

```sh
pebbles run          # build + run with rich logs; hot-restarts on save
pebbles run --log trace
```

## Release

```sh
pebbles run --release
# or a plain cargo build:
cargo build --release
```
"
    )
}

const GITIGNORE: &str = "/target\n**/*.rs.bk\nCargo.lock\n.pebbles/\n";

/// The starter app — a counter, mirroring the Pebbles house style: a component is
/// a function, state is a signal, handlers are plain closures.
const MAIN_RS: &str = "\
use pebbles::prelude::*;

fn app() -> impl IntoWidget {
    let count = create_signal(0);

    center(column(children![
        text(\"Welcome to Pebbles\").size(22.0).color(palette::zinc::S600),
        gap_h(16.0),
        text(format!(\"{}\", count.get())).size(72.0).color(palette::zinc::S900),
        gap_h(24.0),
        row(children![
            button(\"\u{2212}\")
                .variant(ButtonVariant::Outline)
                .on_pressed(move || count.update(|c| *c -= 1)),
            gap_w(16.0),
            button(\"+\").on_pressed(move || count.update(|c| *c += 1)),
        ])
        .main_axis_size(MainAxisSize::Min),
    ]))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(app))
        .title(\"Pebbles App\")
        .size(480, 420)
        .background(palette::zinc::S50)
        .run()
}
";
