use std::rc::Rc;

use pebbles::prelude::*;

use crate::ui::{doc, gap_h, screen};

pub fn icons() -> Element {
    let c = theme().colors;

    // A live filter over the whole bundled Lucide set.
    let query = create_signal(String::new());
    let q = query.get().to_lowercase();
    let matches: Rc<Vec<(&'static str, IconData)>> = Rc::new(
        lucide::ALL
            .iter()
            .filter(|(name, _)| q.is_empty() || name.contains(q.as_str()))
            .copied()
            .collect(),
    );
    let count = matches.len();

    let grid_items = matches.clone();
    let grid = Container::new()
        .decoration(
            BoxDecoration::new()
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(theme().radius)),
        )
        .height(420.0)
        .child(GridView::builder(count, 6, 82.0, move |i| {
            let cc = theme().colors;
            let (name, data) = grid_items[i];
            Container::new().padding(EdgeInsets::all(4.0)).child(
                Container::new()
                    .decoration(BoxDecoration::new().radius(BorderRadius::all(8.0)))
                    .padding(EdgeInsets::symmetric(6.0, 8.0))
                    .child(
                        column(children![
                            icon(data).size(22.0).color(cc.foreground),
                            gap_h(7.0),
                            text(name).size(10.0).color(cc.muted_foreground),
                        ])
                        .cross_axis_alignment(CrossAxisAlignment::Center)
                        .main_axis_size(MainAxisSize::Min),
                    ),
            )
        }));

    screen("Icons")

        .description("The default icon set is Lucide — every glyph below ships in the framework. Icons are plain data (IconData), so any Lucide glyph, a named IconKind, or your own icon drops in wherever an icon is accepted.")

        .body(
        children![
            doc("Named handles")
                .description("IconKind covers the common glyphs the widgets use — icon(IconKind::Check). Each resolves to a Lucide icon.")
                .body(
                wrap(children![
                    icon(IconKind::Check).size(22.0),
                    icon(IconKind::Search).size(22.0),
                    icon(IconKind::Star).size(22.0),
                    icon(IconKind::Info).size(22.0),
                    icon(IconKind::Warning).size(22.0),
                    icon(IconKind::ArrowRight).size(22.0),
                    icon(IconKind::User).size(22.0),
                    icon(IconKind::Calendar).size(22.0),
                ])
                .spacing(14.0),
            ),
            doc("The full Lucide set")
                .description("Reach any of the bundled icons by const — icon(lucide::CAMERA) — or by name at runtime — icon(lucide::by_name(\"circle-check\").unwrap()). Search the whole catalog:")
                .body(
                column(
                    children![
                        text_field()
                            .kind(InputKind::Search)
                            .placeholder("Search icons…")
                            .width(360.0)
                            .on_changed(move |s| query.set(s.to_string())),
                        gap_h(6.0),
                        muted(format!("{count} of {} icons", lucide::ALL.len())),
                        gap_h(10.0),
                        grid,
                    ],
                )
                .cross_axis_alignment(CrossAxisAlignment::Start)
                .main_axis_size(MainAxisSize::Min),
            ),
            doc("Bring your own")
                .description("An icon is just data. Define a const IconData from SVG path strings (or load one at runtime) and it works everywhere — no enum entry, no framework change.")
                .body(
                icon(HEART).size(28.0).color(c.destructive),
            ),
        ],
    )
}

// A custom, non-Lucide icon defined entirely in user code — a filled heart.
const HEART: IconData = IconData::filled(
    24.0,
    &[IconPrim::Path(
        "M12 21c-1-.7-8-5.5-8-11a4.5 4.5 0 0 1 8-2.8A4.5 4.5 0 0 1 20 10c0 5.5-7 10.3-8 11z",
    )],
);
