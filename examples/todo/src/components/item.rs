//! One todo row — a checkbox, the label (struck through when done), and a delete
//! button. It's a pure view of a `Todo`: interactions call the store's actions.

use pebbles::prelude::*;

use crate::store;
use crate::store::Todo;

pub fn item(todo: &Todo) -> impl IntoWidget {
    let c = theme().colors;
    let (id, done) = (todo.id, todo.done);

    let mut label = text(todo.text.clone()).size(15.0);
    label = if done { label.strikethrough().color(c.muted_foreground) } else { label.color(c.foreground) };

    row(children![
        checkbox(done).on_changed(move || store::toggle(id)),
        gap_w(12.0),
        Expanded::new(label),
        icon_button(IconKind::Close).variant(ButtonVariant::Ghost).on_pressed(move || store::remove(id)),
    ])
    .cross_axis_alignment(CrossAxisAlignment::Center)
}
