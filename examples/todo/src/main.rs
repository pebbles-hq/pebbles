//! A todo list — add, toggle and remove items. Shows a reactive `Vec` in a
//! signal: every edit re-renders only this component, and the list rebuilds from
//! the current state. No stores, no reducers — just `signal.update(..)`.

use pebbles::prelude::*;

#[derive(Clone)]
struct Item {
    id: u64,
    text: String,
    done: bool,
}

fn todo() -> impl IntoWidget {
    let items = create_signal(Vec::<Item>::new());
    let draft = create_signal(String::new());
    let next_id = create_signal(1_u64);

    // Add the current draft as a new item (ignored when blank).
    let add = move || {
        let text = draft.peek().trim().to_string();
        if text.is_empty() {
            return;
        }
        let id = next_id.peek();
        next_id.set(id + 1);
        items.update(|v| v.push(Item { id, text, done: false }));
        draft.set(String::new());
    };

    // One row per item: a checkbox, the label (struck through when done), a remove.
    let rows: Vec<AnyWidget> = items
        .get()
        .iter()
        .map(|it| {
            let (id, done) = (it.id, it.done);
            let mut label = text(it.text.clone()).size(15.0);
            label = if done {
                label.strikethrough().color(palette::zinc::S400)
            } else {
                label.color(palette::zinc::S800)
            };
            row(children![
                checkbox(done).on_changed(move || {
                    items.update(|v| {
                        if let Some(x) = v.iter_mut().find(|x| x.id == id) {
                            x.done = !x.done;
                        }
                    })
                }),
                gap_w(10.0),
                Expanded::new(label),
                icon_button(IconKind::Close)
                    .variant(ButtonVariant::Ghost)
                    .on_pressed(move || items.update(|v| v.retain(|x| x.id != id))),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Center)
            .into_widget()
        })
        .collect();

    let left = items.get().iter().filter(|x| !x.done).count();

    center(
        container().width(420.0).child(
            column(children![
                text("Todo").size(22.0).semibold().color(palette::zinc::S900),
                gap_h(16.0),
                row(children![
                    Expanded::new(
                        text_field()
                            .placeholder("What needs doing?")
                            .value(draft.get())
                            .on_changed(move |s| draft.set(s.to_string()))
                            .on_submit(move |_| add()),
                    ),
                    gap_w(10.0),
                    button("Add").on_pressed(add),
                ]),
                gap_h(16.0),
                column(rows)
                    .cross_axis_alignment(CrossAxisAlignment::Stretch)
                    .main_axis_size(MainAxisSize::Min),
                gap_h(12.0),
                text(format!("{left} left")).size(13.0).color(palette::zinc::S500),
            ])
            .cross_axis_alignment(CrossAxisAlignment::Stretch)
            .main_axis_size(MainAxisSize::Min),
        ),
    )
}

#[pebbles::main]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    App::new(component(todo)).title("Pebbles — Todo").size(460, 560).background(palette::zinc::S50).run()
}
