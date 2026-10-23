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
    /// `--tier-a off`: tessellate Tier A shapes (pre-R10 baseline, debug).
    pub tier_a: bool,
    /// Write candidate images of passing `in_refs` cases here (V12).
    pub write_refs: Option<PathBuf>,
    /// `--glyph-hint auto|on|off` (R27): `None` = the 02 §7.8 policy.
    pub glyph_hint: Option<bool>,
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
            reference_dir: PathBuf::from("tools/render-check/refs/wgpu/linux-lavapipe"),
            tolerance: Tolerance::Canonical,
            platform_tag: None,
            determinism: false,
            keep_images: false,
            tier_a: true,
            write_refs: None,
            glyph_hint: None,
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
       render-check bench --help   (10 §11.2 timing mode, M21)
  --out <DIR>                     output root (default target/render-check)
  --only <PREFIX>                 run cases whose name starts with PREFIX (repeatable)
  --skip <PREFIX>                 skip cases with PREFIX (repeatable, after --only)
  --list                          print the case table and exit (no GPU)
  --reference <golden|dir>        spec reference mode (default golden)
  --reference-dir <DIR>           committed self-goldens
  --tolerance <canonical|relaxed> threshold set (default canonical)
  --glyph-hint <auto|on|off>  glyph hinting (default auto = the 02 §7.8 policy)
  --platform-tag <TAG>            output subdir and report label
  --determinism                   render every candidate twice; differences are UNSTABLE
  --tier-a on|off                 off: tessellate Tier A shapes (pre-R10 baseline, debug)
  --keep-images                   also write PNGs for passing cases
  --write-refs <DIR>              write passing (and probe-clean self-golden) in_refs candidates (llvmpipe only)
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
            "--reference-dir" => out.reference_dir = PathBuf::from(value("--reference-dir")?),
            "--tolerance" => {
                out.tolerance = match value("--tolerance")?.as_str() {
                    "canonical" => Tolerance::Canonical,
                    "relaxed" => Tolerance::Relaxed,
                    v => return Err(format!("--tolerance: expected canonical|relaxed, got `{v}`")),
                }
            }
            "--glyph-hint" => {
                out.glyph_hint = match value("--glyph-hint")?.as_str() {
                    "auto" => None,
                    "on" => Some(true),
                    "off" => Some(false),
                    v => return Err(format!("--glyph-hint: expected auto|on|off, got `{v}`")),
                }
            }
            "--platform-tag" => out.platform_tag = Some(value("--platform-tag")?),
            "--determinism" => out.determinism = true,
            "--keep-images" => out.keep_images = true,
            "--tier-a" => {
                out.tier_a = match value("--tier-a")?.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--tier-a expects on|off, got {other:?}")),
                }
            }
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
        assert!(a.list && a.determinism && a.keep_images);
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
    }
}
