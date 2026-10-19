//! `render-check bench` (10 §11.2, M21): hand-rolled timing of the §11.1 scenes on wgpu and on
//! the transitional vello-hybrid path, each on its own device in this process (DR-09).
//! Informative only — CI never gates on it (P8); `--max-ratio` is for reference machines (M10).

use std::fmt::Write as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use pebbles_gpu::testing::bench_scenes::{BENCH_CASES, COLD_CASES};
use pebbles_gpu::{GpuContext, GpuInit, OffscreenRenderer};
use pebbles_render::paint::{Painter, Scene};

use crate::reference::HybridReference;

/// Which backends run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Wgpu,
    Hybrid,
    Both,
}

/// Parsed `bench` options.
#[derive(Clone, Debug, PartialEq)]
pub struct BenchArgs {
    pub cases: Vec<String>,
    pub frames: usize,
    pub warmup: usize,
    pub backend: Backend,
    pub out: PathBuf,
    pub max_ratio: Option<f64>,
}

pub const USAGE: &str = "\
usage: render-check bench [--cases a,b,c] [--frames 300] [--warmup 30]
                          [--backend wgpu|hybrid|both] [--out target/render-check/bench] [--max-ratio R]";

/// Parse the arguments after `bench`.
pub fn parse(mut it: impl Iterator<Item = String>) -> Result<BenchArgs, String> {
    let mut a = BenchArgs {
        cases: Vec::new(),
        frames: 300,
        warmup: 30,
        backend: Backend::Both,
        out: PathBuf::from("target/render-check/bench"),
        max_ratio: None,
    };
    while let Some(flag) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{flag} needs a value"));
        match flag.as_str() {
            "--cases" => a.cases = value()?.split(',').filter(|s| !s.is_empty()).map(str::to_owned).collect(),
            "--frames" => a.frames = value()?.parse().map_err(|e| format!("--frames: {e}"))?,
            "--warmup" => a.warmup = value()?.parse().map_err(|e| format!("--warmup: {e}"))?,
            "--backend" => {
                a.backend = match value()?.as_str() {
                    "wgpu" => Backend::Wgpu,
                    "hybrid" => Backend::Hybrid,
                    "both" => Backend::Both,
                    v => return Err(format!("--backend: unknown {v}")),
                }
            }
            "--out" => a.out = PathBuf::from(value()?),
            "-h" | "--help" => return Err("help".into()),
            "--max-ratio" => a.max_ratio = Some(value()?.parse().map_err(|e| format!("--max-ratio: {e}"))?),
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

fn bench_hybrid(
    h: &mut HybridReference,
    device: &wgpu::Device,
    name: &str,
    size: (u32, u32),
    scene: &Scene,
    a: &BenchArgs,
) -> Result<Vec<Row>, String> {
    let (_t, view) = target(device, size);
    let cold = COLD_CASES.contains(&name);
    let n = if cold { a.frames.min(20) } else { a.frames };
    h.clear_images();
    let (mut cpu, mut frame) = (Vec::new(), Vec::new());
    for i in 0..a.warmup + n {
        let t0 = Instant::now();
        let idx = h
            .render_only(scene, &view, size, peniko::Color::WHITE, cold)
            .map_err(|e| format!("{name}: {e}"))?;
        let t1 = Instant::now();
        wait(device, idx)?;
        let t2 = Instant::now();
        if i >= a.warmup {
            cpu.push(us(t1 - t0));
            frame.push(us(t2 - t0));
        }
    }
    Ok(rows(name, "hybrid", [("cpu", cpu), ("frame", frame), ("gpu", Vec::new())], false))
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

/// `ratio = wgpu_p50 / hybrid_p50` per case and metric; `(markdown, worst compared cpu/frame ratio)`.
pub fn summary(rows: &[Row]) -> (String, f64) {
    let mut s = String::from(
        "# render-check bench\n\n| case | metric | wgpu p50 µs | hybrid p50 µs | ratio | note |\n|---|---|---|---|---|---|\n",
    );
    let mut worst = 0.0f64;
    for w in rows.iter().filter(|r| r.backend == "wgpu") {
        let h = rows.iter().find(|r| r.backend == "hybrid" && r.case == w.case && r.metric == w.metric);
        #[allow(clippy::cast_precision_loss)] // µs counts far below 2^52
        let ratio = h.filter(|h| h.p50 > 0).map(|h| w.p50 as f64 / h.p50 as f64);
        if let Some(r) = ratio
            && !w.partial
            && matches!(w.metric, "cpu" | "frame")
        {
            worst = worst.max(r);
        }
        let _ = writeln!(
            s,
            "| {} | {} | {} | {} | {} | {} |",
            w.case,
            w.metric,
            w.p50,
            h.map_or_else(|| "—".to_owned(), |h| h.p50.to_string()),
            ratio.map_or_else(|| "—".to_owned(), |r| format!("{r:.2}")),
            if w.partial { "partial" } else { "" }
        );
    }
    for h in rows
        .iter()
        .filter(|r| r.backend == "hybrid" && !rows.iter().any(|w| w.backend == "wgpu" && w.case == r.case))
    {
        let _ = writeln!(s, "| {} | {} | — | {} | — | |", h.case, h.metric, h.p50);
    }
    (s, worst)
}

/// Run the bench; the process exit code (0 informative, 1 over `--max-ratio`, 2 errors).
pub fn run(a: &BenchArgs) -> u8 {
    let selected: Vec<_> =
        BENCH_CASES.iter().filter(|(n, ..)| a.cases.is_empty() || a.cases.iter().any(|c| c == n)).collect();
    let mut all = Vec::new();
    if matches!(a.backend, Backend::Wgpu | Backend::Both) {
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
    if matches!(a.backend, Backend::Hybrid | Backend::Both) {
        // Its own device (DR-09: each backend on its own device, same process).
        let mut ctx = pollster::block_on(GpuContext::new(&GpuInit::headless()));
        let Some(gpu) = pollster::block_on(ctx.request_device(None)).ok().and_then(|id| ctx.device(id))
        else {
            eprintln!("render-check bench: hybrid: no GPU adapter");
            return 2;
        };
        let (device, queue) = (gpu.device().clone(), gpu.queue().clone());
        let mut h = HybridReference::new(device.clone(), queue);
        for (name, size, f) in &selected {
            let mut scene = Scene::new();
            f(&mut Painter::new(&mut scene));
            match bench_hybrid(&mut h, &device, name, *size, &scene, a) {
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
    let (md, worst) = summary(&all);
    let mut files = vec![("summary.md".to_owned(), md)];
    for b in ["wgpu", "hybrid"] {
        if all.iter().any(|r| r.backend == b) {
            files.push((format!("{b}.tsv"), tsv(&all, b)));
        }
    }
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
    match a.max_ratio {
        Some(m) if worst > m => {
            eprintln!("render-check bench: worst wgpu/hybrid ratio {worst:.3} exceeds --max-ratio {m}");
            1
        }
        _ => 0,
    }
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
        assert_eq!((a.frames, a.warmup, a.backend, a.max_ratio), (300, 30, Backend::Both, None));
        let a = parse(
            ["--cases", "bench/rects_10k", "--frames", "20", "--max-ratio", "0.5"]
                .map(String::from)
                .into_iter(),
        )
        .expect("args");
        assert_eq!((a.cases.len(), a.frames, a.max_ratio), (1, 20, Some(0.5)));
        assert!(parse(["--cases", "bench/nope"].map(String::from).into_iter()).is_err());
    }

    #[test]
    fn summary_ratio_and_worst() {
        let r = |backend, metric, p50, partial| Row {
            case: "bench/x".into(),
            backend,
            metric,
            p50,
            p95: p50,
            max: p50,
            frames: 1,
            partial,
        };
        let (md, worst) = summary(&[r("wgpu", "frame", 300, false), r("hybrid", "frame", 200, false)]);
        assert!(md.contains("| bench/x | frame | 300 | 200 | 1.50 |"), "{md}");
        assert!((worst - 1.5).abs() < 1e-9);
        let (_, worst) = summary(&[r("wgpu", "frame", 300, true), r("hybrid", "frame", 200, false)]);
        assert_eq!(worst, 0.0, "partial cases are not compared");
    }
}
