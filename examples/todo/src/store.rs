//! The **todo store** — the app's state manager.
//!
//! It owns two pieces of global state (the list of todos and the current filter) as
//! app-scoped signals, and exposes **actions** (`add`, `toggle`, …) and **derived
//! reads** (`visible`, `remaining`). Components read the signals — and re-render when
//! they change — and call the actions; nothing outside this file touches the `Vec`.
//! That's the whole idea of a store: one place that knows how the data changes.

use std::cell::{Cell, RefCell};

use pebbles::prelude::*;

/// A single todo item.
#[derive(Clone)]
pub struct Todo {
    pub id: u64,
    pub text: String,
    pub done: bool,
}

/// Which todos the list shows.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Active,
    Done,
}

thread_local! {
    static ITEMS: RefCell<Option<Signal<Vec<Todo>>>> = const { RefCell::new(None) };
    static FILTER: RefCell<Option<Signal<Filter>>> = const { RefCell::new(None) };
    static NEXT_ID: Cell<u64> = const { Cell::new(3) }; // seed uses 1 and 2
}

/// The list signal (created once, seeded with a couple of items).
pub fn items() -> Signal<Vec<Todo>> {
    ITEMS.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_root_signal(seed()));
        }
        cell.unwrap()
    })
}

/// The active filter signal (created once).
pub fn filter() -> Signal<Filter> {
    FILTER.with(|cell| {
        let mut cell = cell.borrow_mut();
        if cell.is_none() {
            *cell = Some(create_root_signal(Filter::All));
        }
        cell.unwrap()
    })
}

fn seed() -> Vec<Todo> {
    vec![
        Todo { id: 1, text: "Try the Pebbles counter".into(), done: true },
        Todo { id: 2, text: "Read how this todo is structured".into(), done: false },
    ]
}

fn next_id() -> u64 {
    NEXT_ID.with(|c| {
        let id = c.get();
        c.set(id + 1);
        id
    })
}

// --- actions ----------------------------------------------------------------

/// Append a new (unfinished) todo. Blank input is ignored.
pub fn add(text: &str) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let todo = Todo { id: next_id(), text: text.to_string(), done: false };
    items().update(|v| v.push(todo));
}

/// Flip a todo's done state.
pub fn toggle(id: u64) {
    items().update(|v| {
        if let Some(t) = v.iter_mut().find(|t| t.id == id) {
            t.done = !t.done;
        }
    });
}

/// Delete a todo.
pub fn remove(id: u64) {
    items().update(|v| v.retain(|t| t.id != id));
}

/// Drop every completed todo.
pub fn clear_completed() {
    items().update(|v| v.retain(|t| !t.done));
}

/// Switch the visible filter.
pub fn set_filter(f: Filter) {
    filter().set(f);
}

// --- derived reads ----------------------------------------------------------

/// How many todos are still unfinished.
pub fn remaining() -> usize {
    items().get().iter().filter(|t| !t.done).count()
}

/// The todos to show under the current filter (subscribes to both signals).
pub fn visible() -> Vec<Todo> {
    let f = filter().get();
    items()
        .get()
        .into_iter()
        .filter(|t| match f {
            Filter::All => true,
            Filter::Active => !t.done,
            Filter::Done => t.done,
        })
        .collect()
}
