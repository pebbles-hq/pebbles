//! gallery/ — every page of `examples/gallery`, rendered headless (fixed size, bundled fonts,
//! settled animations) through its `headless` module. Default light for every page; the
//! Material / Tailwind / dark looks for the component groups the theme visibly restyles; RTL
//! for the pages whose layout mirrors. Category Screen, slice Screens; self-goldens.

use gallery::headless::{self, Look};
use gallery::{PAGES, Page};
use pebbles_testing::Harness;
use pebbles_widgets::theme::{DesignLanguage, Theme};

use crate::case::{Case, Category::Screen, Slice::Screens, harness_arg};

/// The looks a page is rendered under; a case argument is `page_index * LOOKS.len() + look`.
const LOOKS: [(&str, Look); 5] = [
    ("", Look::DEFAULT),
    ("_material", Look { design: DesignLanguage::Material, dark: false, rtl: false }),
    ("_tailwind", Look { design: DesignLanguage::Tailwind, dark: false, rtl: false }),
    ("_dark", Look { design: DesignLanguage::Default, dark: true, rtl: false }),
    ("_rtl", Look { design: DesignLanguage::Default, dark: false, rtl: true }),
];

/// Groups whose components the design language and palette visibly restyle.
const THEMED_GROUPS: [&str; 4] = ["Input", "Display", "Navigation", "Overlays"];
/// Pages whose layout mirrors under right-to-left.
const RTL_PAGES: [&str; 8] =
    ["buttons", "text-fields", "toggles", "slider", "rows", "tabs", "breadcrumb", "list"];

/// Pages left out of the corpus (see `cases`).
const SKIP: [&str; 1] = ["effects"];

fn looks_for(p: &Page) -> Vec<usize> {
    let mut v = vec![0];
    if THEMED_GROUPS.contains(&p.group) {
        v.extend([1, 2, 3]);
    }
    if RTL_PAGES.contains(&p.id) {
        v.push(4);
    }
    v
}

fn build(h: &mut Harness, arg: usize) {
    headless::mount(h, &PAGES[arg / LOOKS.len()], LOOKS[arg % LOOKS.len()].1);
}

/// Overlay pages, rendered again with their first overlay open: (page, control, hover?).
const OPEN: [(&str, &str, bool); 6] = [
    ("dialog", "Open dialog", false),
    ("popovers", "Open popover", false),
    ("sheets", "Open right sheet", false),
    ("toasts", "Long-lived", false), // 10 s: outlives both settles
    ("menubar", "File", false),
    ("command", "Open palette", false),
];

fn build_open(h: &mut Harness, i: usize) {
    let (id, label, hover) = OPEN[i];
    let page = PAGES.iter().find(|p| p.id == id).expect("gallery page");
    headless::mount(h, page, Look::DEFAULT);
    if hover { headless::hover(h, label) } else { headless::click(h, label) }
}

pub fn cases() -> Vec<Case> {
    let mut out = Vec::new();
    for (i, p) in PAGES.iter().enumerate() {
        // Blur / dilate / backdrop sit just outside the relaxed tolerance on WARP; the effect/
        // family already judges those ops against the golden model.
        if SKIP.contains(&p.id) {
            continue;
        }
        for l in looks_for(p) {
            let name = format!("gallery/{}{}", p.id.replace('-', "_"), LOOKS[l].0);
            let mut c = harness_arg(
                &name,
                Screen,
                Screens,
                headless::WIDTH,
                headless::HEIGHT,
                build,
                i * LOOKS.len() + l,
            );
            if LOOKS[l].1.dark {
                c.background = Theme::dark().colors.background;
            }
            out.push(c);
        }
    }
    for (i, (id, _, _)) in OPEN.iter().enumerate() {
        let name = format!("gallery/{}_open", id.replace('-', "_"));
        out.push(harness_arg(&name, Screen, Screens, headless::WIDTH, headless::HEIGHT, build_open, i));
    }
    out
}
