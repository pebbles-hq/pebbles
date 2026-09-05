//! The profile tab — avatar + bio + stats, an editable profile (bottom sheet), a
//! **live dark-mode toggle** (`toggle_theme`), and the user's own posts.

use std::cell::RefCell;

use pebbles::prelude::*;

use crate::components::bits::avatar;
use crate::components::post_card;
use crate::store;

thread_local! {
    // Global so it survives tab switches (a root signal, like real global state).
    static DARK: RefCell<Option<Signal<bool>>> = const { RefCell::new(None) };
}
fn dark() -> Signal<bool> {
    DARK.with(|c| *c.borrow_mut().get_or_insert_with(|| create_root_signal(false)))
}

pub fn profile() -> impl IntoWidget {
    let c = theme().colors;
    let me = store::me();
    let posts = store::my_posts();

    let header = column(children![
        avatar(&me.avatar, 76.0),
        gap_h(12.0),
        text(me.name.clone()).size(20.0).semibold().color(c.foreground),
        text(format!("@{}", me.handle)).size(13.5).color(c.muted_foreground),
        gap_h(8.0),
        container().width(280.0).child(
            text(me.bio.clone()).size(13.5).line_height(1.4).align(TextAlign::Center).color(c.foreground)
        ),
        gap_h(16.0),
        row(children![
            stat(posts.len() as u32, "Posts"),
            stat(me.followers, "Followers"),
            stat(me.following, "Following"),
        ])
        .main_axis_alignment(MainAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
        gap_h(16.0),
        crate::components::bits::pill("Edit profile", false, open_edit),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min);

    // A real, working setting: flip the whole app light/dark live.
    let is_dark = dark().get();
    let settings = switch_list_tile("Dark mode", is_dark)
        .secondary(icon(lucide::MOON).color(c.muted_foreground))
        .on_changed(move || {
            dark().update(|d| *d = !*d);
            toggle_theme();
        });

    let mut kids: Vec<AnyWidget> = vec![
        container().padding(EdgeInsets::symmetric(16.0, 24.0)).child(header).into_widget(),
        container().padding(EdgeInsets::symmetric(8.0, 0.0)).child(settings).into_widget(),
        container().color(c.border).height(1.0).into_widget(),
        container()
            .padding(EdgeInsets::only(16.0, 14.0, 16.0, 6.0))
            .child(text("Your posts").size(15.0).semibold().color(c.foreground))
            .into_widget(),
    ];
    kids.extend(posts.iter().map(|p| post_card(p).into_widget()));
    kids.push(gap_h(16.0).into_widget());

    scroll_view(
        column(kids).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
}

fn stat(value: u32, label: &str) -> impl IntoWidget {
    let c = theme().colors;
    container().padding(EdgeInsets::symmetric(16.0, 0.0)).child(
        column(children![
            text(fmt_count(value)).size(18.0).semibold().color(c.foreground),
            text(label.to_string()).size(12.5).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn fmt_count(n: u32) -> String {
    if n >= 1000 { format!("{:.1}k", n as f64 / 1000.0) } else { n.to_string() }
}

// --- edit-profile bottom sheet ---------------------------------------------

fn open_edit() {
    sheet(component(edit_form)).side(Side::Bottom).size(300.0).title("Edit profile").open();
}

fn edit_form() -> impl IntoWidget {
    let me = store::me();
    let name = create_signal(me.name.clone());
    let bio = create_signal(me.bio.clone());

    let save = move || {
        store::update_profile(&name.peek(), &bio.peek());
        close_sheet(0);
    };

    column(children![
        text_field().label("Name").bind(name),
        gap_h(12.0),
        text_area(2).label("Bio").bind(bio),
        gap_h(16.0),
        button("Save").on_pressed(save),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min)
}
