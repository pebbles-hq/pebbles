//! **Pebbles Atelier** — a demo storefront landing page, built to show Pebbles can
//! carry a real, image-heavy marketing site: a sliding hero over photography, glass
//! chrome, shop-by-category tiles, a product grid, a lifestyle split and a newsletter
//! card on a gradient — all GPU-rendered with Vello.

mod data;
mod sections;
mod ui;

use pebbles::prelude::*;

fn page() -> impl IntoWidget {
    scroll_view(
        column(children![
            sections::hero(),
            sections::categories(),
            sections::products(),
            sections::lifestyle(),
            sections::newsletter(),
            sections::footer(),
        ])
        .cross_axis_alignment(CrossAxisAlignment::Stretch)
        .main_axis_size(MainAxisSize::Min),
    )
    .drag_scroll(true)
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(page)).title("Pebbles Atelier").size(1200, 860).background(ui::paper()).run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pebbles_testing::Harness;

    /// The whole page mounts and produces substantial content (all the image-heavy
    /// sections render, not just a blank frame).
    #[test]
    fn landing_page_renders() {
        let mut h = Harness::new().window(1200.0, 860.0);
        h.mount(page);
        h.settle();
        h.draw();
        assert!(h.element_count() > 100, "landing page should render richly, got {}", h.element_count());
    }

    /// The framework renders centered text *actually centered*: a hero headline
    /// (`TextAlign::Center` in a centered column) sits within a few px of the window
    /// center — answering "can it render text properly?" with a measurable yes.
    #[test]
    fn hero_headline_is_horizontally_centered() {
        let mut h = Harness::new().window(1200.0, 860.0);
        h.mount(page);
        h.settle();
        h.draw();

        // The hero is a carousel: its three pages sit side by side (x = 0/1200/2400),
        // so headlines exist at centers 600/1800/3000. Scope to the *visible* first page
        // (center within the 1200px window) and take its widest paragraph — the headline.
        let (center_x, width) = h
            .find_all::<pebbles_render::RenderParagraph>()
            .into_iter()
            .map(|id| {
                let off = h.ui.render_tree().absolute_offset(id);
                let sz = h.size_of(id);
                (off.x + sz.width / 2.0, sz.width)
            })
            .filter(|&(cx, w)| w > 200.0 && (0.0..1200.0).contains(&cx))
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .expect("a wide headline paragraph on the visible page");

        assert!(
            (center_x - 600.0).abs() < 40.0,
            "hero headline should be centered (~600 in a 1200px window), got center {center_x} (w {width})"
        );
    }
}
