//! A simple **landing page** — a scrolling marketing page (hero → features → code →
//! footer). A preview of the kind of site Pebbles can build *in Pebbles itself*
//! (the real one lives in the pebbles-landing repo).

use pebbles::prelude::*;

/// The content column's max width — everything centers within it.
const MAXW: f64 = 920.0;

fn page() -> impl IntoWidget {
    scroll_view(
        column(children![
            gap_h(72.0),
            hero(),
            gap_h(80.0),
            features(),
            gap_h(80.0),
            code_sample(),
            gap_h(64.0),
            footer(),
            gap_h(48.0)
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}

// ---------------------------------------------------------------------------
// Hero
// ---------------------------------------------------------------------------

fn hero() -> impl IntoWidget {
    let c = theme().colors;
    capped(
        column(children![
            // Brand mark.
            row(children![
                container()
                    .decoration(BoxDecoration::new().color(c.primary).radius(BorderRadius::all(10.0)))
                    .padding(EdgeInsets::all(8.0))
                    .child(icon(lucide::LAYERS).size(22.0).color(c.primary_foreground)),
                gap_w(12.0),
                text("Pebbles").size(20.0).weight(700.0).color(c.foreground),
            ])
            .main_axis_alignment(MainAxisAlignment::Center)
            .main_axis_size(MainAxisSize::Min),
            gap_h(28.0),
            text("Flutter's feel. Rust's teeth.").size(48.0).weight(800.0).align(TextAlign::Center).color(c.foreground),
            gap_h(16.0),
            container().width(600.0).child(
                text("A desktop-first GUI framework for Rust — Flutter-style widgets, SolidJS-style reactivity, drawn on the GPU with Vello. Build for desktop, web and mobile from one codebase.")
                    .size(17.0)
                    .line_height(1.5)
                    .align(TextAlign::Center)
                    .color(c.muted_foreground),
            ),
            gap_h(28.0),
            row(children![
                button("Get started").size(ButtonSize::Lg).trailing(lucide::ARROW_RIGHT),
                gap_w(12.0),
                button("Star on GitHub").variant(ButtonVariant::Outline).size(ButtonSize::Lg).leading(lucide::STAR),
            ])
            .main_axis_alignment(MainAxisAlignment::Center)
            .main_axis_size(MainAxisSize::Min),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}

// ---------------------------------------------------------------------------
// Features
// ---------------------------------------------------------------------------

fn features() -> impl IntoWidget {
    let items = [
        (
            "Flutter-style widgets",
            "Row/Column/Container/Stack, the box layout protocol, a rich themed catalog — you already know the vocabulary.",
            lucide::LAYOUT_DASHBOARD,
            palette::sky::S500,
        ),
        (
            "SolidJS reactivity",
            "No StatefulWidget, no setState. A create_signal you read and write directly; only what changed re-renders.",
            lucide::ZAP,
            palette::amber::S500,
        ),
        (
            "GPU rendering",
            "Vello + wgpu draw every frame on the GPU. Live theming, spring animations, virtualized lists — all smooth.",
            lucide::ACTIVITY,
            palette::violet::S500,
        ),
        (
            "One codebase",
            "Desktop today; web via WebGPU; iOS/Android compile-ready. Branch on the platform only where it matters.",
            lucide::SMARTPHONE,
            palette::emerald::S500,
        ),
        (
            "shadcn-styled",
            "Buttons, dialogs, sheets, tables, nav chrome — a polished, tokened design system out of the box.",
            lucide::PALETTE,
            palette::rose::S500,
        ),
        (
            "Idiomatic Rust",
            "Function components, plain closures for handlers, arenas instead of Rc<RefCell> — fast to compile, safe by default.",
            lucide::CODE,
            palette::orange::S500,
        ),
    ];
    let cards: Vec<AnyWidget> =
        items.iter().map(|&(t, d, ic, color)| feature_card(t, d, ic, color).into_widget()).collect();

    capped(
        column(children![
            text("Everything you need")
                .size(30.0)
                .weight(700.0)
                .align(TextAlign::Center)
                .color(theme().colors.foreground),
            gap_h(28.0),
            wrap(cards).spacing(18.0).run_spacing(18.0).alignment(WrapAlignment::Center),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn feature_card(title: &str, desc: &str, ic: IconData, color: Color) -> impl IntoWidget {
    let c = theme().colors;
    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(16.0)),
        )
        .padding(EdgeInsets::all(22.0))
        .width(288.0)
        .child(
            column(children![
                container()
                    .decoration(
                        BoxDecoration::new().color(with_alpha(color, 0.14)).radius(BorderRadius::all(10.0))
                    )
                    .padding(EdgeInsets::all(9.0))
                    .child(icon(ic).size(20.0).color(color)),
                gap_h(14.0),
                text(title.to_string()).size(16.5).semibold().color(c.foreground),
                gap_h(8.0),
                text(desc.to_string()).size(13.5).line_height(1.5).color(c.muted_foreground),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Start)
            .main_axis_size(MainAxisSize::Min),
        )
}

// ---------------------------------------------------------------------------
// Code sample
// ---------------------------------------------------------------------------

fn code_sample() -> impl IntoWidget {
    let c = theme().colors;
    let src = "fn counter() -> impl IntoWidget {\n    let count = create_signal(0);\n    center(column(children![\n        text(format!(\"{}\", count.get())).size(72.0),\n        button(\"+\").on_pressed(move || count.update(|c| *c += 1)),\n    ]))\n}";
    capped(
        column(children![
            text("A component is a function")
                .size(30.0)
                .weight(700.0)
                .align(TextAlign::Center)
                .color(c.foreground),
            gap_h(8.0),
            text("State is a signal. A handler is a closure. That's the whole model.")
                .size(15.0)
                .align(TextAlign::Center)
                .color(c.muted_foreground),
            gap_h(24.0),
            container()
                .decoration(BoxDecoration::new().color(palette::zinc::S900).radius(BorderRadius::all(14.0)))
                .padding(EdgeInsets::all(22.0))
                .width(640.0)
                .child(
                    text(src.to_string())
                        .size(13.5)
                        .line_height(1.6)
                        .font_family("monospace")
                        .color(palette::zinc::S100)
                ),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

fn footer() -> impl IntoWidget {
    let c = theme().colors;
    capped(
        column(children![
            container().color(c.border).height(1.0),
            gap_h(20.0),
            row(children![
                text("Pebbles").size(14.0).semibold().color(c.foreground),
                spacer(),
                text("© 2026 · Apache-2.0 · Built with Pebbles").size(12.5).color(c.muted_foreground),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    )
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Cap a section to the page's max content width.
fn capped(child: impl IntoWidget) -> impl IntoWidget {
    container().width(MAXW).padding(EdgeInsets::symmetric(24.0, 0.0)).child(child)
}

fn with_alpha(c: Color, a: f32) -> Color {
    let [r, g, b, _] = c.components;
    Color::new([r, g, b, a])
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(page)).title("Pebbles — Landing").size(1080, 820).background(palette::zinc::S50).run()
}
