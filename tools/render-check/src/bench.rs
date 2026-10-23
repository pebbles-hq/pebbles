//! `render-check bench` (10 §11.2, M21): hand-rolled timing of the §11.1 scenes on the wgpu
//! renderer. Informative only — CI never gates on it (P8).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pebbles_gpu::OffscreenRenderer;
use pebbles_gpu::testing::bench_scenes::{BENCH_CASES, COLD_CASES};
use pebbles_render::paint::{Painter, Scene};

/// Parsed `bench` options.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchArgs {
    pub cases: Vec<String>,
    pub frames: usize,
    pub warmup: usize,
    pub out: PathBuf,
}

pub const USAGE: &str = "\
usage: render-check bench [--cases a,b,c] [--frames 300] [--warmup 30]
                          [--out target/render-check/bench]";

/// Parse the arguments after `bench`.
pub fn parse(mut it: impl Iterator<Item = String>) -> Result<BenchArgs, String> {
    let mut a = BenchArgs {
        cases: Vec::new(),
        frames: 300,
        warmup: 30,
        out: PathBuf::from("target/render-check/bench"),
    };
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--cases" => a.cases = value()?.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect(),
            "--frames" => a.frames = value()?.parse().map_err(|e| format!("--frames: {e}"))?,
            "--warmup" => a.warmup = value()?.parse().map_err(|e| format!("--warmup: {e}"))?,
            "--out" => a.out = PathBuf::from(value()?),
            "-h" | "--help" => return Err("help".into()),
            other => return Err(format!("unknown bench option {other}")),
        }
    }
    if a.frames == 0 {
        return Err("--frames must be > 0".into());
    }
    for c in &a.cases {
        if !BENCH_CASES.iter().any(|(n, ..)| n == c) {
            return Err(format!("unknown bench case {c}"));
        }
    }
    Ok(a)
}

/// Nearest-rank percentile of a sorted sample (10 §11.2).
pub fn pct(sorted: &[u64], p: usize) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    sorted[((sorted.len() - 1) * p + 50) / 100]
}

/// One `(case, backend, metric)` row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub case: String,
    pub backend: &'static str,
    pub metric: &'static str,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
    pub frames: usize,
    /// wgpu recorded fallbacks: the feature has not landed (C17).
    pub partial: bool,
}

fn rows(
    case: &str,
    backend: &'static str,
    samples: [(&'static str, Vec<u64>); 3],
    partial: bool,
) -> Vec<Row> {
    samples
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(metric, mut v)| {
            v.sort_unstable();
            Row {
                case: case.to_owned(),
                backend,
                metric,
                p50: pct(&v, 50),
                p95: pct(&v, 95),
                max: v.last().copied().unwrap_or(0),
                frames: v.len(),
                partial,
            }
        })
        .collect()
}

fn target(device: &wgpu::Device, (w, h): (u32, u32)) -> (wgpu::Texture, wgpu::TextureView) {
    let t = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("check.bench.target"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let v = t.create_view(&wgpu::TextureViewDescriptor::default());
    (t, v)
}

fn us(d: Duration) -> u64 {
    u64::try_from(d.as_micros()).unwrap_or(u64::MAX)
}

fn wait(device: &wgpu::Device, idx: wgpu::SubmissionIndex) -> Result<(), String> {
    device
        .poll(wgpu::PollType::Wait { submission_index: Some(idx), timeout: Some(Duration::from_secs(5)) })
        .map(|_| ())
        .map_err(|e| format!("poll: {e}"))
}

fn bench_wgpu(
    r: &mut OffscreenRenderer,
    name: &str,
    size: (u32, u32),
    scene: &Scene,
    a: &BenchArgs,
) -> Result<Vec<Row>, String> {
    let device = r.device().clone();
    let (_t, view) = target(&device, size);
    let cold = COLD_CASES.contains(&name);
    let n = if cold { a.frames.min(20) } else { a.frames };
    let (mut cpu, mut frame, mut gpu, mut partial) = (Vec::new(), Vec::new(), Vec::new(), false);
    for i in 0..a.warmup + n {
        if cold {
            r.reset_renderer();
        }
        let t0 = Instant::now();
        let idx = r.render_only(scene, &view, peniko::Color::WHITE).map_err(|e| format!("{name}: {e}"))?;
        let t1 = Instant::now();
        wait(&device, idx)?;
        let t2 = Instant::now();
        if i >= a.warmup {
            let s = r.last_frame_stats();
            partial |= s.fallbacks > 0;
            cpu.push(us(t1 - t0));
            frame.push(us(t2 - t0));
            if let Some(g) = s.gpu_us {
                gpu.push(u64::from(g));
            }
        }
    }
    Ok(rows(name, "wgpu", [("cpu", cpu), ("frame", frame), ("gpu", gpu)], partial))
}

fn tsv(rows: &[Row], backend: &str) -> String {
    let mut s = String::from("case\tbackend\tmetric\tp50_us\tp95_us\tmax_us\tframes\n");
    for r in rows.iter().filter(|r| r.backend == backend) {
        let _ = writeln!(
            s,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            r.case, r.backend, r.metric, r.p50, r.p95, r.max, r.frames
        );
    }
    s
}

/// The markdown summary: one row per case and metric.
pub fn summary(rows: &[Row]) -> String {
    let mut s = String::from(
        "# render-check bench\n\n| case | metric | p50 µs | p95 µs | max µs | note |\n|---|---|---|---|---|---|\n",
    );
    for r in rows {
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} |",
            r.case,
            r.metric,
            r.p50,
            r.p95,
            r.max,
            if r.partial { "partial" } else { "" }
        );
    }
    s
}

/// Run the bench; the process exit code (0 informative, 2 errors).
pub fn run(a: &BenchArgs) -> u8 {
    let selected: Vec<_> =
        BENCH_CASES.iter().filter(|(n, ..)| a.cases.is_empty() || a.cases.iter().any(|c| c == n)).collect();
    let mut all = Vec::new();
    {
        let mut r = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(e) => {
                eprintln!("render-check bench: wgpu: {e}");
                return 2;
            }
        };
        for (name, size, f) in &selected {
            let mut scene = Scene::new();
            f(&mut Painter::new(&mut scene));
            match bench_wgpu(&mut r, name, *size, &scene, a) {
                Ok(rows) => all.extend(rows),
                Err(e) => {
                    eprintln!("render-check bench: {e}");
                    return 2;
                }
            }
        }
    }
    if let Err(e) = std::fs::create_dir_all(&a.out) {
        eprintln!("render-check bench: {}: {e}", a.out.display());
        return 2;
    }
    let files = vec![("summary.md".to_owned(), summary(&all)), ("wgpu.tsv".to_owned(), tsv(&all, "wgpu"))];
    for (f, body) in files {
        if let Err(e) = std::fs::write(a.out.join(&f), body) {
            eprintln!("render-check bench: {f}: {e}");
            return 2;
        }
    }
    for r in &all {
        println!(
            "{:28} {:6} {:5} p50 {:>8} µs  p95 {:>8} µs{}",
            r.case,
            r.backend,
            r.metric,
            r.p50,
            r.p95,
            if r.partial { "  partial" } else { "" }
        );
    }
    println!("render-check bench: {} rows; summary: {}", all.len(), a.out.join("summary.md").display());
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_is_nearest_rank() {
        let v: Vec<u64> = (1..=100).collect();
        assert_eq!((pct(&v, 50), pct(&v, 95), pct(&v, 100)), (51, 95, 100));
        assert_eq!(pct(&[7], 95), 7);
    }

    #[test]
    fn parse_defaults_and_errors() {
        let a = parse(std::iter::empty()).expect("defaults");
        assert_eq!((a.frames, a.warmup), (300, 30));
        let a = parse(["--cases", "bench/rects_10k", "--frames", "20"].map(String::from).into_iter())
            .expect("args");
        assert_eq!((a.cases.len(), a.frames), (1, 20));
        assert!(parse(["--cases", "bench/nope"].map(String::from).into_iter()).is_err());
    }

    #[test]
    fn summary_lists_every_row() {
        let row = Row {
            case: "bench/x".into(),
            backend: "wgpu",
            metric: "frame",
            p50: 300,
            p95: 310,
            max: 320,
            frames: 1,
            partial: true,
        };
        assert!(summary(&[row]).contains("| bench/x | frame | 300 | 310 | 320 | partial |"));
    }
}
