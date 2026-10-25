//! M32 matched scenes, Pebbles side (see `README.md`). The Flutter (`flutter/`) and iced (`iced/`)
//! apps draw the same scenes and report with the same method:
//!
//! - every frame tick takes a wall-clock timestamp; from `M32_WARM` s to `M32_SECS` s the
//!   intervals between consecutive frames are collected, then the animation stops (the app stays
//!   open and idle so the driver can sample idle CPU and RSS);
//! - one `M32 …` line is printed (frames, p50/p95 interval in ms), and one `M32-first-frame` line with the wall-clock epoch (ms) of the first frame,
//!   which the driver subtracts from its spawn time for startup.
//!
//! Scene from `M32_SCENE`: `list_10k_scroll`, `dashboard`, `effects`, `text_heavy` (plus the
//! legacy `rects_10k`). The scale factor comes from the window: the driver sets
//! `WINIT_X11_SCALE_FACTOR=2`.

use std::cell::RefCell;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use pebbles::prelude::*;
use pebbles::render::ScrollHandle;

const W: f64 = 1200.0;
const H: f64 = 800.0;

fn env_f(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

struct Clock {
    start: Instant,
    first: bool,
    last: Option<Instant>,
    samples: Vec<f64>,
    done: bool,
}

thread_local! {
    static CLOCK: RefCell<Clock> =
        RefCell::new(Clock { start: Instant::now(), first: false, last: None, samples: Vec::new(), done: false });
    static ARTICLE: ScrollHandle = ScrollHandle::new();
}

fn elapsed() -> f64 {
    CLOCK.with(|c| c.borrow().start.elapsed().as_secs_f64())
}

/// Called once per frame tick. Returns false once the measurement window is over.
fn on_frame(scene: &str) -> bool {
    let (warm, secs) = (env_f("M32_WARM", 2.0), env_f("M32_SECS", 12.0));
    CLOCK.with(|c| {
        let mut c = c.borrow_mut();
        if c.done {
            return false;
        }
        let now = Instant::now();
        if !c.first {
            c.first = true;
            let epoch = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            println!("M32-first-frame epoch_ms={epoch}");
        }
        let t = now.duration_since(c.start).as_secs_f64();
        if t >= warm
            && let Some(last) = c.last
        {
            c.samples.push(now.duration_since(last).as_secs_f64() * 1000.0);
        }
        c.last = Some(now);
        if t >= secs {
            c.done = true;
            let mut s = std::mem::take(&mut c.samples);
            s.sort_by(f64::total_cmp);
            let pct = |p: usize| if s.is_empty() { 0.0 } else { s[((s.len() - 1) * p + 50) / 100] };
            println!(
                "M32 toolkit=pebbles scene={scene} frames={} p50_ms={:.2} p95_ms={:.2}",
                s.len(),
                pct(50),
                pct(95)
            );
            return false;
        }
        true
    })
}

fn rects(phase: f64) -> impl IntoWidget {
    let shift = (phase * 20.0) % 20.0;
    canvas(move |c: &mut Canvas<'_>| {
        let (a, b) = (Color::from_rgba8(66, 133, 244, 255), Color::from_rgba8(234, 67, 53, 255));
        for i in 0..100 {
            for j in 0..100 {
                let (x, y) = (f64::from(i) * 10.0 + shift, f64::from(j) * 10.0 + shift * 0.5);
                c.fill_rect(Rect::new(x, y, x + 8.0, y + 8.0), if (i + j) % 2 == 0 { a } else { b });
            }
        }
    })
    .width(W)
    .height(H)
}

fn chart(phase: f64, k: usize) -> impl IntoWidget {
    canvas(move |c: &mut Canvas<'_>| {
        let s = c.size();
        let mut p = BezPath::new();
        for i in 0..=60 {
            let x = s.width * f64::from(i) / 60.0;
            let y = s.height * (0.5 + 0.4 * (f64::from(i) * 0.25 + phase * 6.0 + k as f64).sin());
            if i == 0 {
                p.move_to((x, y));
            } else {
                p.line_to((x, y));
            }
        }
        c.stroke_path(&p, 2.0, Color::from_rgba8(120, 200, 255, 255));
    })
    .height(160.0)
}

fn card(phase: f64, k: usize) -> impl IntoWidget {
    container()
        .width(360.0)
        .height(330.0)
        .margin(EdgeInsets::all(12.0))
        .padding(EdgeInsets::all(16.0))
        .decoration(
            BoxDecoration::new()
                .color(Color::from_rgba8(32, 36, 52, 255))
                .radius(BorderRadius::all(14.0))
                .shadow(BoxShadow::new(Color::from_rgba8(0, 0, 0, 120), Offset::new(0.0, 8.0), 24.0, 0.0)),
        )
        .child(column(children![
            text(format!("Metric {k}")).size(18.0).weight(600.0).color(Color::from_rgba8(230, 230, 240, 255)),
            text(format!("{:.1}", 1000.0 + 50.0 * (phase * 6.0 + k as f64).sin()))
                .size(32.0)
                .color(Color::from_rgba8(255, 255, 255, 255)),
            chart(phase, k),
        ]))
}

fn dashboard(phase: f64) -> impl IntoWidget {
    container()
        .width(W)
        .height(H)
        .decoration(BoxDecoration::new().color(Color::from_rgba8(15, 17, 26, 255)))
        .child(column(children![
            row(children![card(phase, 0), card(phase, 1), card(phase, 2)]),
            row(children![card(phase, 3), card(phase, 4), card(phase, 5)]),
        ]))
}

fn row_item(i: usize) -> impl IntoWidget {
    row(children![
        text(format!("Row {i}")).size(15.0).weight(600.0),
        gap_w(16.0),
        text(format!("The quick brown fox jumps over the lazy dog — item {i}")).size(14.0),
    ])
}

/// Background for `effects`: vertical stripes in three colours plus a horizontal band, both
/// moving every frame so every blur has fresh content to filter.
fn stripes(phase: f64) -> impl IntoWidget {
    canvas(move |c: &mut Canvas<'_>| {
        let cols = [
            Color::from_rgba8(66, 133, 244, 255),
            Color::from_rgba8(234, 67, 53, 255),
            Color::from_rgba8(251, 188, 5, 255),
        ];
        let shift = (phase * 120.0) % 120.0;
        for i in -3_i32..30 {
            let x = f64::from(i) * 40.0 + shift;
            c.fill_rect(Rect::new(x, 0.0, x + 40.0, H), cols[i.rem_euclid(3) as usize]);
        }
        let y = (phase * 90.0) % H;
        c.fill_rect(Rect::new(0.0, y, W, y + 60.0), Color::from_rgba8(20, 160, 90, 255));
    })
    .width(W)
    .height(H)
}

fn effect_card(label: String, fill: Color, ink: Color, height: f64) -> impl IntoWidget {
    container()
        .width(360.0)
        .height(height)
        .padding(EdgeInsets::all(16.0))
        .decoration(BoxDecoration::new().color(fill).radius(BorderRadius::all(16.0)))
        .child(text(label).size(22.0).weight(600.0).color(ink))
}

/// `effects`: 3 frosted panels (backdrop blur σ 12 under a translucent fill), 3 cards drawn
/// through a blur (σ 4) and 3 cards whose opacity animates, over a moving background.
fn effects(phase: f64) -> impl IntoWidget {
    let white = Color::from_rgba8(255, 255, 255, 255);
    let mut kids: Vec<AnyWidget> = vec![positioned(stripes(phase)).left(0.0).top(0.0).into_widget()];
    for i in 0..3_u8 {
        let x = 30.0 + f64::from(i) * 390.0;
        let frosted = backdrop_filter(
            LayerEffect::Blur { sigma_x: 12.0, sigma_y: 12.0, edge: BlurEdge::Clamp },
            effect_card(format!("Frosted {i}"), Color::from_rgba8(255, 255, 255, 60), white, 220.0),
        )
        .radius(16.0);
        kids.push(positioned(frosted).left(x).top(40.0).into_widget());
        let blurred = image_filtered(
            LayerEffect::Blur { sigma_x: 4.0, sigma_y: 4.0, edge: BlurEdge::Decal },
            effect_card(format!("Blurred {i}"), Color::from_rgba8(32, 36, 52, 255), white, 200.0),
        );
        kids.push(positioned(blurred).left(x).top(300.0).into_widget());
        let a = 0.55 + 0.45 * (phase * 3.0 + f64::from(i)).sin();
        let faded = opacity(
            a as f32,
            effect_card(
                format!("Opacity {i}"),
                Color::from_rgba8(250, 250, 250, 255),
                Color::from_rgba8(20, 20, 30, 255),
                200.0,
            ),
        );
        kids.push(positioned(faded).left(x).top(560.0).into_widget());
    }
    container().width(W).height(H).child(stack(kids))
}

const WORDS: [&str; 32] = [
    "the", "render", "pipeline", "frame", "budget", "layout", "glyph", "atlas", "shadow", "layer", "scroll",
    "physics", "texture", "upload", "batch", "paint", "widget", "tree", "signal", "commit", "raster",
    "cache", "a", "of", "and", "with", "every", "between", "measured", "quickly", "under", "across",
];

/// Paragraph `p` of the shared article: 90 words picked deterministically from [`WORDS`]
/// (the Flutter and iced apps use the same formula).
fn paragraph(p: usize) -> String {
    let mut s = String::new();
    for j in 0..90 {
        if j > 0 {
            s.push(' ');
        }
        s.push_str(WORDS[(p * 7 + j * 13 + j / 5) % WORDS.len()]);
    }
    s.push('.');
    s
}

/// `text_heavy`: a 200-paragraph article (a heading every 10) in a 760 px column, scrolled at
/// 1200 px/s through a single-child scroll view (the whole article is laid out once).
fn article() -> impl IntoWidget {
    let mut kids: Vec<AnyWidget> = Vec::new();
    for p in 0..200 {
        if p % 10 == 0 {
            kids.push(text(format!("Section {}", p / 10 + 1)).size(26.0).weight(700.0).into_widget());
            kids.push(gap_h(8.0).into_widget());
        }
        kids.push(text(paragraph(p)).size(16.0).line_height(1.5).into_widget());
        kids.push(gap_h(12.0).into_widget());
    }
    let body = container().width(W).padding(EdgeInsets::symmetric(220.0, 24.0)).child(column(kids));
    scroll_view(body).controller(ARTICLE.with(Clone::clone))
}

fn bench() -> impl IntoWidget {
    let scene = std::env::var("M32_SCENE").unwrap_or_else(|_| "dashboard".into());
    let active = create_signal(true);
    let tick = create_loop_while(active.get(), 1.0);
    let list = use_scroll_controller();
    let s2 = scene.clone();
    create_effect(move || {
        tick.get();
        if !active.peek() {
            return;
        }
        if !on_frame(&s2) {
            active.set(false);
        }
        // A constant-speed fling: 1200 px/s.
        list.jump_to(elapsed() * 1200.0);
        ARTICLE.with(|a| a.scroll_to(elapsed() * 1200.0));
    });
    let _ = tick.get(); // re-render every frame while active
    let phase = elapsed();
    match scene.as_str() {
        "rects_10k" => rects(phase).into_widget(),
        "list_10k_scroll" => ListView::builder(10_000, 48.0, row_item).controller(list).into_widget(),
        "effects" => effects(phase).into_widget(),
        "text_heavy" => article().into_widget(),
        _ => dashboard(phase).into_widget(),
    }
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    CLOCK.with(|c| c.borrow_mut().start = Instant::now());
    App::new(component(bench)).title("M32 Pebbles").size(W as u32, H as u32).run()
}
