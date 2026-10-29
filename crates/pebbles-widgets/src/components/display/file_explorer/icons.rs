//! VS Code-style file icon themes: [`IconTheme`] (file names, compound
//! extensions, extensions, language ids, folder names with open/closed
//! variants, root folder, defaults), [`ExplorerIcon`] (a glyph, a decoded
//! SVG/PNG image, or any widget), per-path overrides and a resolver callback
//! that wins over everything. [`IconTheme::default_theme`] is the built-in
//! coloured theme.

use std::collections::HashMap;

#[allow(clippy::wildcard_imports)]
use super::*;

/// What an explorer icon renders as.
#[derive(Clone)]
pub enum ExplorerIcon {
    /// A vector glyph (any bundled tabler icon or your own [`IconData`]);
    /// `None` color = the explorer's icon tint token.
    Glyph(IconData, Option<Color>),
    /// A decoded raster/SVG image (e.g. `image_from_bytes(include_bytes!("rust.png"))`).
    Image(pebbles_render::Image),
    /// Any widget, built at the icon size (logical px).
    Widget(Rc<dyn Fn(f64) -> AnyWidget>),
}

impl std::fmt::Debug for ExplorerIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExplorerIcon::Glyph(_, c) => write!(f, "Glyph(.., {c:?})"),
            ExplorerIcon::Image(_) => f.write_str("Image(..)"),
            ExplorerIcon::Widget(_) => f.write_str("Widget(..)"),
        }
    }
}

impl PartialEq for ExplorerIcon {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ExplorerIcon::Glyph(a, ca), ExplorerIcon::Glyph(b, cb)) => a == b && ca == cb,
            (ExplorerIcon::Widget(a), ExplorerIcon::Widget(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl ExplorerIcon {
    /// A glyph tinted with the explorer's icon colour.
    pub fn glyph(data: IconData) -> Self {
        ExplorerIcon::Glyph(data, None)
    }
    /// A glyph in a fixed colour.
    pub fn colored(data: IconData, color: Color) -> Self {
        ExplorerIcon::Glyph(data, Some(color))
    }
    /// A decoded image (PNG/JPEG via `image_from_bytes`, or a rasterised SVG).
    pub fn image(image: pebbles_render::Image) -> Self {
        ExplorerIcon::Image(image)
    }
    /// Any widget, built at the requested icon size.
    pub fn widget(f: impl Fn(f64) -> AnyWidget + 'static) -> Self {
        ExplorerIcon::Widget(Rc::new(f))
    }

    /// Build the icon widget at `size`, tinting glyphs without their own
    /// colour with `tint`.
    pub fn build(&self, size: f64, tint: Color) -> AnyWidget {
        match self {
            ExplorerIcon::Glyph(d, c) => icon(*d).size(size).color(c.unwrap_or(tint)).into_widget(),
            ExplorerIcon::Image(img) => Container::new()
                .width(size)
                .height(size)
                .decoration(BoxDecoration::new().image(img.clone()))
                .into_widget(),
            ExplorerIcon::Widget(f) => f(size),
        }
    }
}

impl From<IconData> for ExplorerIcon {
    fn from(d: IconData) -> Self {
        ExplorerIcon::glyph(d)
    }
}

/// Maps a file name to a language id (`main.rs` → `rust`).
pub type LanguageResolver = Rc<dyn Fn(&str) -> Option<String>>;

/// A VS Code-style file icon theme. Resolution, per item:
///
/// - **files**: exact file name → longest compound extension (`d.ts`,
///   `test.ts`, `tar.gz`) → extension → language id → the default file icon;
/// - **folders**: exact folder name (separate expanded/collapsed icons) →
///   the root-folder icon (for the workspace root) → the default folder
///   (open/closed).
///
/// Names and extensions match case-insensitively (VS Code lowercases them).
/// Unset slots fall through, ending in the explorer's built-in glyphs.
///
/// ```ignore
/// let theme = IconTheme::default_theme()                // start from the built-in
///     .file_name("justfile", ExplorerIcon::glyph(tabler::TERMINAL_2))
///     .file_extension("d.ts", ExplorerIcon::colored(tabler::FILE_TYPE_TS, palette::SKY.S600))
///     .folder_name("migrations", ExplorerIcon::glyph(tabler::DATABASE), None);
/// explorer.set_icon_theme(theme);
/// ```
#[derive(Clone, Default)]
pub struct IconTheme {
    file: Option<ExplorerIcon>,
    folder: Option<ExplorerIcon>,
    folder_expanded: Option<ExplorerIcon>,
    root_folder: Option<ExplorerIcon>,
    root_folder_expanded: Option<ExplorerIcon>,
    file_names: HashMap<String, ExplorerIcon>,
    file_extensions: HashMap<String, ExplorerIcon>,
    language_ids: HashMap<String, ExplorerIcon>,
    folder_names: HashMap<String, ExplorerIcon>,
    folder_names_expanded: HashMap<String, ExplorerIcon>,
    language_of: Option<LanguageResolver>,
}

impl IconTheme {
    /// An empty theme (everything falls through to the explorer's glyphs).
    pub fn new() -> Self {
        Self::default()
    }

    /// The default file icon.
    #[must_use]
    pub fn file(mut self, icon: impl Into<ExplorerIcon>) -> Self {
        self.file = Some(icon.into());
        self
    }
    /// The default folder icons (`expanded` `None` = same as closed).
    #[must_use]
    pub fn folder(mut self, closed: impl Into<ExplorerIcon>, expanded: Option<ExplorerIcon>) -> Self {
        self.folder = Some(closed.into());
        self.folder_expanded = expanded;
        self
    }
    /// The workspace root folder's icons.
    #[must_use]
    pub fn root_folder(mut self, closed: impl Into<ExplorerIcon>, expanded: Option<ExplorerIcon>) -> Self {
        self.root_folder = Some(closed.into());
        self.root_folder_expanded = expanded;
        self
    }
    /// An exact file name (`Cargo.toml`, `.gitignore`, `Dockerfile`).
    #[must_use]
    pub fn file_name(mut self, name: &str, icon: impl Into<ExplorerIcon>) -> Self {
        self.file_names.insert(name.to_lowercase(), icon.into());
        self
    }
    /// An extension without the leading dot — simple (`rs`) or compound
    /// (`d.ts`, `test.ts`, `tar.gz`; the longest match wins).
    #[must_use]
    pub fn file_extension(mut self, ext: &str, icon: impl Into<ExplorerIcon>) -> Self {
        self.file_extensions.insert(ext.trim_start_matches('.').to_lowercase(), icon.into());
        self
    }
    /// A language id (`rust`, `python`) — consulted after extensions.
    #[must_use]
    pub fn language(mut self, id: &str, icon: impl Into<ExplorerIcon>) -> Self {
        self.language_ids.insert(id.to_string(), icon.into());
        self
    }
    /// An exact folder name with its collapsed and (optionally) expanded icon.
    #[must_use]
    pub fn folder_name(
        mut self,
        name: &str,
        closed: impl Into<ExplorerIcon>,
        expanded: Option<ExplorerIcon>,
    ) -> Self {
        let k = name.to_lowercase();
        self.folder_names.insert(k.clone(), closed.into());
        if let Some(e) = expanded {
            self.folder_names_expanded.insert(k, e);
        }
        self
    }
    /// How file names map to language ids (default: [`language_of`]).
    #[must_use]
    pub fn language_resolver(mut self, f: impl Fn(&str) -> Option<String> + 'static) -> Self {
        self.language_of = Some(Rc::new(f));
        self
    }

    /// Resolve an item's icon from this theme alone (`None` = fall through).
    pub fn resolve(
        &self,
        name: &str,
        is_folder: bool,
        expanded: bool,
        is_root: bool,
    ) -> Option<ExplorerIcon> {
        let lower = name.to_lowercase();
        if is_folder {
            if expanded && let Some(i) = self.folder_names_expanded.get(&lower) {
                return Some(i.clone());
            }
            if let Some(i) = self.folder_names.get(&lower) {
                return Some(i.clone());
            }
            if is_root {
                let r = if expanded {
                    self.root_folder_expanded.as_ref().or(self.root_folder.as_ref())
                } else {
                    self.root_folder.as_ref()
                };
                if let Some(i) = r {
                    return Some(i.clone());
                }
            }
            return if expanded {
                self.folder_expanded.clone().or_else(|| self.folder.clone())
            } else {
                self.folder.clone()
            };
        }
        if let Some(i) = self.file_names.get(&lower) {
            return Some(i.clone());
        }
        // Extensions, longest first: "a.test.ts" tries "test.ts" then "ts".
        // (A leading dot is a name, not an extension: ".env" has none.)
        let body = lower.strip_prefix('.').unwrap_or(&lower);
        let mut rest = body;
        while let Some(i) = rest.find('.') {
            rest = &rest[i + 1..];
            if let Some(icon) = self.file_extensions.get(rest) {
                return Some(icon.clone());
            }
        }
        let lang = match &self.language_of {
            Some(f) => f(name),
            None => language_of(name).map(str::to_string),
        };
        if let Some(l) = lang
            && let Some(i) = self.language_ids.get(&l)
        {
            return Some(i.clone());
        }
        self.file.clone()
    }

    /// The built-in coloured theme: common languages, config and lock files,
    /// images, archives, `.git*`, `Cargo.toml`, `package.json`, `README`,
    /// `LICENSE`, `Dockerfile`, and `src`/`tests`/`docs`/`target`/
    /// `node_modules`/`.github`/… folders.
    pub fn default_theme() -> Self {
        use pebbles_render::tabler as t;
        let c = |r, g, b| Color::from_rgb8(r, g, b);
        let (rust, blue, yellow, py, go, html, css, green, purple, grey, red, orange, teal, pink) = (
            c(222, 165, 132),
            c(49, 120, 198),
            c(203, 171, 38),
            c(55, 118, 171),
            c(0, 173, 216),
            c(228, 77, 38),
            c(86, 61, 124),
            c(76, 175, 80),
            c(156, 39, 176),
            c(120, 130, 140),
            c(229, 57, 53),
            c(239, 108, 0),
            c(0, 150, 136),
            c(216, 27, 96),
        );
        let g = ExplorerIcon::colored;
        let mut th = IconTheme::new()
            .file(ExplorerIcon::glyph(t::FILE))
            .folder(ExplorerIcon::glyph(t::FOLDER), Some(ExplorerIcon::glyph(t::FOLDER_OPEN)))
            .root_folder(g(t::FOLDER_ROOT, blue), None);
        for (ext, icon) in [
            ("rs", g(t::BRAND_RUST, rust)),
            ("toml", g(t::FILE_SETTINGS, grey)),
            ("md", g(t::MARKDOWN, blue)),
            ("mdx", g(t::MARKDOWN, yellow)),
            ("json", g(t::JSON, yellow)),
            ("jsonc", g(t::JSON, yellow)),
            ("ts", g(t::BRAND_TYPESCRIPT, blue)),
            ("tsx", g(t::BRAND_REACT, blue)),
            ("d.ts", g(t::FILE_TYPE_TS, teal)),
            ("test.ts", g(t::TEST_PIPE, green)),
            ("spec.ts", g(t::TEST_PIPE, green)),
            ("js", g(t::BRAND_JAVASCRIPT, yellow)),
            ("mjs", g(t::BRAND_JAVASCRIPT, yellow)),
            ("cjs", g(t::BRAND_JAVASCRIPT, yellow)),
            ("jsx", g(t::BRAND_REACT, yellow)),
            ("test.js", g(t::TEST_PIPE, green)),
            ("py", g(t::BRAND_PYTHON, py)),
            ("go", g(t::BRAND_GOLANG, go)),
            ("html", g(t::BRAND_HTML5, html)),
            ("htm", g(t::BRAND_HTML5, html)),
            ("css", g(t::BRAND_CSS3, css)),
            ("scss", g(t::BRAND_SASS, pink)),
            ("svelte", g(t::BRAND_SVELTE, html)),
            ("vue", g(t::BRAND_VUE, green)),
            ("c", g(t::FILE_CODE, blue)),
            ("h", g(t::FILE_CODE, purple)),
            ("cpp", g(t::BRAND_CPP, blue)),
            ("cs", g(t::BRAND_C_SHARP, purple)),
            ("kt", g(t::BRAND_KOTLIN, purple)),
            ("swift", g(t::BRAND_SWIFT, orange)),
            ("sh", g(t::TERMINAL_2, green)),
            ("bash", g(t::TERMINAL_2, green)),
            ("zsh", g(t::TERMINAL_2, green)),
            ("yml", g(t::FILE_SETTINGS, purple)),
            ("yaml", g(t::FILE_SETTINGS, purple)),
            ("xml", g(t::FILE_TYPE_XML, orange)),
            ("sql", g(t::DATABASE, pink)),
            ("csv", g(t::FILE_TYPE_CSV, green)),
            ("txt", g(t::FILE_TEXT, grey)),
            ("log", g(t::FILE_TEXT, grey)),
            ("pdf", g(t::FILE_TYPE_PDF, red)),
            ("lock", g(t::LOCK, grey)),
            ("png", g(t::PHOTO, purple)),
            ("jpg", g(t::PHOTO, purple)),
            ("jpeg", g(t::PHOTO, purple)),
            ("gif", g(t::PHOTO, purple)),
            ("webp", g(t::PHOTO, purple)),
            ("ico", g(t::PHOTO, purple)),
            ("svg", g(t::SVG, orange)),
            ("zip", g(t::FILE_ZIP, grey)),
            ("tar", g(t::FILE_ZIP, grey)),
            ("gz", g(t::FILE_ZIP, grey)),
            ("tar.gz", g(t::FILE_ZIP, grey)),
            ("wasm", g(t::FILE_CODE, purple)),
            ("diff", g(t::FILE_DIFF, green)),
            ("patch", g(t::FILE_DIFF, green)),
            ("env", g(t::KEY, yellow)),
            ("pem", g(t::FILE_CERTIFICATE, red)),
            ("mp3", g(t::FILE_MUSIC, pink)),
            ("mp4", g(t::MOVIE, pink)),
        ] {
            th = th.file_extension(ext, icon);
        }
        for (name, icon) in [
            ("Cargo.toml", g(t::PACKAGE, rust)),
            ("Cargo.lock", g(t::LOCK, rust)),
            ("package.json", g(t::BRAND_NPM, red)),
            ("package-lock.json", g(t::LOCK, red)),
            ("yarn.lock", g(t::BRAND_YARN, blue)),
            ("pnpm-lock.yaml", g(t::LOCK, orange)),
            ("tsconfig.json", g(t::FILE_SETTINGS, blue)),
            ("README.md", g(t::INFO_CIRCLE, blue)),
            ("README", g(t::INFO_CIRCLE, blue)),
            ("CHANGELOG.md", g(t::FILE_DESCRIPTION, green)),
            ("LICENSE", g(t::LICENSE, yellow)),
            ("LICENSE.md", g(t::LICENSE, yellow)),
            ("LICENSE-MIT", g(t::LICENSE, yellow)),
            ("LICENSE-APACHE", g(t::LICENSE, yellow)),
            ("Dockerfile", g(t::BRAND_DOCKER, blue)),
            ("docker-compose.yml", g(t::BRAND_DOCKER, blue)),
            (".dockerignore", g(t::BRAND_DOCKER, grey)),
            (".gitignore", g(t::BRAND_GIT, html)),
            (".gitattributes", g(t::BRAND_GIT, html)),
            (".gitmodules", g(t::BRAND_GIT, html)),
            (".editorconfig", g(t::SETTINGS, grey)),
            (".env", g(t::KEY, yellow)),
            ("Makefile", g(t::TERMINAL_2, orange)),
            ("rust-toolchain.toml", g(t::BRAND_RUST, rust)),
        ] {
            th = th.file_name(name, icon);
        }
        for (name, closed, open) in [
            ("src", g(t::FOLDER_CODE, blue), None),
            ("lib", g(t::FOLDER_CODE, blue), None),
            ("tests", g(t::FOLDER, green), Some(g(t::FOLDER_OPEN, green))),
            ("test", g(t::FOLDER, green), Some(g(t::FOLDER_OPEN, green))),
            ("__tests__", g(t::FOLDER, green), Some(g(t::FOLDER_OPEN, green))),
            ("docs", g(t::FOLDER, blue), Some(g(t::FOLDER_OPEN, blue))),
            ("examples", g(t::FOLDER_STAR, yellow), None),
            ("assets", g(t::FOLDER, purple), Some(g(t::FOLDER_OPEN, purple))),
            ("target", g(t::FOLDER, grey), Some(g(t::FOLDER_OPEN, grey))),
            ("dist", g(t::FOLDER, grey), Some(g(t::FOLDER_OPEN, grey))),
            ("build", g(t::FOLDER, grey), Some(g(t::FOLDER_OPEN, grey))),
            ("node_modules", g(t::FOLDER, green), Some(g(t::FOLDER_OPEN, green))),
            (".git", g(t::FOLDER, html), Some(g(t::FOLDER_OPEN, html))),
            (".github", g(t::BRAND_GITHUB, grey), None),
            (".vscode", g(t::BRAND_VSCODE, blue), None),
            ("config", g(t::FOLDER_COG, grey), None),
            (".cargo", g(t::FOLDER_COG, rust), None),
        ] {
            th = th.folder_name(name, closed, open);
        }
        for (lang, icon) in [("rust", g(t::BRAND_RUST, rust)), ("python", g(t::BRAND_PYTHON, py))] {
            th = th.language(lang, icon);
        }
        th
    }
}

/// The built-in file-name → language id map (by extension, plus a few
/// well-known names).
pub fn language_of(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    match lower.as_str() {
        "dockerfile" => return Some("dockerfile"),
        "makefile" => return Some("makefile"),
        _ => {}
    }
    let ext = lower.rsplit_once('.').map(|(_, e)| e)?;
    Some(match ext {
        "rs" => "rust",
        "py" | "pyi" => "python",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "typescriptreact",
        "js" | "mjs" | "cjs" => "javascript",
        "jsx" => "javascriptreact",
        "go" => "go",
        "c" | "h" => "c",
        "cc" | "cpp" | "cxx" | "hpp" => "cpp",
        "cs" => "csharp",
        "java" => "java",
        "kt" | "kts" => "kotlin",
        "swift" => "swift",
        "rb" => "ruby",
        "php" => "php",
        "lua" => "lua",
        "sh" | "bash" | "zsh" => "shellscript",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "json" => "json",
        "jsonc" => "jsonc",
        "toml" => "toml",
        "yml" | "yaml" => "yaml",
        "md" | "markdown" => "markdown",
        "xml" => "xml",
        "sql" => "sql",
        _ => return None,
    })
}

impl FileExplorer {
    /// Install an icon theme (see [`IconTheme`]; [`IconTheme::default_theme`]
    /// is the built-in coloured one). Rows re-render.
    pub fn set_icon_theme(&self, theme: IconTheme) {
        self.icon_theme.set(Some(Rc::new(theme)));
    }

    /// Back to the plain built-in glyphs (no theme).
    pub fn clear_icon_theme(&self) {
        self.icon_theme.set(None);
    }

    /// A resolver that wins over everything (per-path overrides, per-node
    /// icons, the theme): return `Some` for full custom logic, `None` to fall
    /// through. Gets the entry and whether a folder renders expanded.
    pub fn set_icon_resolver(&self, f: impl Fn(&ExplorerEntry, bool) -> Option<ExplorerIcon> + 'static) {
        self.icon_resolver.set(Some(Rc::new(f)));
    }

    /// Remove the resolver.
    pub fn clear_icon_resolver(&self) {
        self.icon_resolver.set(None);
    }

    /// Override the icon of one specific path (filesystem mode) — `None`
    /// removes the override. Wins over the theme and per-node icons.
    pub fn set_path_icon(&self, path: impl Into<PathBuf>, icon: Option<ExplorerIcon>) {
        let path = path.into();
        self.path_icons.update(|m| match icon {
            Some(i) => {
                m.insert(path, i);
            }
            None => {
                m.remove(&path);
            }
        });
    }

    /// Resolve a node's icon: resolver → per-path override → per-node
    /// [`FsNode::icon`] → icon theme → built-in (open/closed folder, file).
    pub fn resolved_icon(&self, node: &FsNode, expanded: bool) -> ExplorerIcon {
        if let Some(r) = self.icon_resolver.get()
            && let Some(e) = self.entry(node.id)
            && let Some(i) = r(&e, expanded)
        {
            return i;
        }
        if !self.path_icons.get().is_empty()
            && let Some(p) = self.path_of(node.id)
            && let Some(i) = self.path_icons.get().get(&p)
        {
            return i.clone();
        }
        if let Some(d) = node.icon {
            return ExplorerIcon::Glyph(d, node.color);
        }
        let is_folder = node.kind == FsKind::Folder;
        if let Some(theme) = self.icon_theme.get()
            && let Some(i) = theme.resolve(&node.name, is_folder, expanded, false)
        {
            return match (i, node.color) {
                (ExplorerIcon::Glyph(d, None), Some(c)) => ExplorerIcon::Glyph(d, Some(c)),
                (i, _) => i,
            };
        }
        let d = if is_folder {
            if expanded { tabler::FOLDER_OPEN } else { IconKind::Folder.data() }
        } else {
            IconKind::File.data()
        };
        ExplorerIcon::Glyph(d, node.color)
    }

    /// The opened root folder's icon (for an app's root header row).
    pub fn root_icon(&self, expanded: bool) -> ExplorerIcon {
        let name = self
            .fs_root
            .get()
            .and_then(|r| r.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        self.icon_theme.get().and_then(|t| t.resolve(&name, true, expanded, true)).unwrap_or(
            ExplorerIcon::Glyph(if expanded { tabler::FOLDER_OPEN } else { IconKind::Folder.data() }, None),
        )
    }
}
