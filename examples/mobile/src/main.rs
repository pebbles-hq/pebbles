//! A **mobile-structured** app, built at phone size (390×844). This is the shape a
//! real mobile app takes — a `Scaffold` with a top app bar, a scrolling body, and
//! **bottom navigation** switching between per-tab screens, plus a floating action
//! button. On desktop for now: only the window size differs.
//!
//! Structure (how a mobile app is normally laid out):
//!   app()            → the Scaffold shell + the current tab's screen
//!   home/discover/…  → one function per tab (a real app would split these into files)
//!   post()/…         → small reusable pieces

use std::collections::HashSet;

use pebbles::prelude::*;

// ---------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------

fn app() -> impl IntoWidget {
    let tab = create_signal(0_usize);
    let title = ["Home", "Discover", "Profile"][tab.get().min(2)];

    let mut shell = scaffold(safe_area(body(tab.get())))
        .top(top_panel(title).action(icon_button(lucide::BELL).variant(ButtonVariant::Ghost)))
        .bottom(
            bottom_nav()
                .item(nav(lucide::HOUSE, "Home", 0, tab))
                .item(nav(lucide::COMPASS, "Discover", 1, tab))
                .item(nav(lucide::USER, "Profile", 2, tab)),
        );
    // A FAB only on the feed — Flutter's canonical bottom-right action.
    if tab.get() == 0 {
        shell = shell.fab(fab(lucide::PENCIL).on_pressed(|| {}));
    }
    shell
}

fn nav(icon: IconData, label: &str, index: usize, tab: Signal<usize>) -> BottomNavItem {
    bottom_nav_item(icon, label).selected(tab.get() == index).on_select(move || tab.set(index))
}

fn body(tab: usize) -> AnyWidget {
    match tab {
        0 => home().into_widget(),
        1 => discover().into_widget(),
        _ => profile().into_widget(),
    }
}

// ---------------------------------------------------------------------------
// Home — a scrolling feed of posts with an interactive like button
// ---------------------------------------------------------------------------

fn home() -> impl IntoWidget {
    // Which posts the user has liked (reactive — the hearts toggle live).
    let liked = create_signal(HashSet::<u32>::new());

    let posts = [
        (
            1_u32,
            "Ada Lovelace",
            "@ada",
            "Shipping a GUI in Rust that feels like Flutter — signals instead of setState. Wild how little ceremony there is.",
            palette::violet::S500,
        ),
        (
            2,
            "Grace Hopper",
            "@grace",
            "The whole app is a function returning a widget. State is a create_signal you read and write directly. That's it.",
            palette::sky::S500,
        ),
        (
            3,
            "Alan Turing",
            "@alan",
            "Bottom nav + a scaffold + a FAB — this is a real mobile shell, drawn on the GPU via Vello.",
            palette::emerald::S500,
        ),
    ];

    let feed: Vec<AnyWidget> = posts
        .iter()
        .map(|&(id, name, handle, text_body, color)| {
            post(id, name, handle, text_body, color, liked).into_widget()
        })
        .collect();

    scroll_view(
        column(feed).cross_axis_alignment(CrossAxisAlignment::Stretch).main_axis_size(MainAxisSize::Min),
    )
}

fn post(
    id: u32,
    name: &str,
    handle: &str,
    body: &str,
    color: Color,
    liked: Signal<HashSet<u32>>,
) -> impl IntoWidget {
    let c = theme().colors;
    let is_liked = liked.get().contains(&id);
    let likes = 12 + id * 7 + u32::from(is_liked);

    let header = row(children![
        avatar(initials(name)).color(color),
        gap_w(10.0),
        column(children![
            text(name.to_string()).size(14.5).semibold().color(c.foreground),
            text(handle.to_string()).size(12.5).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .main_axis_size(MainAxisSize::Min),
        spacer(),
        icon_button(lucide::ELLIPSIS).variant(ButtonVariant::Ghost),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    // Liked state shows in the heart's color (Lucide ships one heart glyph).
    let heart_color = if is_liked { palette::rose::S500 } else { c.muted_foreground };
    let actions = row(children![
        pressable(
            row(children![
                icon(lucide::HEART).size(18.0).color(heart_color),
                gap_w(6.0),
                text(format!("{likes}")).size(13.0).color(c.muted_foreground)
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center),
        )
        .radius(8.0)
        .on_tap(move || liked.update(|s| {
            if !s.insert(id) {
                s.remove(&id);
            }
        })),
        gap_w(20.0),
        icon(lucide::MESSAGE_CIRCLE).size(18.0).color(c.muted_foreground),
        gap_w(6.0),
        text(format!("{}", id * 3)).size(13.0).color(c.muted_foreground),
        spacer(),
        icon(lucide::BOOKMARK).size(18.0).color(c.muted_foreground),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center);

    container()
        .decoration(
            BoxDecoration::new()
                .color(c.card)
                .border(Border::new(c.border, 1.0))
                .radius(BorderRadius::all(14.0)),
        )
        .padding(EdgeInsets::all(14.0))
        .margin(EdgeInsets::only(14.0, 14.0, 14.0, 0.0))
        .child(
            column(children![
                header,
                gap_h(10.0),
                text(body.to_string()).size(14.0).line_height(1.45).color(c.foreground),
                gap_h(12.0),
                actions
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        )
}

// ---------------------------------------------------------------------------
// Discover — a search field + a chip row + a grid of topics
// ---------------------------------------------------------------------------

fn discover() -> impl IntoWidget {
    let c = theme().colors;
    let topics = [
        ("Rust", lucide::CODE, palette::orange::S500),
        ("Design", lucide::PALETTE, palette::violet::S500),
        ("GPU", lucide::ZAP, palette::amber::S500),
        ("Mobile", lucide::SMARTPHONE, palette::sky::S500),
        ("Layout", lucide::LAYERS, palette::emerald::S500),
        ("Motion", lucide::ACTIVITY, palette::rose::S500),
    ];
    let tiles: Vec<AnyWidget> = topics
        .iter()
        .map(|&(name, ic, color)| {
            container()
                .decoration(
                    BoxDecoration::new()
                        .color(c.card)
                        .border(Border::new(c.border, 1.0))
                        .radius(BorderRadius::all(14.0)),
                )
                .padding(EdgeInsets::all(16.0))
                .width(160.0)
                .child(
                    column(children![
                        icon(ic).size(24.0).color(color),
                        gap_h(10.0),
                        text(name.to_string()).size(15.0).semibold().color(c.foreground)
                    ])
                    .cross_axis_alignment(CrossAxisAlignment::Start)
                    .main_axis_size(MainAxisSize::Min),
                )
                .into_widget()
        })
        .collect();

    scroll_view(
        column(children![
            container()
                .padding(EdgeInsets::all(14.0))
                .child(text_field().kind(InputKind::Search).placeholder("Search Pebbles")),
            container()
                .padding(EdgeInsets::only(14.0, 0.0, 14.0, 6.0))
                .child(wrap(tiles).spacing(12.0).run_spacing(12.0),),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    )
}

// ---------------------------------------------------------------------------
// Profile — a header + stats + a settings list
// ---------------------------------------------------------------------------

fn profile() -> impl IntoWidget {
    let c = theme().colors;

    let header = column(children![
        avatar("RS").color(palette::violet::S500).size(72.0),
        gap_h(12.0),
        text("Reyco Seguma").size(20.0).semibold().color(c.foreground),
        text("Building Pebbles").size(13.5).color(c.muted_foreground),
        gap_h(16.0),
        row(children![stat("128", "Posts"), stat("4.2k", "Followers"), stat("312", "Following")])
            .main_axis_alignment(MainAxisAlignment::Center)
            .main_axis_size(MainAxisSize::Min),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
    .main_axis_size(MainAxisSize::Min);

    let settings = column(children![
        list_tile("Account")
            .leading(icon(lucide::USER).color(c.muted_foreground))
            .trailing(icon(lucide::CHEVRON_RIGHT).color(c.muted_foreground))
            .on_tap(|| {}),
        list_tile("Notifications")
            .leading(icon(lucide::BELL).color(c.muted_foreground))
            .trailing(icon(lucide::CHEVRON_RIGHT).color(c.muted_foreground))
            .on_tap(|| {}),
        list_tile("Appearance")
            .leading(icon(lucide::PALETTE).color(c.muted_foreground))
            .trailing(icon(lucide::CHEVRON_RIGHT).color(c.muted_foreground))
            .on_tap(|| {}),
        list_tile("Privacy")
            .leading(icon(lucide::LOCK).color(c.muted_foreground))
            .trailing(icon(lucide::CHEVRON_RIGHT).color(c.muted_foreground))
            .on_tap(|| {}),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Stretch)
    .main_axis_size(MainAxisSize::Min);

    scroll_view(
        column(children![
            container().padding(EdgeInsets::symmetric(16.0, 24.0)).child(header),
            container().padding(EdgeInsets::symmetric(8.0, 0.0)).child(settings),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn stat(value: &str, label: &str) -> impl IntoWidget {
    let c = theme().colors;
    container().padding(EdgeInsets::symmetric(14.0, 0.0)).child(
        column(children![
            text(value.to_string()).size(18.0).semibold().color(c.foreground),
            text(label.to_string()).size(12.5).color(c.muted_foreground),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Center)
        .main_axis_size(MainAxisSize::Min),
    )
}

fn initials(name: &str) -> String {
    name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect::<String>().to_uppercase()
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(app))
        .title("Pebbles — Mobile")
        .size(390, 844) // iPhone-ish; only the window size makes it "mobile" for now
        .run()
}
