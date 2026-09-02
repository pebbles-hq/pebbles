use pebbles::prelude::*;

use crate::ui::{doc, gap_h, gap_w, screen};

const SAMPLE: &str = "Typography is the craft of endowing human language with a durable visual form. Good typesetting builds a clear hierarchy, a steady rhythm and an inviting flow, so readers absorb meaning without ever noticing the machinery.";

pub fn typography() -> Element {
    let align = create_signal(0usize);

    screen("Typography")
        .description(
            "The full typographic voice: a type scale from display to overline, nine weights, emphasis, a live alignment playground (Word-style left / center / right / justified), line height and truncation — then the bundled families from the Fonts screen.",
        )
        .body(children![
            presets(),
            type_scale(),
            weights(),
            emphasis(),
            alignment_playground(align),
            line_height(),
            truncation(),
            no_wrap(),
            families(),
        ])
}

fn presets() -> impl IntoWidget {
    doc("Presets")
        .description("The themed helpers: heading / title / subtitle / body / label / muted.")
        .body(
            column(children![
                heading("Heading — 30 bold"),
                title("Title — 18 semibold"),
                subtitle("Subtitle — 14 muted"),
                body("Body — the quick brown fox jumps over the lazy dog."),
                label("Label — 13 medium"),
                muted("Muted — secondary text"),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(8.0),
        )
}

fn scale_row(label: &'static str, sample: impl IntoWidget) -> impl IntoWidget {
    row(children![
        Container::new()
            .width(110.0)
            .alignment(Alignment::CENTER_RIGHT)
            .padding(EdgeInsets::only(0.0, 0.0, 16.0, 0.0))
            .child(muted(label.to_string()).size(11.5))
            .into_widget(),
        sample.into_widget(),
    ])
    .main_axis_size(MainAxisSize::Min)
    .into_widget()
}

fn type_scale() -> impl IntoWidget {
    doc("Type scale")
        .description("Eight steps, from display to overline — the sizes and weights a product UI draws from. Labels show size + weight.")
        .body(
            column(children![
                scale_row("Display · 44 · 300", text("The quick brown fox").size(44.0).weight(300.0)),
                scale_row("H1 · 32 · bold", text("Heading one").size(32.0).bold()),
                scale_row("H2 · 26 · 600", text("Heading two").size(26.0).weight(600.0)),
                scale_row("H3 · 21 · 600", text("Heading three").size(21.0).weight(600.0)),
                scale_row("H4 · 17 · 500", text("Heading four").size(17.0).weight(500.0)),
                scale_row("Body · 14 · 400", text("Body copy sets the reading rhythm of the interface.").size(14.0).line_height(1.4)),
                scale_row("Small · 12.5 · 400", text("Small print for auxiliary detail.").size(12.5)),
                scale_row("Overline · 11 · 600", text("OVERLINE LABEL".to_string()).size(11.0).weight(600.0).letter_spacing(1.4)),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(12.0),
        )
}

fn weights() -> impl IntoWidget {
    doc("Weights")
        .description("Nine weights of the same family (Inter's full variable range) — set with .weight(n).")
        .body(
            row({
                let mut items: Vec<AnyWidget> = Vec::new();
                for w in [100, 200, 300, 400, 500, 600, 700, 800, 900] {
                    items.push(
                        column(children![
                            text("Aa").size(26.0).weight(w as f32),
                            muted(format!("{w}")).size(11.0),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget(),
                    );
                }
                items
            })
            .main_axis_size(MainAxisSize::Min)
            .spacing(14.0),
        )
}

fn emphasis() -> impl IntoWidget {
    let th = theme();
    doc("Emphasis")
        .description(".italic(), .underline(), .strikethrough(), .letter_spacing(n) — and a highlight achieved with a colored Container.")
        .body(
            column(children![
                row(children![
                    Container::new()
                        .width(150.0)
                        .child(muted("italic"))
                        .into_widget(),
                    text("The quick brown fox jumps over the lazy dog.").italic().into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                row(children![
                    Container::new()
                        .width(150.0)
                        .child(muted("underline"))
                        .into_widget(),
                    text("The quick brown fox jumps over the lazy dog.").underline().into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                row(children![
                    Container::new()
                        .width(150.0)
                        .child(muted("strikethrough"))
                        .into_widget(),
                    text("The quick brown fox jumps over the lazy dog.").strikethrough().into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                row(children![
                    Container::new()
                        .width(150.0)
                        .child(muted("letter-spacing"))
                        .into_widget(),
                    text("T R A C K E D   O U T").letter_spacing(3.0).size(13.0).weight(600.0).into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                row(children![
                    Container::new()
                        .width(150.0)
                        .child(muted("highlight"))
                        .into_widget(),
                    Container::new()
                        .padding(EdgeInsets::symmetric(2.0, 6.0))
                        .decoration(BoxDecoration::new().color(th.colors.secondary).radius(BorderRadius::all(4.0)))
                        .child(text("marked passage").size(13.0))
                        .into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
                row(children![
                    Container::new()
                        .width(150.0)
                        .child(muted("italic + bold"))
                        .into_widget(),
                    text("The quick brown fox jumps over the lazy dog.").italic().bold().into_widget(),
                ])
                .main_axis_size(MainAxisSize::Min),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(10.0),
        )
}

fn alignment_playground(align: Signal<usize>) -> impl IntoWidget {
    let a = align.get();
    let (name, value) = match a {
        1 => ("center", TextAlign::Center),
        2 => ("right", TextAlign::Right),
        3 => ("justified", TextAlign::Justify),
        _ => ("left", TextAlign::Left),
    };
    let th = theme();
    doc("Alignment — live playground")
        .description("Click the Word-style toolbar to re-align the paragraph: left, center, right, or justified (flush both edges). .align(TextAlign) drives it; Start/End variants follow text direction for RTL.")
        .body(
            column(children![
                row(children![
                    align_button("Left", lucide::TEXT_ALIGN_START, 0, align),
                    align_button("Center", lucide::TEXT_ALIGN_CENTER, 1, align),
                    align_button("Right", lucide::TEXT_ALIGN_END, 2, align),
                    align_button("Justified", lucide::TEXT_ALIGN_JUSTIFY, 3, align),
                    gap_w(6.0),
                    muted(format!("TextAlign::{name}")).size(12.0),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(10.0),
                Container::new()
                    .width(480.0)
                    .padding(EdgeInsets::all(16.0))
                    .decoration(
                        BoxDecoration::new()
                            .border(Border::new(th.colors.border, 1.0))
                            .radius(BorderRadius::all(th.radius)),
                    )
                    .child(
                        text(SAMPLE.to_string())
                            .size(14.0)
                            .line_height(1.5)
                            .align(value),
                    ),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

fn align_button(label: &'static str, icon: IconData, which: usize, active: Signal<usize>) -> impl IntoWidget {
    let selected = active.get() == which;
    let mut b = button(label).leading(icon);
    b = if selected { b.variant(ButtonVariant::Primary) } else { b.variant(ButtonVariant::Secondary) };
    b.on_click(move || active.set(which))
}

fn line_height() -> impl IntoWidget {
    let th = theme();
    doc("Line height")
        .description(".line_height(factor) multiplies the font size. Tight for headings, 1.4–1.6 for long-form reading.")
        .body(
            row({
                let mut items: Vec<AnyWidget> = Vec::new();
                for lh in [1.0f32, 1.2, 1.4, 1.6] {
                    items.push(
                        column(children![
                            Container::new()
                                .width(130.0)
                                .padding(EdgeInsets::all(10.0))
                                .decoration(
                                    BoxDecoration::new()
                                        .border(Border::new(th.colors.border, 1.0))
                                        .radius(BorderRadius::all(th.radius)),
                                )
                                .child(
                                    text("Reading rhythm comes from generous leading and steady measure.".to_string())
                                        .size(12.5)
                                        .line_height(lh),
                                )
                                .into_widget(),
                            gap_h(6.0),
                            muted(format!("line_height({lh})")).size(11.5).into_widget(),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget(),
                    );
                }
                items
            })
            .main_axis_size(MainAxisSize::Min)
            .spacing(14.0),
        )
}

fn truncation() -> impl IntoWidget {
    let th = theme();
    doc("Truncation")
        .description(".max_lines(n) + .ellipsis() clamps overflow with a … — the clamp re-shapes the longest prefix that fits.")
        .body(
            row({
                let mut items: Vec<AnyWidget> = Vec::new();
                for (n, w) in [(1u32, 150.0), (2, 220.0), (3, 300.0)] {
                    items.push(
                        column(children![
                            Container::new()
                                .width(w)
                                .padding(EdgeInsets::all(10.0))
                                .decoration(
                                    BoxDecoration::new()
                                        .border(Border::new(th.colors.border, 1.0))
                                        .radius(BorderRadius::all(th.radius)),
                                )
                                .child(
                                    text(SAMPLE.to_string())
                                        .size(12.5)
                                        .line_height(1.4)
                                        .max_lines(n)
                                        .ellipsis(),
                                )
                                .into_widget(),
                            gap_h(6.0),
                            muted(format!("max_lines({n})")).size(11.5).into_widget(),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget(),
                    );
                }
                items
            })
            .main_axis_size(MainAxisSize::Min)
            .spacing(14.0),
        )
}

fn no_wrap() -> impl IntoWidget {
    let th = theme();
    doc("No wrap")
        .description(".soft_wrap(false) shapes a single unbounded line that clips to its box — pair with .ellipsis() for the classic one-line label. The default (soft_wrap true) wraps to fit.")
        .body(
            row(children![
                Container::new()
                    .width(150.0)
                    .padding(EdgeInsets::all(10.0))
                    .decoration(
                        BoxDecoration::new()
                            .border(Border::new(th.colors.border, 1.0))
                            .radius(BorderRadius::all(th.radius)),
                    )
                    .clip()
                    .child(text(SAMPLE.to_string()).size(12.5).soft_wrap(false).ellipsis())
                    .into_widget(),
                gap_w(16.0),
                muted("soft_wrap(false) → one line, …".to_string()).size(11.5).into_widget(),
            ])
            .main_axis_size(MainAxisSize::Min),
        )
}

fn families() -> impl IntoWidget {
    doc("Font families")
        .description(".font_family(\'name\') picks any family — bundled or installed on this machine. The four bundled faces ship in the binary; browse every available family (with search) on the Fonts screen.")
        .body(
            column({
                let mut items: Vec<AnyWidget> = Vec::new();
                for name in builtins() {
                    items.push(
                        row(children![
                            Container::new()
                                .width(130.0)
                                .child(text(name.to_string()).size(13.0).semibold())
                                .into_widget(),
                            text("The quick brown fox jumps over the lazy dog".to_string())
                                .font_family(name.to_string())
                                .size(15.0)
                                .into_widget(),
                        ])
                        .main_axis_size(MainAxisSize::Min)
                        .into_widget(),
                    );
                }
                items
            })
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min)
            .spacing(6.0),
        )
}
