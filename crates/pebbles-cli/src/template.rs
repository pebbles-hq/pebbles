//! The project templates `pebbles create` writes, and the tiny renderer that
//! fills them in.
//!
//! Templates are real files under `templates/<kind>/`, embedded into the binary
//! at compile time with `include_str!`. Two reasons they are files rather than
//! string literals in the scaffolding code: a template you can open, read and
//! run is far easier to keep correct than one spliced together with `format!`,
//! and adding a kind stops meaning "grow the command".
//!
//! A `.tmpl` extension keeps `templates/*/Cargo.toml.tmpl` from being mistaken
//! for a real manifest by cargo (and by editors/linters walking the tree).
//!
//! ### Placeholders
//!
//! * `{{name}}` — the project name as given (`my-widget`)
//! * `{{name_snake}}` — the same, underscored, for Rust paths (`my_widget`)
//! * `{{renderer}}` — the chosen default render backend feature
//!   (`vello-hybrid` | `vello`)
//! * `{{dep.<crate>}}` — a Cargo dependency line for a Pebbles crate, resolved
//!   to either a git or a path source (see [`Source`])
//! * `{{dep_nodefault.<crate>}}` — the same, but with `default-features = false`
//!   (so the app's own `default`/`vello`/`vello-hybrid` features drive the backend)
//!
//! An unknown placeholder is left untouched rather than silently blanked, so a
//! typo shows up in the generated file instead of vanishing.

/// Where a generated project should get the Pebbles crates from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The published git repository — the default, and the only form that works
    /// on a machine that has no Pebbles checkout.
    Git,
    /// The local checkout this CLI was built from. For working *on* the
    /// framework: edits to Pebbles are picked up without publishing.
    Path,
}

pub const GIT_URL: &str = "https://github.com/pebbles-hq/pebbles";

/// One file in a template: its path in the generated project, and its contents.
pub struct File {
    /// Destination path, relative to the project root.
    pub path: &'static str,
    pub contents: &'static str,
}

/// A project kind `pebbles create --template <name>` can scaffold.
pub struct Template {
    pub name: &'static str,
    /// One line, shown by `pebbles create --list`.
    pub summary: &'static str,
    /// What to print after a successful scaffold.
    pub next_steps: &'static [&'static str],
    pub files: &'static [File],
}

/// A runnable desktop application — the default.
const APP: Template = Template {
    name: "app",
    summary: "a runnable desktop app (default)",
    next_steps: &["cd {{name}}", "pebbles run"],
    files: &[
        File { path: "Cargo.toml", contents: include_str!("../templates/app/Cargo.toml.tmpl") },
        File { path: "src/main.rs", contents: include_str!("../templates/app/src/main.rs.tmpl") },
        File { path: "pebbles.toml", contents: include_str!("../templates/app/pebbles.toml.tmpl") },
        File { path: "README.md", contents: include_str!("../templates/app/README.md.tmpl") },
        // Named `gitignore.tmpl` in the source tree: a real `.gitignore` inside
        // the CLI crate would apply to the CLI itself.
        File { path: ".gitignore", contents: include_str!("../templates/app/gitignore.tmpl") },
    ],
};

/// A reusable widget package — the ecosystem shape: a library other people's
/// Pebbles apps depend on, with its own example and headless tests.
const WIDGET: Template = Template {
    name: "widget",
    summary: "a reusable widget package (library + example + tests)",
    next_steps: &["cd {{name}}", "cargo run --example demo", "cargo test"],
    files: &[
        File { path: "Cargo.toml", contents: include_str!("../templates/widget/Cargo.toml.tmpl") },
        File { path: "src/lib.rs", contents: include_str!("../templates/widget/src/lib.rs.tmpl") },
        File {
            path: "examples/demo.rs",
            contents: include_str!("../templates/widget/examples/demo.rs.tmpl"),
        },
        File {
            path: "tests/rendering.rs",
            contents: include_str!("../templates/widget/tests/rendering.rs.tmpl"),
        },
        File { path: "README.md", contents: include_str!("../templates/widget/README.md.tmpl") },
        File { path: ".gitignore", contents: include_str!("../templates/widget/gitignore.tmpl") },
    ],
};

pub const ALL: &[&Template] = &[&APP, &WIDGET];

/// Look a template up by name.
pub fn find(name: &str) -> Option<&'static Template> {
    ALL.iter().copied().find(|t| t.name == name)
}

/// The known template names, for error messages.
pub fn names() -> String {
    ALL.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")
}

/// Substitute the placeholders in `src`.
///
/// `renderer` fills `{{renderer}}` (the default backend feature). `dep` resolves
/// `{{dep.<crate>}}` / `{{dep_nodefault.<crate>}}`; it takes the crate name and a
/// `no_default` flag and returns the full dependency line. Unknown placeholders are
/// left verbatim.
pub fn render(src: &str, name: &str, renderer: &str, dep: &dyn Fn(&str, bool) -> String) -> String {
    let snake = name.replace('-', "_");
    let mut out = String::with_capacity(src.len() + 64);
    let mut rest = src;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            // Unterminated — emit the rest as-is rather than truncating the file.
            out.push_str(&rest[start..]);
            return out;
        };
        let key = after[..end].trim();
        match key {
            "name" => out.push_str(name),
            "name_snake" => out.push_str(&snake),
            "renderer" => out.push_str(renderer),
            k if k.starts_with("dep_nodefault.") => {
                out.push_str(&dep(&k["dep_nodefault.".len()..], true));
            }
            k if k.starts_with("dep.") => out.push_str(&dep(&k["dep.".len()..], false)),
            _ => {
                // Unknown key: keep it literal so the mistake is visible.
                out.push_str("{{");
                out.push_str(key);
                out.push_str("}}");
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dep(c: &str, no_default: bool) -> String {
        if no_default {
            format!("{c} = {{ git = \"{GIT_URL}\", default-features = false }}")
        } else {
            format!("{c} = {{ git = \"{GIT_URL}\" }}")
        }
    }

    #[test]
    fn substitutes_the_known_placeholders() {
        let out = render("{{name}} / {{name_snake}} / {{renderer}}", "my-widget", "vello", &dep);
        assert_eq!(out, "my-widget / my_widget / vello");
    }

    #[test]
    fn resolves_dependency_lines() {
        let out = render("{{dep.pebbles-core}}", "x", "vello-hybrid", &dep);
        assert_eq!(out, format!("pebbles-core = {{ git = \"{GIT_URL}\" }}"));
    }

    #[test]
    fn resolves_no_default_dependency_lines() {
        let out = render("{{dep_nodefault.pebbles}}", "x", "vello-hybrid", &dep);
        assert_eq!(out, format!("pebbles = {{ git = \"{GIT_URL}\", default-features = false }}"));
    }

    #[test]
    fn leaves_unknown_placeholders_visible() {
        // Silently blanking a typo would ship a broken file that looks fine.
        assert_eq!(render("a {{nope}} b", "x", "vello-hybrid", &dep), "a {{nope}} b");
    }

    #[test]
    fn survives_an_unterminated_placeholder() {
        assert_eq!(render("head {{name", "x", "vello-hybrid", &dep), "head {{name");
    }

    #[test]
    fn every_template_is_well_formed() {
        for t in ALL {
            assert!(!t.files.is_empty(), "{} has files", t.name);
            assert!(t.files.iter().any(|f| f.path == "Cargo.toml"), "{} generates a manifest", t.name);
            for f in t.files {
                assert!(!f.contents.is_empty(), "{}/{} is not empty", t.name, f.path);
                // Every placeholder in every template must be one the renderer
                // knows — otherwise it reaches the generated project verbatim.
                let rendered = render(f.contents, "demo-name", "vello-hybrid", &dep);
                assert!(
                    !rendered.contains("{{"),
                    "{}/{} has an unresolved placeholder:\n{rendered}",
                    t.name,
                    f.path
                );
            }
        }
    }
}
