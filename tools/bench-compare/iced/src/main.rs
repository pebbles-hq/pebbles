//! M32 matched scenes, iced side. Same scenes and reporting as `../src/main.rs` (see README.md).
//! Frame ticks come from `iced::window::frames()`.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::widget::{column, container, row, scrollable, text, Space};
use iced::{mouse, window, Border, Color, Element, Length, Point, Rectangle, Renderer, Shadow, Size, Subscription, Task, Theme, Vector};

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
    list: scrollable::Id,
}

#[derive(Debug, Clone)]
enum Msg {
    Frame(Instant),
}

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

impl App {
    fn update(&mut self, Msg::Frame(now): Msg) -> Task<Msg> {
        if self.done {
            return Task::none();
        }
        if !self.first {
            self.first = true;
            let epoch = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            println!("M32-first-frame epoch_ms={epoch}");
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
            println!("M32 toolkit=iced scene={} frames={} p50_ms={:.2} p95_ms={:.2}", self.scene, s.len(), pct(50), pct(95));
        }
        if self.scene == "list_10k_scroll" {
            return scrollable::scroll_to(self.list.clone(), scrollable::AbsoluteOffset { x: 0.0, y: (t * 1200.0) as f32 });
        }
        Task::none()
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
                    text(format!("Row {i}")).size(15).font(iced::Font { weight: iced::font::Weight::Semibold, ..Default::default() }),
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
            _ => self.dashboard(),
        }
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
                App { scene, start, first: false, last: None, samples: Vec::new(), done: false, phase: 0.0, list: scrollable::Id::unique() },
                Task::none(),
            )
        })
}
