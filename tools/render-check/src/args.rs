//! Hand-rolled CLI parser (§6.4). No `clap` — same style as `pebbles-cli`.

use std::path::PathBuf;

/// What judges op-level cases (`--reference`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefMode {
    /// The golden model (and probes / self-goldens per case).
    Golden,
    /// Every `in_refs` case against the committed self-goldens in `--reference-dir`.
    Dir,
}

impl RefMode {
    /// The report label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Golden => "golden",
            Self::Dir => "dir",
        }
    }
}

/// Threshold set (§8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tolerance {
    /// Linux + lavapipe.
    Canonical,
    /// Non-Linux lanes.
    Relaxed,
}

impl Tolerance {
    /// The report label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Relaxed => "relaxed",
        }
    }
}

/// Parsed command line.
#[derive(Clone, Debug, PartialEq)]
pub struct Args {
    /// Output root.
    pub out: PathBuf,
    /// Name prefixes to run (OR); empty = all.
    pub only: Vec<String>,
    /// Name prefixes to skip (applied after `only`).
    pub skip: Vec<String>,
    /// Print the case table and exit.
    pub list: bool,
    /// Reference mode.
    pub reference: RefMode,
    /// Also render the transitional vello-hybrid smoke image.
    pub smoke: bool,
    /// Committed self-goldens.
    pub reference_dir: PathBuf,
    /// Threshold set.
    pub tolerance: Tolerance,
    /// Output subdir / report label; `None` = env or `<os>-<adapter-slug>`.
    pub platform_tag: Option<String>,
    /// Render every candidate twice and require byte equality.
    pub determinism: bool,
    /// Write PNGs for passing cases too.
    pub keep_images: bool,
    /// Write candidate images of passing `in_refs` cases here (V12).
    pub write_refs: Option<PathBuf>,
    /// Print usage and exit 0.
    pub help: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            out: PathBuf::from("target/render-check"),
            only: Vec::new(),
            skip: Vec::new(),
            list: false,
            reference: RefMode::Golden,
            smoke: true,
            reference_dir: PathBuf::from("tools/render-check/refs/wgpu/linux-lavapipe"),
            tolerance: Tolerance::Canonical,
            platform_tag: None,
            determinism: false,
            keep_images: false,
            write_refs: None,
            help: false,
        }
    }
}

impl Args {
    /// Whether `name` survives `--only` / `--skip`.
    pub fn selects(&self, name: &str) -> bool {
        let included = self.only.is_empty() || self.only.iter().any(|p| name.starts_with(p.as_str()));
        included && !self.skip.iter().any(|p| name.starts_with(p.as_str()))
    }
}

/// Usage text (`-h`).
pub const USAGE: &str = "\
usage: render-check [options]
  --out <DIR>                     output root (default target/render-check)
  --only <PREFIX>                 run cases whose name starts with PREFIX (repeatable)
  --skip <PREFIX>                 skip cases with PREFIX (repeatable, after --only)
  --list                          print the case table and exit (no GPU)
  --reference <golden|dir>        spec reference mode (default golden)
  --smoke <on|off>                transitional vello-hybrid smoke check (default on)
  --reference-dir <DIR>           committed self-goldens
  --tolerance <canonical|relaxed> threshold set (default canonical)
  --platform-tag <TAG>            output subdir and report label
  --determinism                   render every candidate twice; differences are UNSTABLE
  --keep-images                   also write PNGs for passing cases
  --write-refs <DIR>              write passing in_refs candidates (llvmpipe only)
  -h, --help                      this text
exit: 0 pass/xfail, 1 any failure, 2 usage or I/O error, 3 no GPU adapter";

/// Parse the arguments after the program name.
pub fn parse(args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args;
    while let Some(flag) = it.next() {
        let mut value = |f: &str| it.next().ok_or_else(|| format!("{f} needs a value"));
        match flag.as_str() {
            "--out" => out.out = PathBuf::from(value("--out")?),
            "--only" => out.only.push(value("--only")?),
            "--skip" => out.skip.push(value("--skip")?),
            "--list" => out.list = true,
            "--reference" => {
                out.reference = match value("--reference")?.as_str() {
                    "golden" => RefMode::Golden,
                    "dir" => RefMode::Dir,
                    v => return Err(format!("--reference: expected golden|dir, got `{v}`")),
                }
            }
            "--smoke" => {
                out.smoke = match value("--smoke")?.as_str() {
                    "on" => true,
                    "off" => false,
                    v => return Err(format!("--smoke: expected on|off, got `{v}`")),
                }
            }
            "--reference-dir" => out.reference_dir = PathBuf::from(value("--reference-dir")?),
            "--tolerance" => {
                out.tolerance = match value("--tolerance")?.as_str() {
                    "canonical" => Tolerance::Canonical,
                    "relaxed" => Tolerance::Relaxed,
                    v => return Err(format!("--tolerance: expected canonical|relaxed, got `{v}`")),
                }
            }
            "--platform-tag" => out.platform_tag = Some(value("--platform-tag")?),
            "--determinism" => out.determinism = true,
            "--keep-images" => out.keep_images = true,
            "--write-refs" => out.write_refs = Some(PathBuf::from(value("--write-refs")?)),
            "-h" | "--help" => out.help = true,
            other => return Err(format!("unknown flag `{other}`")),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(v: &[&str]) -> Result<Args, String> {
        parse(v.iter().map(|s| (*s).to_string()))
    }

    #[test]
    fn parses_defaults() {
        let a = p(&[]).unwrap();
        assert_eq!(a, Args::default());
        assert_eq!(a.out, PathBuf::from("target/render-check"));
        assert_eq!(a.reference, RefMode::Golden);
        assert!(a.smoke);
        assert_eq!(a.tolerance, Tolerance::Canonical);
        assert!(!a.list && !a.determinism && !a.keep_images);
        assert!(a.selects("anything/at_all"));
    }

    #[test]
    fn parses_repeatable_only() {
        let a = p(&["--only", "prim/", "--only", "stroke/", "--skip", "stroke/caps"]).unwrap();
        assert_eq!(a.only, ["prim/", "stroke/"]);
        assert!(a.selects("prim/rect_aligned"));
        assert!(a.selects("stroke/joins"));
        assert!(!a.selects("stroke/caps_s125"));
        assert!(!a.selects("clip/rect_scissor"));
    }

    #[test]
    fn parses_every_valued_flag() {
        let a = p(&[
            "--out",
            "o",
            "--reference",
            "dir",
            "--smoke",
            "off",
            "--reference-dir",
            "r",
            "--tolerance",
            "relaxed",
            "--platform-tag",
            "t",
            "--write-refs",
            "w",
            "--list",
            "--determinism",
            "--keep-images",
        ])
        .unwrap();
        assert_eq!(a.reference, RefMode::Dir);
        assert!(!a.smoke && a.list && a.determinism && a.keep_images);
        assert_eq!(a.tolerance, Tolerance::Relaxed);
        assert_eq!(a.platform_tag.as_deref(), Some("t"));
        assert_eq!(a.write_refs, Some(PathBuf::from("w")));
    }

    #[test]
    fn rejects_unknown_flag() {
        assert!(p(&["--frobnicate"]).unwrap_err().contains("--frobnicate"));
    }

    #[test]
    fn rejects_missing_value() {
        assert!(p(&["--out"]).unwrap_err().contains("--out"));
        assert!(p(&["--only"]).is_err());
        assert!(p(&["--smoke", "maybe"]).is_err());
    }
}
