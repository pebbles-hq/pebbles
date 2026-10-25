//! M32 matched scenes, iced side. Same scenes and reporting as `../src/main.rs` (see README.md).
//! Frame ticks come from `iced::window::frames()`. The scale factor comes from winit
//! (`WINIT_X11_SCALE_FACTOR=2`, set by the driver); the `M32` line reports what the window saw.
//!
//! iced 0.13 has no blur, backdrop filter or opacity layer, so `effects` uses the closest
//! equivalents: frosted panels are translucent fills (no backdrop blur), blurred cards are a card
//! plus a same-colour blurred shadow halo, and opacity is applied to each colour's alpha.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{column, container, row, scrollable, stack, text, Space};
use iced::{
    mouse, window, Border, Color, Element, Font, Length, Padding, Point, Rectangle, Renderer, Shadow, Size, Subscription,
    Task, Theme, Vector,
};

const W: f32 = 1200.0;
const H: f32 = 800.0;

fn env_f(name: &str, default: f64) -> f64 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

struct App {
    scene: String,
    start: Instant,
    first: bool,
    last: Option<Instant>,
    samples: Vec<f64>,
    done: bool,
    phase: f32,
    dpr: f32,
    list: scrollable::Id,
    article: Vec<String>,
}

#[derive(Debug, Clone)]
enum Msg {
    Frame(Instant),
    Scale(f32),
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

fn bold(w: iced::font::Weight) -> Font {
    Font { weight: w, ..Default::default() }
}

const WORDS: [&str; 32] = [
    "the", "render", "pipeline", "frame", "budget", "layout", "glyph", "atlas", "shadow", "layer", "scroll", "physics",
    "texture", "upload", "batch", "paint", "widget", "tree", "signal", "commit", "raster", "cache", "a", "of", "and",
    "with", "every", "between", "measured", "quickly", "under", "across",
];

/// Paragraph `p` of the shared article (same formula as the Pebbles and Flutter apps).
fn paragraph(p: usize) -> String {
    let words: Vec<&str> = (0..90).map(|j| WORDS[(p * 7 + j * 13 + j / 5) % WORDS.len()]).collect();
    format!("{}.", words.join(" "))
}

impl App {
    fn update(&mut self, msg: Msg) -> Task<Msg> {
        let now = match msg {
            Msg::Scale(s) => {
                self.dpr = s;
                return Task::none();
            }
            Msg::Frame(now) => now,
        };
        if self.done {
            return Task::none();
        }
        let mut tasks = Vec::new();
        if !self.first {
            self.first = true;
            let epoch = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            println!("M32-first-frame epoch_ms={epoch}");
            tasks.push(window::get_oldest().and_then(window::get_scale_factor).map(Msg::Scale));
        }
        let t = now.duration_since(self.start).as_secs_f64();
        if t >= env_f("M32_WARM", 2.0) {
            if let Some(last) = self.last {
                self.samples.push(now.duration_since(last).as_secs_f64() * 1000.0);
            }
        }
        self.last = Some(now);
        self.phase = t as f32;
        if t >= env_f("M32_SECS", 12.0) {
            self.done = true;
            let mut s = std::mem::take(&mut self.samples);
            s.sort_by(f64::total_cmp);
            let pct = |p: usize| if s.is_empty() { 0.0 } else { s[((s.len() - 1) * p + 50) / 100] };
            println!(
                "M32 toolkit=iced scene={} frames={} p50_ms={:.2} p95_ms={:.2} dpr={}",
                self.scene,
                s.len(),
                pct(50),
                pct(95),
                self.dpr
            );
        }
        if self.scene == "list_10k_scroll" || self.scene == "text_heavy" {
            tasks.push(scrollable::scroll_to(self.list.clone(), scrollable::AbsoluteOffset { x: 0.0, y: (t * 1200.0) as f32 }));
        }
        Task::batch(tasks)
    }

    fn subscription(&self) -> Subscription<Msg> {
        if self.done {
            Subscription::none()
        } else {
            window::frames().map(Msg::Frame)
        }
    }

    fn view(&self) -> Element<'_, Msg> {
        match self.scene.as_str() {
            "rects_10k" => canvas(Rects { phase: self.phase }).width(W).height(H).into(),
            "list_10k_scroll" => scrollable(column((0..10_000).map(|i| {
                row![
                    text(format!("Row {i}")).size(15).font(bold(iced::font::Weight::Semibold)),
                    Space::with_width(16),
                    text(format!("The quick brown fox jumps over the lazy dog — item {i}")).size(14),
                ]
                .height(48)
                .into()
            })))
            .id(self.list.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
            "effects" => self.effects(),
            "text_heavy" => self.text_heavy(),
            _ => self.dashboard(),
        }
    }

    fn text_heavy(&self) -> Element<'_, Msg> {
        let mut col = column![];
        for (p, para) in self.article.iter().enumerate() {
            if p % 10 == 0 {
                col = col.push(text(format!("Section {}", p / 10 + 1)).size(26).font(bold(iced::font::Weight::Bold)));
                col = col.push(Space::with_height(8));
            }
            col = col.push(text(para.as_str()).size(16).line_height(text::LineHeight::Relative(1.5)));
            col = col.push(Space::with_height(12));
        }
        scrollable(container(col).width(W).padding(Padding { top: 24.0, bottom: 24.0, left: 220.0, right: 220.0 }))
            .id(self.list.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }

    fn effects(&self) -> Element<'_, Msg> {
        fn at<'a>(x: f32, y: f32, e: Element<'a, Msg>) -> Element<'a, Msg> {
            container(e).padding(Padding { top: y, left: x, right: 0.0, bottom: 0.0 }).into()
        }
        fn card<'a>(label: String, fill: Color, ink: Color, h: f32, shadow: Shadow) -> Element<'a, Msg> {
            container(text(label).size(22).color(ink).font(bold(iced::font::Weight::Semibold)))
                .width(360)
                .height(h)
                .padding(16)
                .style(move |_| container::Style {
                    background: Some(fill.into()),
                    border: Border { radius: 16.0.into(), ..Border::default() },
                    shadow,
                    ..container::Style::default()
                })
                .into()
        }
        let mut layers: Vec<Element<'_, Msg>> = vec![canvas(Stripes { phase: self.phase }).width(W).height(H).into()];
        for i in 0..3 {
            let x = 30.0 + i as f32 * 390.0;
            // Frosted: translucent fill only (iced has no backdrop filter).
            layers.push(at(x, 40.0, card(format!("Frosted {i}"), Color::from_rgba8(255, 255, 255, 60.0 / 255.0), Color::WHITE, 220.0, Shadow::default())));
            // Blurred (σ 4): a same-colour shadow with blur radius 8 (= 2σ) softens the edge.
            let fill = rgb(32, 36, 52);
            let halo = Shadow { color: fill, offset: Vector::ZERO, blur_radius: 8.0 };
            layers.push(at(x, 300.0, card(format!("Blurred {i}"), fill, Color::WHITE, 200.0, halo)));
            // Opacity: alpha on each colour (no group opacity in iced 0.13).
            let a = 0.55 + 0.45 * (self.phase * 3.0 + i as f32).sin();
            let fill = Color { a, ..rgb(250, 250, 250) };
            let ink = Color { a, ..rgb(20, 20, 30) };
            layers.push(at(x, 560.0, card(format!("Opacity {i}"), fill, ink, 200.0, Shadow::default())));
        }
        stack(layers).width(W).height(H).into()
    }

    fn dashboard(&self) -> Element<'_, Msg> {
        let card = |k: usize| -> Element<'_, Msg> {
            container(column![
                text(format!("Metric {k}")).size(18).color(rgb(230, 230, 240)),
                text(format!("{:.1}", 1000.0 + 50.0 * (self.phase * 6.0 + k as f32).sin())).size(32).color(Color::WHITE),
                canvas(Chart { phase: self.phase, k }).width(Length::Fill).height(160),
            ])
            .width(360)
            .height(330)
            .padding(16)
            .style(|_| container::Style {
                background: Some(rgb(32, 36, 52).into()),
                border: Border { radius: 14.0.into(), ..Border::default() },
                shadow: Shadow { color: Color::from_rgba8(0, 0, 0, 120.0 / 255.0), offset: Vector::new(0.0, 8.0), blur_radius: 24.0 },
                ..container::Style::default()
            })
            .into()
        };
        fn pad(e: Element<'_, Msg>) -> Element<'_, Msg> {
            container(e).padding(12).into()
        }
        container(column![
            row![pad(card(0)), pad(card(1)), pad(card(2))],
            row![pad(card(3)), pad(card(4)), pad(card(5))],
        ])
        .width(W)
        .height(H)
        .style(|_| container::Style { background: Some(rgb(15, 17, 26).into()), ..container::Style::default() })
        .into()
    }
}

fn canvas<P: canvas::Program<Msg>>(p: P) -> canvas::Canvas<P, Msg> {
    iced::widget::canvas(p)
}

struct Rects {
    phase: f32,
}

impl canvas::Program<Msg> for Rects {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let shift = (self.phase * 20.0) % 20.0;
        let (a, b) = (rgb(66, 133, 244), rgb(234, 67, 53));
        for i in 0..100 {
            for j in 0..100 {
                let p = Point::new(i as f32 * 10.0 + shift, j as f32 * 10.0 + shift * 0.5);
                f.fill_rectangle(p, Size::new(8.0, 8.0), if (i + j) % 2 == 0 { a } else { b });
            }
        }
        vec![f.into_geometry()]
    }
}

struct Stripes {
    phase: f32,
}

impl canvas::Program<Msg> for Stripes {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let cols = [rgb(66, 133, 244), rgb(234, 67, 53), rgb(251, 188, 5)];
        let shift = (self.phase * 120.0) % 120.0;
        for i in -3i32..30 {
            f.fill_rectangle(Point::new(i as f32 * 40.0 + shift, 0.0), Size::new(40.0, H), cols[i.rem_euclid(3) as usize]);
        }
        let y = (self.phase * 90.0) % H;
        f.fill_rectangle(Point::new(0.0, y), Size::new(W, 60.0), rgb(20, 160, 90));
        vec![f.into_geometry()]
    }
}

struct Chart {
    phase: f32,
    k: usize,
}

impl canvas::Program<Msg> for Chart {
    type State = ();
    fn draw(&self, _: &(), renderer: &Renderer, _: &Theme, bounds: Rectangle, _: mouse::Cursor) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let s = bounds.size();
        let path = Path::new(|b| {
            for i in 0..=60 {
                let x = s.width * i as f32 / 60.0;
                let y = s.height * (0.5 + 0.4 * (i as f32 * 0.25 + self.phase * 6.0 + self.k as f32).sin());
                if i == 0 {
                    b.move_to(Point::new(x, y));
                } else {
                    b.line_to(Point::new(x, y));
                }
            }
        });
        f.stroke(&path, Stroke::default().with_width(2.0).with_color(rgb(120, 200, 255)));
        vec![f.into_geometry()]
    }
}

fn main() -> iced::Result {
    let start = Instant::now();
    let scene = std::env::var("M32_SCENE").unwrap_or_else(|_| "dashboard".into());
    iced::application("M32 iced", App::update, App::view)
        .subscription(App::subscription)
        .window_size(Size::new(W, H))
        .run_with(move || {
            (
                App {
                    scene,
                    start,
                    first: false,
                    last: None,
                    samples: Vec::new(),
                    done: false,
                    phase: 0.0,
                    dpr: 0.0,
                    list: scrollable::Id::unique(),
                    article: (0..200).map(paragraph).collect(),
                },
                Task::none(),
            )
        })
}
