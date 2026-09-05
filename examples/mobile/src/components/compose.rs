//! The new-post composer, opened as a bottom sheet from the feed's FAB. Local draft
//! text + an optional attached photo; posting calls the store and closes the sheet.

use pebbles::prelude::*;

use crate::store;

/// Open the composer as a bottom sheet.
pub fn open_composer() {
    sheet(component(form)).side(Side::Bottom).size(360.0).title("New post").open();
}

fn form() -> impl IntoWidget {
    let c = theme().colors;
    let draft = create_signal(String::new());
    let media = create_signal::<Option<String>>(None);
    let shot = create_signal(0_usize); // cycles the sample photos

    let publish = move || {
        store::create_post(&draft.peek(), media.peek());
        close_sheet(0); // 0 = whatever sheet is open
    };
    let attach = move || {
        const SEEDS: [&str; 6] = ["sunset", "forest", "ocean", "street", "food", "workspace"];
        let i = shot.peek();
        shot.set((i + 1) % SEEDS.len());
        media.set(Some(store::photo(SEEDS[i])));
    };

    // Optional photo preview + a remove button.
    let preview: AnyWidget = match media.get() {
        Some(url) => stack(children![
            container()
                .decoration(BoxDecoration::new().color(c.secondary).radius(BorderRadius::all(12.0)))
                .clip()
                .height(150.0)
                .child(ImageView::network(url).fit(ImageFit::Cover)),
            positioned(
                icon_button(lucide::X).variant(ButtonVariant::Secondary).on_pressed(move || media.set(None))
            )
            .top(6.0)
            .right(6.0),
        ])
        .into_widget(),
        None => gap_h(0.0).into_widget(),
    };

    column(children![
        text_area(3).bind(draft).placeholder("What's happening?"),
        gap_h(12.0),
        preview,
        gap_h(12.0),
        row(children![
            pressable(
                row(children![
                    icon(lucide::IMAGE).size(18.0).color(c.primary),
                    gap_w(8.0),
                    text("Photo").size(14.0).color(c.primary)
                ])
                .cross_axis_alignment(CrossAxisAlignment::Center),
            )
            .radius(8.0)
            .on_tap(attach),
            spacer(),
            button("Post").on_pressed(publish),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min)
}
