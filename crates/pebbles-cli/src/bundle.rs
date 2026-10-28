//! `pebbles bundle` — package a release build for Linux desktops.
//!
//! Produces, under `target/bundle/linux/<bin>/`:
//!
//! * `<bin>.AppDir/` — a complete AppDir: `AppRun`, `usr/bin/<bin>`, the freedesktop
//!   `.desktop` entry in `usr/share/applications/<id>.desktop`, hicolor icons in
//!   `usr/share/icons/hicolor/<size>/apps/<id>.<ext>`, plus the top-level `.desktop` /
//!   icon / `.DirIcon` copies AppImage expects.
//! * `<Name>-<arch>.AppImage` — when `appimagetool` is on `PATH`. Nothing is downloaded;
//!   without it the AppDir is left ready and the one finishing command is printed.
//!
//! App metadata comes from `pebbles.toml`:
//!
//! ```toml
//! [app]
//! name = "Counter"                 # human name (Name= in the .desktop file)
//! id = "dev.pebbles.Counter"       # reverse-DNS app id: .desktop file name + window app id
//! icon = "assets/icon.png"         # PNG or SVG, relative to pebbles.toml (optional)
//! comment = "A tiny counter"       # optional
//! categories = "Utility;"          # optional freedesktop categories
//! ```
//!
//! The launcher exports `PEBBLES_APP_ID=<id>`, which the shell applies as the Wayland
//! `app_id` / X11 `WM_CLASS`, so the dock matches the window to the `.desktop` file.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::run::{cargo_build, find_workspace_root, resolve_member, target_bin, tool_available};
use crate::term;

/// Runtime requirements printed after a bundle (and in the README).
pub const RUNTIME_DEPS: &str = "\
  runtime deps on the target machine (not bundled — they come from the system):
    - a GPU driver: Vulkan (mesa-vulkan-drivers, or the NVIDIA/AMD vendor driver);
      OpenGL ES 3 / GL 3.3 via Mesa is used as a fallback
    - Wayland (libwayland-client) or X11 (libX11, libxcb, libXcursor, libXi, libXrandr)
    - libxkbcommon (keyboard layouts), plus libxkbcommon-x11 on X11; fontconfig fonts are optional
    (a minimal container needs libxcursor1 libxi6 libxkbcommon-x11-0 on Ubuntu,
    libXcursor libXi libxkbcommon-x11 on Fedora; desktop installs ship them)";

#[derive(Debug, PartialEq, Eq)]
struct BundleOpts {
    package: Option<String>,
    /// Stop at the AppDir even when `appimagetool` is available.
    appdir_only: bool,
}

fn parse_args(args: &[String]) -> Result<BundleOpts, String> {
    let mut o = BundleOpts { package: None, appdir_only: false };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-p" | "--package" | "--example" | "--bin" => {
                o.package = Some(it.next().ok_or("`-p` needs a package name")?.clone());
            }
            "--format" => match it.next().map(String::as_str) {
                Some("appimage") => o.appdir_only = false,
                Some("appdir") => o.appdir_only = true,
                Some(v) => return Err(format!("unknown format `{v}` — use appimage | appdir")),
                None => return Err("`--format` needs a value: appimage | appdir".into()),
            },
            s if s.starts_with('-') => return Err(format!("unknown option `{s}` for `pebbles bundle`")),
            s => {
                return Err(format!(
                    "unexpected argument `{s}` — pebbles bundle takes no positional args (did you mean `-p {s}`?)"
                ));
            }
        }
    }
    Ok(o)
}

/// App metadata from `pebbles.toml` `[app]`, with defaults filled in.
#[derive(Debug, PartialEq, Eq)]
struct AppMeta {
    name: String,
    id: String,
    icon: Option<PathBuf>,
    comment: Option<String>,
    categories: String,
}

/// `key = "value"` inside `[section]` of a TOML text (naive, dep-free; mirrors
/// `run`'s pebbles.toml readers). `None` when absent or empty.
fn toml_get(text: &str, section: &str, key: &str) -> Option<String> {
    let header = format!("[{section}]");
    let mut inside = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            inside = l == header;
            continue;
        }
        if !inside || l.starts_with('#') {
            continue;
        }
        let Some((k, v)) = l.split_once('=') else { continue };
        if k.trim() != key {
            continue;
        }
        // Drop a trailing comment outside the quotes, then the quotes.
        let v = v.trim();
        let v = match v.strip_prefix('"') {
            Some(rest) => rest.split('"').next().unwrap_or(""),
            None => v.split('#').next().unwrap_or("").trim(),
        };
        return (!v.is_empty()).then(|| v.to_string());
    }
    None
}

/// A reverse-DNS id: ≥ 2 dot-separated segments of `[A-Za-z0-9_-]`, none empty or
/// starting with a digit (the freedesktop desktop-file-id / D-Bus name rules).
fn valid_app_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 2
        && id.len() <= 255
        && parts.iter().all(|p| {
            !p.is_empty()
                && !p.starts_with(|c: char| c.is_ascii_digit())
                && p.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        })
}

fn read_meta(project: &Path, bin: &str) -> Result<AppMeta, String> {
    let text = std::fs::read_to_string(project.join("pebbles.toml")).unwrap_or_default();
    meta_from(&text, project, bin)
}

fn meta_from(text: &str, project: &Path, bin: &str) -> Result<AppMeta, String> {
    let name = toml_get(text, "app", "name").unwrap_or_else(|| bin.to_string());
    let id = toml_get(text, "app", "id").unwrap_or_else(|| format!("dev.pebbles.{}", bin.replace('-', "_")));
    if !valid_app_id(&id) {
        return Err(format!(
            "pebbles.toml [app] id `{id}` is not a reverse-DNS id (e.g. `org.example.MyApp`)"
        ));
    }
    let icon = toml_get(text, "app", "icon").map(|p| project.join(p));
    if let Some(p) = &icon {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !p.is_file() {
            return Err(format!("pebbles.toml [app] icon `{}` not found", p.display()));
        }
        if ext != "png" && ext != "svg" {
            return Err(format!("[app] icon must be a .png or .svg (got `{}`)", p.display()));
        }
    }
    let mut categories = toml_get(text, "app", "categories").unwrap_or_else(|| "Utility;".into());
    if !categories.ends_with(';') {
        categories.push(';');
    }
    Ok(AppMeta { name, id, icon, comment: toml_get(text, "app", "comment"), categories })
}

/// The freedesktop `.desktop` entry. `StartupWMClass` = the app id the shell sets as
/// `WM_CLASS` / Wayland `app_id`, so docks group the window under this entry.
fn desktop_entry(m: &AppMeta, exec: &str) -> String {
    let mut s = String::from("[Desktop Entry]\nType=Application\n");
    s += &format!("Name={}\n", m.name);
    if let Some(c) = &m.comment {
        s += &format!("Comment={c}\n");
    }
    s += &format!("Exec={exec}\nIcon={}\nTerminal=false\nCategories={}\n", m.id, m.categories);
    s += &format!("StartupWMClass={}\n", m.id);
    s
}

/// Width/height from a PNG's IHDR chunk (`None` if not a PNG).
fn png_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
    Some((w, h))
}

/// A placeholder scalable icon (the app's initial on a rounded square) for apps
/// without `[app] icon` — AppImage requires one.
fn placeholder_svg(name: &str) -> String {
    let initial: String = name.chars().find(|c| c.is_alphanumeric()).unwrap_or('P').to_uppercase().collect();
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"256\" height=\"256\" viewBox=\"0 0 256 256\">\
<rect width=\"256\" height=\"256\" rx=\"56\" fill=\"#18181b\"/>\
<text x=\"128\" y=\"170\" font-family=\"sans-serif\" font-size=\"140\" font-weight=\"bold\" \
text-anchor=\"middle\" fill=\"#fafafa\">{initial}</text></svg>\n"
    )
}

fn launcher(bin: &str, id: &str) -> String {
    format!(
        "#!/bin/sh\n# Generated by `pebbles bundle`.\nHERE=\"$(dirname \"$(readlink -f \"$0\")\")\"\n\
export PEBBLES_APP_ID=\"${{PEBBLES_APP_ID:-{id}}}\"\nexec \"$HERE/usr/bin/{bin}\" \"$@\"\n"
    )
}

fn write(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, contents)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}
#[cfg(not(unix))]
fn make_executable(_: &Path) -> std::io::Result<()> {
    Ok(())
}

/// Lay out the AppDir at `appdir` (replacing any previous one).
fn write_appdir(appdir: &Path, bin_path: &Path, bin: &str, m: &AppMeta) -> std::io::Result<()> {
    if appdir.exists() {
        std::fs::remove_dir_all(appdir)?;
    }
    let usr_bin = appdir.join("usr/bin").join(bin);
    write(&usr_bin, &std::fs::read(bin_path)?)?;
    make_executable(&usr_bin)?;

    let apprun = appdir.join("AppRun");
    write(&apprun, launcher(bin, &m.id).as_bytes())?;
    make_executable(&apprun)?;

    let desktop = desktop_entry(m, bin);
    write(&appdir.join("usr/share/applications").join(format!("{}.desktop", m.id)), desktop.as_bytes())?;
    write(&appdir.join(format!("{}.desktop", m.id)), desktop.as_bytes())?;

    // Icon: the hicolor theme slot (by PNG size, or scalable for SVG) + the top-level copies.
    let (bytes, ext, size_dir) = match &m.icon {
        Some(p) if p.extension().is_some_and(|e| e == "png") => {
            let b = std::fs::read(p)?;
            let (w, h) = png_size(&b).ok_or_else(|| std::io::Error::other("icon is not a valid PNG"))?;
            if w != h {
                term::warn(&format!("icon is {w}x{h}; desktop icons should be square"));
            }
            (b, "png", format!("{w}x{h}"))
        }
        Some(p) => (std::fs::read(p)?, "svg", "scalable".to_string()),
        None => (placeholder_svg(&m.name).into_bytes(), "svg", "scalable".to_string()),
    };
    let icon_name = format!("{}.{ext}", m.id);
    write(&appdir.join("usr/share/icons/hicolor").join(size_dir).join("apps").join(&icon_name), &bytes)?;
    write(&appdir.join(&icon_name), &bytes)?;
    write(&appdir.join(".DirIcon"), &bytes)?;
    Ok(())
}

fn arch() -> &'static str {
    std::env::consts::ARCH
}

pub fn run(args: &[String]) -> ExitCode {
    let o = match parse_args(args) {
        Ok(o) => o,
        Err(e) => {
            term::error(&e);
            return ExitCode::FAILURE;
        }
    };
    if !cfg!(target_os = "linux") {
        term::error("`pebbles bundle` packages for Linux (AppImage) and must run on Linux.");
        return ExitCode::FAILURE;
    }

    let cwd = std::env::current_dir().unwrap_or_default();
    let workspace_root = find_workspace_root(&cwd);
    let Some((member_dir, bin)) =
        resolve_member(&cwd, workspace_root.as_deref(), o.package.as_deref(), "bundle")
    else {
        return ExitCode::FAILURE;
    };
    let meta = match read_meta(&member_dir, &bin) {
        Ok(m) => m,
        Err(e) => {
            term::error(&e);
            return ExitCode::FAILURE;
        }
    };
    let target_root = workspace_root.unwrap_or_else(|| member_dir.clone());

    term::banner(&format!("pebbles bundle — {} ({})", meta.name, meta.id));
    if toml_get(&std::fs::read_to_string(member_dir.join("pebbles.toml")).unwrap_or_default(), "app", "id")
        .is_none()
    {
        term::warn(&format!(
            "no [app] id in pebbles.toml — using `{}`; set your own reverse-DNS id",
            meta.id
        ));
    }
    if meta.icon.is_none() {
        term::warn("no [app] icon in pebbles.toml — using a generated placeholder");
    }

    term::step("building an optimized release binary…");
    if !cargo_build(&target_root, &bin, true) {
        term::error("build failed — fix the errors above.");
        return ExitCode::FAILURE;
    }
    let bin_path = target_bin(&target_root, &bin, true);
    let out =
        bin_path.parent().and_then(Path::parent).unwrap_or(&target_root).join("bundle/linux").join(&bin);
    let appdir = out.join(format!("{bin}.AppDir"));
    if let Err(e) = write_appdir(&appdir, &bin_path, &bin, &meta) {
        term::error(&format!("could not write the AppDir: {e}"));
        return ExitCode::FAILURE;
    }
    term::ok(&format!("AppDir: {}", appdir.display()));
    println!("  desktop entry: usr/share/applications/{}.desktop", meta.id);
    println!("  icons:         usr/share/icons/hicolor/…/apps/{}.*", meta.id);
    println!("  try it:        {}/AppRun", appdir.display());

    let image = out.join(format!("{}-{}.AppImage", meta.name.replace(' ', "_"), arch()));
    let finish = format!("ARCH={} appimagetool {} {}", arch(), appdir.display(), image.display());
    if o.appdir_only {
        println!("  to make an AppImage: {finish}");
    } else if tool_available("appimagetool") {
        term::step("packing the AppImage with appimagetool…");
        let ok = Command::new("appimagetool")
            .env("ARCH", arch())
            .arg(&appdir)
            .arg(&image)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            term::error("appimagetool failed — see its output above.");
            return ExitCode::FAILURE;
        }
        term::ok(&format!("AppImage: {}", image.display()));
    } else {
        term::warn("appimagetool not found — stopping at the AppDir (nothing is downloaded).");
        println!("  get it from https://github.com/AppImage/appimagetool/releases, then run:");
        println!("    {finish}");
    }
    println!("{RUNTIME_DEPS}");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn parse_flags() {
        assert_eq!(parse_args(&[]).unwrap(), BundleOpts { package: None, appdir_only: false });
        let o = parse_args(&argv(&["-p", "counter", "--format", "appdir"])).unwrap();
        assert_eq!(o, BundleOpts { package: Some("counter".into()), appdir_only: true });
        assert!(parse_args(&argv(&["--format", "deb"])).is_err());
        assert!(parse_args(&argv(&["counter"])).is_err());
        assert!(parse_args(&argv(&["-p"])).is_err());
    }

    #[test]
    fn toml_get_reads_only_the_named_section() {
        let t = "[dev]\nname = \"nope\"\n[app]\n# id = \"x.y\"\nname = \"My App\" # trailing\nid = \"org.example.MyApp\"\n";
        assert_eq!(toml_get(t, "app", "name").as_deref(), Some("My App"));
        assert_eq!(toml_get(t, "app", "id").as_deref(), Some("org.example.MyApp"));
        assert_eq!(toml_get(t, "dev", "name").as_deref(), Some("nope"));
        assert_eq!(toml_get(t, "app", "icon"), None);
    }

    #[test]
    fn app_id_validation() {
        assert!(valid_app_id("org.example.MyApp"));
        assert!(valid_app_id("dev.pebbles.my_app"));
        assert!(!valid_app_id("counter"));
        assert!(!valid_app_id("org..x"));
        assert!(!valid_app_id("org.1bad"));
        assert!(!valid_app_id("org.ex ample"));
    }

    #[test]
    fn meta_defaults_and_desktop_entry() {
        let m = meta_from("", Path::new("."), "my-app").unwrap();
        assert_eq!(m.name, "my-app");
        assert_eq!(m.id, "dev.pebbles.my_app");
        assert!(meta_from("[app]\nid = \"bad\"\n", Path::new("."), "x").is_err());

        let m = meta_from(
            "[app]\nname = \"Counter\"\nid = \"org.example.Counter\"\ncategories = \"Utility\"\n",
            Path::new("."),
            "counter",
        )
        .unwrap();
        let d = desktop_entry(&m, "counter");
        assert!(d.starts_with("[Desktop Entry]\nType=Application\n"));
        for line in [
            "Name=Counter",
            "Exec=counter",
            "Icon=org.example.Counter",
            "Categories=Utility;",
            "StartupWMClass=org.example.Counter",
        ] {
            assert!(d.lines().any(|l| l == line), "{line} in\n{d}");
        }
    }

    #[test]
    fn png_size_reads_ihdr() {
        let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
        b.extend_from_slice(&256u32.to_be_bytes());
        b.extend_from_slice(&128u32.to_be_bytes());
        assert_eq!(png_size(&b), Some((256, 128)));
        assert_eq!(png_size(b"not a png at all, no sir....."), None);
    }

    #[test]
    fn appdir_layout() {
        let dir = std::env::temp_dir().join(format!("pebbles-bundle-test-{}", std::process::id()));
        let bin = dir.join("fakebin");
        write(&bin, b"#!/bin/sh\n").unwrap();
        let m = meta_from("[app]\nname = \"Fake\"\nid = \"org.example.Fake\"\n", &dir, "fakebin").unwrap();
        let appdir = dir.join("fakebin.AppDir");
        write_appdir(&appdir, &bin, "fakebin", &m).unwrap();
        for p in [
            "AppRun",
            "usr/bin/fakebin",
            "usr/share/applications/org.example.Fake.desktop",
            "usr/share/icons/hicolor/scalable/apps/org.example.Fake.svg",
            "org.example.Fake.desktop",
            "org.example.Fake.svg",
            ".DirIcon",
        ] {
            assert!(appdir.join(p).is_file(), "{p}");
        }
        let apprun = std::fs::read_to_string(appdir.join("AppRun")).unwrap();
        assert!(apprun.contains("PEBBLES_APP_ID:-org.example.Fake"));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
