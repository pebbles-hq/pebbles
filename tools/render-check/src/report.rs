//! Diff / side-by-side images and the `report.md` / `report.json` writers (§6.10, §6.12).

use std::fmt::Write as _;
use std::path::Path;

use crate::CheckError;
use crate::case::{Category, Slice};
use crate::metric::{Metrics, Rgba8, SmokeMetrics, combined_mask, delta, luma};
use crate::status::Status;
use crate::thresholds::Limits;

/// Run-level header data.
#[derive(Clone, Debug, Default)]
pub struct RunInfo {
    pub git_rev: String,
    pub platform_tag: String,
    pub reference_mode: String,
    pub smoke: bool,
    pub tolerance: String,
    pub adapter_name: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
    pub driver_info: String,
    /// MSAA sample count, when the candidate exposes it.
    pub msaa: Option<u32>,
}

/// One finished case.
#[derive(Clone, Debug)]
pub struct CaseResult {
    pub name: String,
    pub slice: Slice,
    pub category: Category,
    pub status: Status,
    /// `"pass"` or `"xfail(<card>)"`.
    pub expect: String,
    /// The xfail card, if any.
    pub xfail_until: Option<&'static str>,
    pub device_size: (u32, u32),
    pub scale: f64,
    pub metrics: Option<Metrics>,
    pub limits: Option<Limits>,
    /// `(probe, None = ok | Some(message))`.
    pub probes: Vec<(&'static str, Option<String>)>,
    /// Non-zero `label=count` pairs of the candidate's `FrameStats::unsupported`.
    pub fallbacks: Vec<(&'static str, u32)>,
    pub smoke: Option<SmokeMetrics>,
    /// Panic message / GPU error / why a case is unjudged.
    pub note: String,
    /// Artifact directory (relative to the report), when written.
    pub artifacts: Option<String>,
    pub millis_reference: u128,
    pub millis_candidate: u128,
}

/// Gray for `delta ≤ T`, red for over-threshold edges, magenta for interior failures.
pub fn diff_image(r: &Rgba8, c: &Rgba8, l: &Limits) -> Rgba8 {
    let mask = combined_mask(r, c);
    let mut out = Rgba8::filled(r.width, r.height, [0, 0, 0, 255]);
    for y in 0..r.height {
        for x in 0..r.width {
            let i = (y * r.width + x) as usize;
            let (a, b) = (r.px(x, y), c.px(x, y));
            let d = delta(a, b);
            let px = if !mask[i] && d > l.max_interior {
                [255, 0, 255, 255]
            } else if d > l.over_threshold {
                [(128 + u32::from(d)).min(255) as u8, 0, 0, 255]
            } else {
                let g = (luma(a) * 3 / 10 + 179).min(255) as u8;
                [g, g, g, 255]
            };
            out.set(x, y, px);
        }
    }
    out
}

/// `reference | candidate | diff`, width `3w`.
pub fn side_by_side(r: &Rgba8, c: &Rgba8, d: &Rgba8) -> Rgba8 {
    let (w, h) = (r.width, r.height);
    let mut out = Rgba8::filled(w * 3, h, [0, 0, 0, 255]);
    for (k, img) in [r, c, d].into_iter().enumerate() {
        for y in 0..h.min(img.height) {
            for x in 0..w.min(img.width) {
                out.set(k as u32 * w + x, y, img.px(x, y));
            }
        }
    }
    out
}

/// JSON string escaping for `"`, `\` and control characters.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn md_cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

fn f(v: f64) -> String {
    if v.is_finite() { format!("{v:.4}") } else { "null".to_string() }
}

fn totals(results: &[CaseResult]) -> Vec<(Status, usize)> {
    Status::ALL.iter().map(|s| (*s, results.iter().filter(|r| r.status == *s).count())).collect()
}

/// Render `report.md`.
pub fn report_md(info: &RunInfo, results: &[CaseResult]) -> String {
    let mut s = String::new();
    let msaa = info.msaa.map_or("unknown".to_string(), |n| n.to_string());
    let _ = writeln!(
        s,
        "reference={} smoke={} candidate=pebbles-gpu adapter={} | {} | {} | {} {} msaa={msaa} tolerance={} git={}\n",
        info.reference_mode,
        if info.smoke { "on" } else { "off" },
        info.adapter_name,
        info.backend,
        info.device_type,
        info.driver,
        info.driver_info,
        info.tolerance,
        info.git_rev
    );
    s.push_str("| status | cases |\n|---|---|\n");
    for (st, n) in totals(results) {
        let _ = writeln!(s, "| {} | {n} |", st.label());
    }
    s.push_str("\n| slice | PASS | FAIL | XFAIL | XPASS | other |\n|---|---|---|---|---|---|\n");
    let mut slices: Vec<Slice> = results.iter().map(|r| r.slice).collect();
    slices.sort();
    slices.dedup();
    for sl in slices {
        let of = |st: Status| results.iter().filter(|r| r.slice == sl && r.status == st).count();
        let all = results.iter().filter(|r| r.slice == sl).count();
        let (p, fl, xf, xp) = (of(Status::Pass), of(Status::Fail), of(Status::Xfail), of(Status::Xpass));
        let _ = writeln!(s, "| {sl:?} | {p} | {fl} | {xf} | {xp} | {} |", all - p - fl - xf - xp);
    }
    s.push_str("\n| card | cases |\n|---|---|\n");
    let mut cards: Vec<&str> = results.iter().filter_map(|r| r.xfail_until).collect();
    cards.sort_unstable();
    cards.dedup();
    for card in cards {
        let names: Vec<&str> =
            results.iter().filter(|r| r.xfail_until == Some(card)).map(|r| r.name.as_str()).collect();
        let _ = writeln!(s, "| {card} | {} |", names.join(", "));
    }
    s.push_str("\n| case | fallbacks |\n|---|---|\n");
    for r in results.iter().filter(|r| !r.fallbacks.is_empty()) {
        let keys: Vec<String> = r.fallbacks.iter().map(|(k, n)| format!("{k}={n}")).collect();
        let _ = writeln!(s, "| {} | {} |", r.name, keys.join(" "));
    }
    s.push_str("\n| case | smoke | max_tile_delta | coverage_iou |\n|---|---|---|---|\n");
    for r in results {
        if let Some(sm) = r.smoke {
            let v = if sm.passes() { "PASS" } else { "FAIL" };
            let _ = writeln!(s, "| {} | {v} | {:.2} | {:.4} |", r.name, sm.max_tile_delta, sm.coverage_iou);
        }
    }
    s.push_str(
        "\n| case | slice | category | status | max_interior | pct_over | mae | limits | artifacts | note |\n\
         |---|---|---|---|---|---|---|---|---|---|\n",
    );
    let mut bad: Vec<&CaseResult> = results.iter().filter(|r| r.status != Status::Pass).collect();
    let pct = |r: &CaseResult| r.metrics.map_or(-1.0, |m| m.pct_over);
    bad.sort_by(|a, b| a.slice.cmp(&b.slice).then(pct(b).total_cmp(&pct(a))));
    for r in bad {
        let (mi, po, mae) = r.metrics.map_or(("-".into(), "-".into(), "-".into()), |m| {
            (m.max_interior_delta.to_string(), f(m.pct_over), f(m.mae))
        });
        let lim = r.limits.map_or("-".to_string(), |l| {
            format!("T{} {}% i{} mae{}", l.over_threshold, l.max_pct_over, l.max_interior, l.max_mae)
        });
        let _ = writeln!(
            s,
            "| {} | {:?} | {:?} | {} | {mi} | {po} | {mae} | {lim} | {} | {} |",
            r.name,
            r.slice,
            r.category,
            r.status.label(),
            r.artifacts.as_deref().unwrap_or("-"),
            md_cell(&r.note)
        );
    }
    s
}

/// Render `report.json` (schema 1, §6.12).
pub fn report_json(info: &RunInfo, results: &[CaseResult]) -> String {
    let e = |v: &str| json_escape(v);
    let mut s = String::new();
    let _ = write!(
        s,
        "{{\n  \"schema\": 1,\n  \"git_rev\": \"{}\", \"platform_tag\": \"{}\", \"reference_mode\": \"{}\", \"smoke\": {}, \"tolerance\": \"{}\",\n",
        e(&info.git_rev),
        e(&info.platform_tag),
        e(&info.reference_mode),
        info.smoke,
        e(&info.tolerance)
    );
    let _ = writeln!(
        s,
        "  \"adapter\": {{ \"name\": \"{}\", \"backend\": \"{}\", \"device_type\": \"{}\", \"driver\": \"{}\", \"driver_info\": \"{}\" }},",
        e(&info.adapter_name),
        e(&info.backend),
        e(&info.device_type),
        e(&info.driver),
        e(&info.driver_info)
    );
    let tot: Vec<String> = totals(results).iter().map(|(st, n)| format!("\"{}\": {n}", st.label())).collect();
    let _ = writeln!(s, "  \"totals\": {{ {} }},\n  \"cases\": [", tot.join(", "));
    for (i, r) in results.iter().enumerate() {
        let metrics = r.metrics.map_or("null".to_string(), |m| {
            format!(
                "{{ \"max_delta\": {}, \"max_interior_delta\": {}, \"pct_over\": {}, \"mae\": {}, \"over_threshold\": {} }}",
                m.max_delta,
                m.max_interior_delta,
                f(m.pct_over),
                f(m.mae),
                m.over_count
            )
        });
        let limits = r.limits.map_or("null".to_string(), |l| {
            format!(
                "{{ \"max_pct_over\": {}, \"max_interior\": {}, \"max_mae\": {} }}",
                l.max_pct_over, l.max_interior, l.max_mae
            )
        });
        let probes: Vec<String> = r
            .probes
            .iter()
            .map(|(n, m)| match m {
                None => format!("{{ \"name\": \"{}\", \"ok\": true }}", e(n)),
                Some(m) => format!("{{ \"name\": \"{}\", \"ok\": false, \"message\": \"{}\" }}", e(n), e(m)),
            })
            .collect();
        let fb: Vec<String> = r.fallbacks.iter().map(|(k, n)| format!("\"{}\": {n}", e(k))).collect();
        let smoke = r.smoke.map_or("null".to_string(), |sm| {
            format!(
                "{{ \"pass\": {}, \"max_tile_delta\": {}, \"coverage_iou\": {} }}",
                sm.passes(),
                f(sm.max_tile_delta),
                f(sm.coverage_iou)
            )
        });
        let _ = write!(
            s,
            "    {{ \"name\": \"{}\", \"slice\": \"{:?}\", \"category\": \"{:?}\", \"status\": \"{}\", \"expect\": \"{}\",\n      \"device_size\": [{}, {}], \"scale\": {},\n      \"metrics\": {metrics},\n      \"limits\": {limits},\n      \"probes\": [{}], \"fallbacks\": {{{}}}, \"smoke\": {smoke}, \"note\": \"{}\", \"millis_reference\": {}, \"millis_candidate\": {} }}{}\n",
            e(&r.name),
            r.slice,
            r.category,
            r.status.label(),
            e(&r.expect),
            r.device_size.0,
            r.device_size.1,
            r.scale,
            probes.join(", "),
            fb.join(", "),
            e(&r.note),
            r.millis_reference,
            r.millis_candidate,
            if i + 1 == results.len() { "" } else { "," }
        );
    }
    s.push_str("  ]\n}\n");
    s
}

fn write(path: &Path, body: &str) -> Result<(), CheckError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| CheckError::Io(format!("{}: {e}", dir.display())))?;
    }
    std::fs::write(path, body).map_err(|e| CheckError::Io(format!("{}: {e}", path.display())))
}

/// Write `<dir>/report.md`.
pub fn write_report_md(dir: &Path, info: &RunInfo, results: &[CaseResult]) -> Result<(), CheckError> {
    write(&dir.join("report.md"), &report_md(info, results))
}

/// Write `<dir>/report.json`.
pub fn write_report_json(dir: &Path, info: &RunInfo, results: &[CaseResult]) -> Result<(), CheckError> {
    write(&dir.join("report.json"), &report_json(info, results))
}

#[cfg(test)]
#[path = "report_tests.rs"]
mod tests;
