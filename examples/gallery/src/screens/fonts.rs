use pebbles::prelude::*;

use crate::ui::screen;

const PANGRAM: &str = "The quick brown fox jumps over the lazy dog";
const SPECIMEN: &str = "0123456789 — AaBbCc… &$%#@";

pub fn fonts() -> Element {
    let query = create_signal(String::new());
    let filter = create_signal(1usize);

    screen("Fonts")
        .description(
            "Every family the toolkit can render: the bundled set (Inter, JetBrains Mono, Space Grotesk, Lora — SIL OFL, always available) plus everything installed on this machine, discovered through fontique. Apply one with .font_family(\"name\"); unresolvable names fall back automatically.",
        )
        .body({
            let q = query.get().to_lowercase();
            let f = filter.get();
            let all = families();
            let builtin_count = all.iter().filter(|n| is_builtin(n)).count();
            let mut shown: Vec<(String, bool)> = all
                .iter()
                .filter(|name| {
                    let want = match f {
                        1 => is_builtin(name),
                        2 => !is_builtin(name),
                        _ => true,
                    };
                    want && (q.is_empty() || name.to_lowercase().contains(&q))
                })
                .map(|n| (n.clone(), is_builtin(n)))
                .collect();
            let total = shown.len();
            shown.truncate(120);

            children![column(children![
                row(children![
                    text_field()
                        .placeholder("Search families…")
                        .kind(InputKind::Search)
                        .width(260.0)
                        .on_changed(move |s| query.set(s.to_string())),
                    gap_w(8.0),
                    filter_button("All", 0, filter),
                    filter_button("Built-in", 1, filter),
                    filter_button("System", 2, filter),
                    gap_w(4.0),
                    muted(format!(
                        "{total} shown · {} bundled · {} system",
                        builtin_count,
                        families().len() - builtin_count
                    )),
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(16.0),
                column({
                    let mut items: Vec<AnyWidget> = Vec::new();
                    for (name, builtin) in &shown {
                        items.push(family_card(name, *builtin).into_widget());
                    }
                    items
                })
                .main_axis_size(MainAxisSize::Min)
                .spacing(10.0),
                gap_h(12.0),
                muted(if total > shown.len() {
                    format!("…showing {shown_len} of {total} — refine the search to narrow it down.", shown_len = shown.len())
                } else {
                    String::new()
                }),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min)]
        })
}

fn filter_button(label: &'static str, which: usize, active: Signal<usize>) -> impl IntoWidget {
    let selected = active.get() == which;
    let mut b = button(label);
    b = if selected { b.variant(ButtonVariant::Primary) } else { b.variant(ButtonVariant::Secondary) };
    b.on_click(move || active.set(which))
}

fn family_card(name: &str, builtin: bool) -> impl IntoWidget {
    let th = theme();
    container()
        .decoration(
            BoxDecoration::new()
                .border(Border::new(th.colors.border, 1.0))
                .radius(BorderRadius::all(th.radius)),
        )
        .padding(EdgeInsets::all(14.0))
        .child(
            column(children![
                row(children![
                    text(name.to_string()).font_family(name.to_string()).size(17.0).semibold(),
                    gap_w(8.0),
                    if builtin {
                        badge("bundled").variant(BadgeVariant::Secondary).into_widget()
                    } else {
                        badge("system").into_widget()
                    },
                ])
                .main_axis_size(MainAxisSize::Min),
                gap_h(6.0),
                text(PANGRAM.to_string()).font_family(name.to_string()).size(15.0).line_height(1.35),
                text(SPECIMEN.to_string())
                    .font_family(name.to_string())
                    .size(13.0)
                    .color(th.colors.muted_foreground),
                gap_h(8.0),
                row(children![
                    text("Regular").font_family(name.to_string()).weight(400.0).size(12.5),
                    text("Medium").font_family(name.to_string()).weight(500.0).size(12.5),
                    text("Bold").font_family(name.to_string()).weight(700.0).size(12.5),
                    text("Black").font_family(name.to_string()).weight(900.0).size(12.5),
                ])
                .main_axis_size(MainAxisSize::Min)
                .spacing(14.0),
                gap_h(8.0),
                muted(format!(".font_family(\"{name}\")")),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
}
