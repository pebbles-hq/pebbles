//! `pebbles create <name>` — scaffold a Pebbles project from a template.
//!
//! The templates themselves (and the placeholder renderer) live in
//! [`crate::template`]; this module is only argument parsing and file writing.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::pebbles_repo_root;
use crate::template::{self, GIT_URL, Source, Template};
use crate::term;

pub fn run(args: &[String]) -> ExitCode {
    let mut name: Option<&str> = None;
    let mut kind = "app";
    let mut source = Source::Git; // portable by default
    let mut list = false;
    // The app's DEFAULT render backend feature. Both are always wired; this only
    // picks which one a plain `pebbles run` / `cargo run` uses.
    let mut renderer = "vello-hybrid";

    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--list" => list = true,
            // Point the generated project at a local Pebbles checkout instead of
            // git — for working ON the framework.
            "--path" => source = Source::Path,
            // Explicit opt-in to the default, so scripts can be unambiguous.
            "--git" => source = Source::Git,
            "-t" | "--template" => match it.next() {
                Some(t) => kind = t.as_str(),
                None => {
                    term::error("`--template` needs a value");
                    return ExitCode::FAILURE;
                }
            },
            s if s.starts_with("--template=") => {
                kind = &s["--template=".len()..];
            }
            "-r" | "--renderer" | "--backend" => match it.next() {
                Some(v) => match parse_renderer(v) {
                    Some(r) => renderer = r,
                    None => {
                        term::error(&format!("unknown renderer `{v}` — use: hybrid | vello"));
                        return ExitCode::FAILURE;
                    }
                },
                None => {
                    term::error("`--renderer` needs a value: hybrid | vello");
                    return ExitCode::FAILURE;
                }
            },
            s if s.starts_with("--renderer=") => match parse_renderer(&s["--renderer=".len()..]) {
                Some(r) => renderer = r,
                None => {
                    term::error("unknown renderer — use: hybrid | vello");
                    return ExitCode::FAILURE;
                }
            },
            s if s.starts_with('-') => {
                term::error(&format!("unknown option `{s}` for `pebbles create`"));
                return ExitCode::FAILURE;
            }
            s => name = Some(s),
        }
    }

    if list {
        print_templates();
        return ExitCode::SUCCESS;
    }

    let Some(template) = template::find(kind) else {
        term::error(&format!("unknown template `{kind}` (available: {})", template::names()));
        return ExitCode::FAILURE;
    };

    let Some(name) = name else {
        term::error("usage: pebbles create [--template <kind>] <name>");
        return ExitCode::FAILURE;
    };
    if let Err(why) = validate_crate_name(name) {
        term::error(&format!("`{name}` isn't a usable crate name: {why}"));
        return ExitCode::FAILURE;
    }

    let dir = Path::new(name);
    if dir.exists() {
        term::error(&format!("`{name}` already exists"));
        return ExitCode::FAILURE;
    }

    // Resolve the dependency source ONCE, up front, so a missing local checkout
    // fails before any file is written rather than half-way through.
    let root = match source {
        Source::Path => match local_crates_dir() {
            Ok(p) => Some(p),
            Err(why) => {
                term::error(&why);
                return ExitCode::FAILURE;
            }
        },
        Source::Git => None,
    };
    let dep = move |crate_name: &str, no_default: bool| {
        let feats = if no_default { ", default-features = false" } else { "" };
        match &root {
            Some(crates) => {
                let p = crates.join(crate_name);
                format!("{crate_name} = {{ path = {:?}{feats} }}", p.display().to_string())
            }
            None => format!("{crate_name} = {{ git = {GIT_URL:?}{feats} }}"),
        }
    };

    term::banner(&format!("Creating Pebbles {} `{name}` ({renderer})", template.name));

    if let Err(code) = write_template(template, dir, name, renderer, &dep) {
        // Best-effort cleanup: a half-written project is worse than none.
        let _ = fs::remove_dir_all(dir);
        return code;
    }

    term::ok(&format!("`{name}` is ready."));
    println!();
    println!("  Next:");
    for step in template.next_steps {
        println!("    {}", step.replace("{{name}}", name));
    }
    println!();
    ExitCode::SUCCESS
}

fn write_template(
    template: &Template,
    dir: &Path,
    name: &str,
    renderer: &str,
    dep: &dyn Fn(&str, bool) -> String,
) -> Result<(), ExitCode> {
    for file in template.files {
        let path = dir.join(file.path);
        if let Some(parent) = path.parent()
            && let Err(e) = fs::create_dir_all(parent)
        {
            term::error(&format!("could not create {}: {e}", parent.display()));
            return Err(ExitCode::FAILURE);
        }
        let contents = template::render(file.contents, name, renderer, dep);
        if let Err(e) = fs::write(&path, contents) {
            term::error(&format!("could not write {}: {e}", path.display()));
            return Err(ExitCode::FAILURE);
        }
        term::step(&format!("created {name}/{}", file.path));
    }
    Ok(())
}

fn print_templates() {
    use term::{BOLD, CYAN, DIM, RESET};
    println!("{BOLD}TEMPLATES{RESET}");
    for t in template::ALL {
        println!("    {CYAN}{:<8}{RESET} {DIM}{}{RESET}", t.name, t.summary);
    }
    println!();
    println!("    pebbles create --template widget my-widget");
}

/// The `crates/` directory of the Pebbles checkout this CLI was built from.
///
/// `--path` is only meaningful when that checkout is still present — a CLI
/// installed with `cargo install` on another machine has a build-time path that
/// no longer exists, and a generated project would fail to resolve its
/// dependencies with a confusing cargo error. Check it here and say so plainly.
fn local_crates_dir() -> Result<PathBuf, String> {
    let crates = pebbles_repo_root().join("crates");
    if !crates.join("pebbles").join("Cargo.toml").exists() {
        return Err(format!(
            "`--path` needs the Pebbles checkout this CLI was built from, but {} is gone.\n         \
             Re-run without `--path` to depend on {GIT_URL} instead.",
            crates.display()
        ));
    }
    Ok(crates)
}

/// Map a `--renderer` value to the cargo feature the app should default to.
/// Accepts friendly synonyms; returns `None` for anything unknown.
fn parse_renderer(s: &str) -> Option<&'static str> {
    match s.to_ascii_lowercase().as_str() {
        "hybrid" | "vello-hybrid" | "vello_hybrid" | "default" => Some("vello-hybrid"),
        "vello" | "compute" | "classic" => Some("vello"),
        _ => None,
    }
}

/// Cargo's rules, plus the ones that only bite later: a name that is a Rust
/// keyword makes `use <name>::…` unusable in the generated example and tests.
fn validate_crate_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("it is empty".into());
    }
    if !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return Err("it must start with a letter".into());
    }
    if let Some(bad) = name.chars().find(|c| !c.is_ascii_alphanumeric() && *c != '_' && *c != '-') {
        return Err(format!("`{bad}` is not allowed (use letters, digits, `_`, `-`)"));
    }
    // The crate name becomes a Rust path (`{{name_snake}}`) in the generated
    // example and tests, so a keyword would produce code that cannot compile.
    const KEYWORDS: &[&str] = &[
        "as", "async", "await", "box", "break", "const", "continue", "crate", "dyn", "else", "enum",
        "extern", "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
        "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "type", "union",
        "unsafe", "use", "where", "while", "yield", "test", "gen", "try", "macro", "override", "priv",
        "typeof", "unsized", "virtual", "become", "abstract", "do", "final",
    ];
    let snake = name.replace('-', "_");
    if KEYWORDS.contains(&snake.as_str()) {
        return Err(format!("`{snake}` is a Rust keyword, so `use {snake}::…` would not compile"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_crate_name;

    #[test]
    fn accepts_ordinary_names() {
        for n in ["hello", "my-widget", "app2", "a_b-c"] {
            assert!(validate_crate_name(n).is_ok(), "{n} should be accepted");
        }
    }

    #[test]
    fn rejects_the_shapes_that_break_later() {
        // Cargo-level rules…
        assert!(validate_crate_name("").is_err());
        assert!(validate_crate_name("2fast").is_err(), "must start with a letter");
        assert!(validate_crate_name("my widget").is_err(), "no spaces");
        assert!(validate_crate_name("my.widget").is_err(), "no dots");
        // …and the one that only bites when the generated code is compiled.
        assert!(validate_crate_name("type").is_err(), "keyword");
        assert!(validate_crate_name("my-type").is_ok(), "keyword only matters whole");
        assert!(validate_crate_name("impl").is_err(), "keyword");
    }
}
