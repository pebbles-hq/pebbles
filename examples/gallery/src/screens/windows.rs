use pebbles::prelude::*;

use crate::state;
use crate::ui::{doc, gap_w, screen};

/// The root of the secondary window. It's an ordinary component — it just reads the
/// SAME global signals the main window does, so everything stays in sync with no
/// serialization. Mounted in its own window (its own `Ui`), sharing the runtime.
fn counter_window() -> impl IntoWidget {
    let count = state::counter();
    let msg = state::ping().latest().unwrap_or_else(|| "—".into());
    Container::new().padding(EdgeInsets::all(22.0)).child(
        column(children![
            text("Counter window").size(18.0).semibold(),
            muted("A separate OS window sharing the app's reactive runtime."),
            SizedBox::spacer(0.0, 20.0),
            row(children![
                text(format!("Shared counter: {}", count.get())).size(15.0),
                gap_w(14.0),
                button("+1").size(ButtonSize::Sm).on_pressed(move || count.update(|c| *c += 1)),
            ])
            .min(),
            SizedBox::spacer(0.0, 16.0),
            muted(format!("Message from main: {msg}")),
        ])
        .start()
        .min(),
    )
}

pub fn windows() -> Element {
    let win = create_signal::<Option<WindowId>>(None);
    let draft = create_signal(String::from("Hello, window!"));

    screen(
        "Windows & IPC",
        "Open a second OS window that shares this app's reactive runtime. Cross-window state is just a shared signal or a typed Channel — no serialization, unlike Electron. winit stays hidden; you drive it all from Pebbles.",
        children![open_section(win), shared_section(), message_section(draft)],
    )
}

fn open_section(win: Signal<Option<WindowId>>) -> impl IntoWidget {
    doc(
        "Open a window",
        "window(content).title(..).size(..).open() spawns a real OS window and returns an id; close_window(id) closes it (so does its own control).",
        row(children![
            button("Open counter window").leading(IconKind::Plus).on_pressed(move || {
                if win.get().is_none() {
                    let id = window(component(counter_window))
                        .title("Counter")
                        .size(360, 280)
                        .on_close(move || win.set(None))
                        .open();
                    win.set(Some(id));
                }
            }),
            gap_w(10.0),
            button("Close window")
                .variant(ButtonVariant::Outline)
                .disabled(win.get().is_none())
                .on_pressed(move || {
                    if let Some(id) = win.get() {
                        close_window(id);
                        win.set(None);
                    }
                }),
        ])
        .min(),
    )
}

fn shared_section() -> impl IntoWidget {
    let count = state::counter();
    doc(
        "Shared state (IPC)",
        "This counter is one global signal. Increment it here or in the other window — both update live, instantly, because they read the same signal.",
        row(children![
            text(format!("Counter: {}", count.get())).size(16.0).semibold(),
            gap_w(14.0),
            button("+1").on_pressed(move || count.update(|c| *c += 1)),
            gap_w(8.0),
            button("Reset").variant(ButtonVariant::Ghost).on_pressed(move || count.set(0)),
        ])
        .min(),
    )
}

fn message_section(draft: Signal<String>) -> impl IntoWidget {
    doc(
        "Send a message (typed Channel)",
        "channel::<String>() is a typed cross-window bus. Send from here; the counter window shows the latest via .latest(). No (de)serialization — it's the same value.",
        column(children![
            row(children![
                text_field().bind(draft).width(260.0),
                gap_w(10.0),
                button("Send").leading(IconKind::ArrowRight).on_pressed(move || {
                    state::ping().send(draft.get());
                }),
            ])
            .min(),
            SizedBox::spacer(0.0, 10.0),
            muted(format!(
                "Last sent: {}",
                state::ping().latest().unwrap_or_else(|| "—".into())
            )),
        ])
        .start()
        .min(),
    )
}
