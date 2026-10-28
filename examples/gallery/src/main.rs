//! `cargo run -p gallery [-- --page <id>]` — the interactive gallery.

use pebbles::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--page" => {
                let id = args.next().unwrap_or_default();
                if gallery::page(&id).is_none() {
                    let ids: Vec<_> = gallery::PAGES.iter().map(|p| p.id).collect();
                    return Err(format!("unknown page `{id}`; one of: {}", ids.join(" ")).into());
                }
                gallery::shell::select(&id);
            }
            "--list" => {
                for p in gallery::PAGES {
                    println!("{:<26} {} / {}", p.id, p.group, p.title);
                }
                return Ok(());
            }
            other => {
                return Err(format!("unknown argument `{other}` (expected --page <id> or --list)").into());
            }
        }
    }
    App::new(component(gallery::app)).title("Pebbles gallery").size(1280, 860).run()
}
